package io.github.yurisismotto.omnibridge.clipboard

import android.os.SystemClock
import android.util.Log
import com.google.protobuf.ByteString
import io.github.yurisismotto.omnibridge.identity.Fingerprint
import io.github.yurisismotto.omnibridge.proto.capabilities.ClipboardControl
import io.github.yurisismotto.omnibridge.proto.capabilities.ClipboardOutcome
import io.github.yurisismotto.omnibridge.proto.capabilities.ClipboardResult
import io.github.yurisismotto.omnibridge.proto.capabilities.ClipboardUpdate
import io.github.yurisismotto.omnibridge.proto.ErrorCode
import java.security.SecureRandom
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withTimeoutOrNull

/**
 * All `clipboard.v1` state and policy for this phone.
 *
 * The Kotlin counterpart of `ClipboardManager` in the Rust daemon, and it
 * follows the same rules for the same reasons:
 *
 *  * the grant and the per-peer policy are asked **freshly** on every
 *    question, never captured at handshake time, so a revocation is in force
 *    at once;
 *  * an event id is de-duplicated so a replay is idempotent;
 *  * content applied from a peer is suppressed so it cannot echo back;
 *  * a clip is never forwarded from one peer to another — there is no code
 *    path that does it;
 *  * nothing is written to disk, ever.
 *
 * ## What is different from the desktop, and why
 *
 * There is no local watcher. Android will not let an ordinary app read the
 * clipboard without focus, so a watcher would be a loop that reads null.
 * Sending is therefore always initiated by a person: [sendCurrentClipboard]
 * is called from a foreground Activity, and never from a service.
 *
 * ## Why every map here is keyed by hex, not by [Fingerprint]
 *
 * `Fingerprint` is a `@JvmInline value class` wrapping a `ByteArray`, so the
 * `equals` and `hashCode` Kotlin generates for it are the *array's* — which
 * are reference identity. Two `Fingerprint`s holding the same 32 bytes are
 * therefore unequal as map keys, and a lookup with a freshly decoded one
 * would silently miss: a peer would appear to have no session, no policy and
 * no pending clip. Keying on `toHex()` sidesteps that entirely, and it is why
 * the rest of the app compares fingerprints with `contentEquals` rather than
 * `==`.
 */
class ClipboardSync(
    private val systemClipboard: ClipboardTarget,
    /** Answers the grant and policy questions from the trust store. */
    private val authorizer: Authorizer,
    /** This device's own id, stamped into outbound updates. */
    private val localDeviceId: String,
    private val clock: Clock = Clock { SystemClock.elapsedRealtime() },
    private val random: SecureRandom = SecureRandom(),
) {

    /** The grant and policy source. Re-asked per operation, never cached. */
    fun interface Authorizer {
        /**
         * The effective policy for [peer] right now.
         *
         * Must return [ClipboardPolicy.DENIED] for a peer that is unknown,
         * forgotten, or has no `clipboard.v1` grant — the grant check and the
         * policy lookup are one call so a caller cannot do one and forget the
         * other.
         */
        fun policyFor(peer: Fingerprint): ClipboardPolicy
    }

    /** A clip accepted but not applied, because `autoReceive` is off. */
    private class PendingClip(
        /** Kept alongside the hex key so a report can name the peer. */
        val peer: Fingerprint,
        val text: ClipboardText,
        val sensitive: Boolean,
        val originDeviceId: String,
        val peerName: String,
        val receivedAt: Long,
    )

    /** A pending clip described without its content. Safe to display. */
    data class PendingClipInfo(
        val peer: Fingerprint,
        val peerName: String,
        val bytes: Int,
        val hashPrefix: String,
        val sensitive: Boolean,
        val ageMillis: Long,
    )

    /** What happened to one inbound update. Mirrors the protobuf enum. */
    enum class Outcome(val proto: ClipboardOutcome) {
        APPLIED(ClipboardOutcome.CLIPBOARD_OUTCOME_APPLIED),
        PENDING_USER(ClipboardOutcome.CLIPBOARD_OUTCOME_PENDING_USER),
        DUPLICATE(ClipboardOutcome.CLIPBOARD_OUTCOME_DUPLICATE),
        NOT_AUTHORIZED(ClipboardOutcome.CLIPBOARD_OUTCOME_NOT_AUTHORIZED),
        REJECTED_POLICY(ClipboardOutcome.CLIPBOARD_OUTCOME_REJECTED_POLICY),
        REJECTED_SENSITIVE(ClipboardOutcome.CLIPBOARD_OUTCOME_REJECTED_SENSITIVE),
        TOO_LARGE(ClipboardOutcome.CLIPBOARD_OUTCOME_TOO_LARGE),
        INVALID_TEXT(ClipboardOutcome.CLIPBOARD_OUTCOME_INVALID_TEXT),
        FAILED(ClipboardOutcome.CLIPBOARD_OUTCOME_FAILED),
    }

    /** Why a send did not happen. Local only: never sent to a peer. */
    sealed interface SendFailure {
        data object NotPermitted : SendFailure
        data object NotConnected : SendFailure
        data class CannotRead(val failure: ClipboardTarget.ReadFailure) : SendFailure

        /** The clip is marked sensitive and the person has not confirmed. */
        data class NeedsConfirmation(val bytes: Int) : SendFailure

        fun describe(): String = when (this) {
            NotPermitted ->
                "This computer is not allowed to receive your clipboard. " +
                    "Turn on \"Send clipboard to this computer\" first."
            NotConnected -> "Not connected to that computer."
            is CannotRead -> failure.describe()
            is NeedsConfirmation -> "This clipboard is marked sensitive."
        }
    }

    private val mutex = Mutex()
    private val events = EventCache(clock = clock)
    private val suppression = SuppressionCache(clock = clock)
    /** Keyed by `fingerprint.toHex()`; see the class docs. */
    private val pending = LinkedHashMap<String, PendingClip>()

    /**
     * Live sessions, for sends this device starts. Keyed by hex.
     *
     * The lambda answers the **envelope message id** the session minted for
     * the frame. That id is the only thing a generic `Error` can be correlated
     * back to — `Envelope.correlation_id` is set to it by a peer that refuses
     * the capability — so throwing it away, as this used to, is what left
     * `ERROR_CODE_UNSUPPORTED_CAPABILITY` a log line nobody could attribute.
     */
    private val sessions = LinkedHashMap<String, suspend (ByteString) -> ByteString>()

    /** One clipboard send that has left and has not been answered. */
    private class Outstanding(
        val peerHex: String,
        val bytes: Int,
        /**
         * The session generation this send belongs to.
         *
         * Compared on every resolution so that a verdict produced by a
         * previous session cannot settle a send made by the current one.
         * Session teardown already settles and removes the entries, so this
         * is the belt to that braces — and it is what makes "no stale result
         * from a previous session" a check rather than a consequence.
         */
        val epoch: Long,
        val settled: CompletableDeferred<ClipboardDelivery>,
    )

    /**
     * Sends awaiting a verdict, keyed by `eventId` hex.
     *
     * Keyed by the **protocol's own** identifier, which is the only safe
     * choice: `ClipboardResult.event_id` echoes it, so a result can resolve
     * exactly the send it is about. Correlating by peer name, byte count,
     * timestamp or arrival order would all mean a verdict for one clip could
     * settle another.
     */
    private val outstanding = LinkedHashMap<String, Outstanding>()

    /**
     * `envelope.message_id` hex to `eventId` hex, for the frames above.
     *
     * The second index exists because the two refusal paths speak different
     * languages. A peer that negotiated `clipboard.v1` answers in
     * `clipboard.v1` and names the `event_id`; a peer that did **not**
     * negotiate it answers with a transport `Error` and can only name the
     * envelope it is replying to. Both are real correlation identities and
     * both are on the wire already.
     */
    private val byMessageId = LinkedHashMap<String, String>()

    /** Per-peer session generation. Bumped on every attach. */
    private val epochs = LinkedHashMap<String, Long>()

    private val _pendingClips = MutableStateFlow<List<PendingClipInfo>>(emptyList())

    /**
     * Clips waiting for the person, for the UI to observe.
     *
     * A `StateFlow` rather than a value the UI polls: the previous release's
     * device card read trust state once and never recomposed, so a change
     * made in one place was invisible in another. Anything the UI must react
     * to is observable here.
     */
    val pendingClips: StateFlow<List<PendingClipInfo>> = _pendingClips.asStateFlow()

    private val _lastDelivery = MutableStateFlow<Map<String, ClipboardDelivery>>(emptyMap())

    /**
     * What is known about the last clipboard sent to each computer, keyed by
     * `fingerprint.toHex()`.
     *
     * The state authority for the UI. A toast is a one-shot announcement that
     * can be missed and cannot be corrected; this is the value a screen draws,
     * and it moves from [ClipboardDelivery.Enqueued] to a settled state as the
     * peer answers. It never holds clipboard text — `ClipboardDelivery` has no
     * field that could.
     */
    val lastDelivery: StateFlow<Map<String, ClipboardDelivery>> = _lastDelivery.asStateFlow()

    /** Called when a clip arrives that needs the person to act. */
    var onClipPending: ((PendingClipInfo) -> Unit)? = null

    // -----------------------------------------------------------------------
    // Sessions
    // -----------------------------------------------------------------------

    suspend fun attachSession(peer: Fingerprint, send: suspend (ByteString) -> ByteString) {
        val hex = peer.toHex()
        mutex.withLock {
            sessions[hex] = send
            // A new generation. Anything still outstanding from the previous
            // one belongs to a session that is gone and can never be resolved
            // by this one.
            epochs[hex] = (epochs[hex] ?: 0L) + 1L
        }
    }

    /**
     * Forgets a peer's session and everything held on its behalf.
     *
     * The pending clip goes with it: a clip from a computer that is no longer
     * connected, which the person never applied, has no reason to stay in
     * memory.
     *
     * ## Outstanding sends are settled, not dropped
     *
     * A clipboard that left and was never answered is
     * [ClipboardDelivery.Unconfirmed], and the screen has to say so. Dropping
     * the entry silently would leave a caller waiting for a verdict that can
     * no longer arrive, and would leave the last thing the person saw —
     * "waiting for confirmation" — standing forever over a dead session.
     */
    suspend fun detachSession(peer: Fingerprint) {
        val hex = peer.toHex()
        val stranded = mutex.withLock {
            sessions.remove(hex)
            pending.remove(hex)
            val mine = outstanding.entries.filter { it.value.peerHex == hex }
            mine.forEach { (eventHex, _) -> outstanding.remove(eventHex) }
            byMessageId.entries.removeAll { (_, eventHex) ->
                mine.any { it.key == eventHex }
            }
            mine.map { it.value }
        }
        for (entry in stranded) {
            settle(
                entry,
                ClipboardDelivery.Unconfirmed(
                    ClipboardDelivery.Unconfirmed.Reason.DISCONNECTED,
                    entry.bytes,
                ),
            )
        }
        publishPending()
    }

    suspend fun isConnected(peer: Fingerprint): Boolean =
        mutex.withLock { sessions.containsKey(peer.toHex()) }

    // -----------------------------------------------------------------------
    // Inbound
    // -----------------------------------------------------------------------

    /**
     * Handles one `clipboard.v1` payload from [peer].
     *
     * Returns the reply to send, or null when there is nothing coherent to
     * answer. A `ClipboardResult` never produces a reply of its own —
     * answering an answer is how two correct peers build a message loop out
     * of nothing.
     */
    suspend fun handleControl(
        peer: Fingerprint,
        peerDeviceId: String,
        peerName: String,
        payload: ByteString,
    ): ByteString? {
        val control = try {
            ClipboardControl.parseFrom(payload)
        } catch (e: Exception) {
            // Never log the payload: it is clipboard content.
            Log.w(TAG, "malformed clipboard payload: ${e.javaClass.simpleName}")
            throw IllegalArgumentException("malformed clipboard.v1 payload")
        }

        return when (control.bodyCase) {
            ClipboardControl.BodyCase.UPDATE -> {
                val update = control.update
                val outcome = handleUpdate(peer, peerDeviceId, peerName, update)
                Log.i(
                    TAG,
                    "clipboard update from ${peer.toDisplayShort()} " +
                        "event=${Redact.eventPrefix(update.eventId.toByteArray())} " +
                        "outcome=$outcome",
                )
                if (update.eventId.size() != ClipboardLimits.EVENT_ID_LENGTH) {
                    // Nothing to correlate an answer with, and echoing an
                    // attacker-chosen id back is not useful either.
                    null
                } else {
                    encodeResult(update.eventId, outcome)
                }
            }

            ClipboardControl.BodyCase.RESULT -> {
                val result = control.result
                val outcome = Outcome.entries.firstOrNull { it.proto == result.outcome }
                    ?: Outcome.FAILED
                // Resolved by the event id the peer echoed, and only for the
                // peer the send was actually made to. A result naming an id
                // this session did not mint resolves nothing at all — which
                // is what stops a verdict for one clip settling another, and
                // what stops a peer influencing a send made to a different
                // computer.
                resolveByEventId(peer, result.eventId.toByteArray(), outcome)
                null
            }

            // A body this build does not know, or none. Not fatal: a newer
            // peer must not break this one.
            else -> null
        }
    }

    /**
     * The decision path for one inbound update.
     *
     * The order is deliberate and matches the desktop's: authorization first,
     * de-duplication only after validation, so a malformed replay cannot
     * consume a cache slot.
     */
    private suspend fun handleUpdate(
        peer: Fingerprint,
        peerDeviceId: String,
        peerName: String,
        update: ClipboardUpdate,
    ): Outcome {
        // 1. Grant, asked fresh.
        val policy = authorizer.policyFor(peer)
        if (!policy.allowSend && !policy.allowReceive) return Outcome.NOT_AUTHORIZED

        // 2. Direction.
        if (!policy.allowReceive) return Outcome.REJECTED_POLICY

        // 3. Shape.
        if (update.eventId.size() != ClipboardLimits.EVENT_ID_LENGTH) return Outcome.INVALID_TEXT

        // 4. Size, before anything is copied anywhere.
        val rawBytes = update.textUtf8.toByteArray(Charsets.UTF_8).size
        if (rawBytes > ClipboardLimits.MAX_TEXT_BYTES) return Outcome.TOO_LARGE

        val text = ClipboardText.validate(update.textUtf8).getOrElse { failure ->
            val rejection = (failure as? ClipboardRejected)?.rejection
            return if (rejection is TextRejection.TooLarge) Outcome.TOO_LARGE
            else Outcome.INVALID_TEXT
        }

        // 5. The hash is a consistency check on a peer, not authentication.
        //    A mismatch means the sender is broken or trying something, and
        //    either way the (hash, content) pair it asked us to remember
        //    would poison de-duplication.
        val declaredHash = update.contentHash.toByteArray()
        if (declaredHash.isNotEmpty() && !declaredHash.contentEquals(text.hash)) {
            return Outcome.INVALID_TEXT
        }

        // 6. De-duplication and replay.
        val eventId = update.eventId.toByteArray()
        val fresh = mutex.withLock { events.admit(eventId) }
        if (!fresh) return Outcome.DUPLICATE

        // 7. Automation.
        val origin = update.originDeviceId.ifEmpty { peerDeviceId }
        if (!policy.mayAutoReceive()) {
            val info = mutex.withLock {
                val clip = PendingClip(
                    peer = peer,
                    text = text,
                    sensitive = update.sensitiveHint,
                    originDeviceId = origin,
                    peerName = peerName,
                    receivedAt = clock.nowMillis(),
                )
                pending[peer.toHex()] = clip
                describe(peer, clip)
            }
            publishPending()
            onClipPending?.invoke(info)
            return Outcome.PENDING_USER
        }

        return apply(text, update.sensitiveHint, origin)
    }

    /**
     * Writes a remote clip to the system clipboard, arming loop suppression
     * first.
     *
     * The order matters and is the whole mechanism: the suppression entry has
     * to exist before the write, because on a platform that reports clipboard
     * changes the notification can arrive while `setPrimaryClip` is still
     * returning.
     */
    private suspend fun apply(
        text: ClipboardText,
        sensitive: Boolean,
        origin: String,
    ): Outcome {
        mutex.withLock { suppression.arm(text.hash, origin) }

        return when (val result = systemClipboard.write(text, sensitive)) {
            is ClipboardTarget.WriteResult.Applied -> Outcome.APPLIED
            is ClipboardTarget.WriteResult.Failed -> {
                // The write did not happen, so no echo will either. Releasing
                // the entry keeps a genuine local copy of the same text from
                // being swallowed later.
                mutex.withLock { suppression.take(text.hash) }
                Log.w(TAG, "could not apply a clipboard update: ${result.reason}")
                Outcome.FAILED
            }
        }
    }

    // -----------------------------------------------------------------------
    // Pending clips
    // -----------------------------------------------------------------------

    /**
     * Applies the clip a computer sent while `autoReceive` was off.
     *
     * The grant is re-checked here, not only when the clip arrived: a clip
     * accepted a minute ago from a computer that has since been forgotten
     * must not still be applicable.
     */
    suspend fun applyPending(peer: Fingerprint): Result<Int> {
        val policy = authorizer.policyFor(peer)
        if (!policy.allowReceive) {
            return Result.failure(ClipboardSendFailed(SendFailure.NotPermitted))
        }

        val clip = mutex.withLock {
            val clip = pending.remove(peer.toHex())
            if (clip == null ||
                clock.nowMillis() - clip.receivedAt >= ClipboardLimits.PENDING_CLIP_TTL_MS
            ) {
                null
            } else {
                clip
            }
        }
        publishPending()

        if (clip == null) {
            return Result.failure(
                ClipboardSendFailed(SendFailure.CannotRead(ClipboardTarget.ReadFailure.Empty)),
            )
        }

        return when (apply(clip.text, clip.sensitive, clip.originDeviceId)) {
            Outcome.APPLIED -> Result.success(clip.text.byteLength)
            else -> Result.failure(
                ClipboardSendFailed(
                    SendFailure.CannotRead(ClipboardTarget.ReadFailure.NotAllowed),
                ),
            )
        }
    }

    /** Discards a held clip without applying it. */
    suspend fun dismissPending(peer: Fingerprint) {
        mutex.withLock { pending.remove(peer.toHex()) }
        publishPending()
    }

    private fun describe(peer: Fingerprint, clip: PendingClip) = PendingClipInfo(
        peer = peer,
        peerName = clip.peerName,
        bytes = clip.text.byteLength,
        hashPrefix = Redact.hashPrefix(clip.text.hash),
        sensitive = clip.sensitive,
        ageMillis = clock.nowMillis() - clip.receivedAt,
    )

    private suspend fun publishPending() {
        val now = clock.nowMillis()
        val live = mutex.withLock {
            pending.entries.removeAll { (_, clip) ->
                now - clip.receivedAt >= ClipboardLimits.PENDING_CLIP_TTL_MS
            }
            pending.values.map { clip -> describe(clip.peer, clip) }
        }
        _pendingClips.value = live
    }

    // -----------------------------------------------------------------------
    // Outbound
    // -----------------------------------------------------------------------

    /**
     * Reads the clipboard and sends it to one computer.
     *
     * **Must be called while OmniBridge is in the foreground.** That is not a
     * style preference: Android returns null from `getPrimaryClip` otherwise,
     * and the failure is reported as [SystemClipboard.ReadFailure.NotAllowed]
     * so the person is told why rather than shown an empty result.
     *
     * @param confirmedSensitive set only after the person has answered the
     *   "this clipboard is marked sensitive" prompt. Without it, a sensitive
     *   clip is refused with [SendFailure.NeedsConfirmation] rather than
     *   leaving the device silently.
     */
    suspend fun sendCurrentClipboard(
        peer: Fingerprint,
        confirmedSensitive: Boolean = false,
    ): Result<SendReceipt> {
        val policy = authorizer.policyFor(peer)
        if (!policy.allowSend) {
            return Result.failure(ClipboardSendFailed(SendFailure.NotPermitted))
        }

        val clip = systemClipboard.read().getOrElse { failure ->
            val readFailure = (failure as? ClipboardReadFailed)?.failure
                ?: ClipboardTarget.ReadFailure.NotAllowed
            return Result.failure(ClipboardSendFailed(SendFailure.CannotRead(readFailure)))
        }

        // A clip the platform marked sensitive never leaves the device on the
        // strength of one tap. The person is asked again, with the computer
        // named, and only their answer sends it.
        if (clip.sensitive && !confirmedSensitive) {
            return Result.failure(
                ClipboardSendFailed(SendFailure.NeedsConfirmation(clip.text.byteLength)),
            )
        }

        return sendText(peer, clip.text, clip.sensitive)
    }

    /**
     * Sends explicit text, bypassing the system clipboard.
     *
     * Answers a [SendReceipt], **not** a success. The frame is on the session
     * and nothing more is known yet; [SendReceipt.awaitVerdict] is how a
     * caller finds out what the peer did with it, and
     * [ClipboardDelivery.Enqueued] is what the state says until it does.
     *
     * This return type is the correction to GitHub #8. The old one was
     * `Result<Int>`, and `Result.success(42)` from here read at every call
     * site as "42 bytes arrived" when all it ever meant was "42 bytes were
     * handed to the writer".
     */
    suspend fun sendText(
        peer: Fingerprint,
        text: ClipboardText,
        sensitive: Boolean,
    ): Result<SendReceipt> {
        val policy = authorizer.policyFor(peer)
        if (!policy.allowSend) {
            return Result.failure(ClipboardSendFailed(SendFailure.NotPermitted))
        }

        val hex = peer.toHex()
        val (send, epoch) = mutex.withLock {
            val session = sessions[hex] ?: return@withLock null
            session to (epochs[hex] ?: 0L)
        } ?: return Result.failure(ClipboardSendFailed(SendFailure.NotConnected))

        val eventId = randomEventId()
        val update = ClipboardControl.newBuilder()
            .setUpdate(
                ClipboardUpdate.newBuilder()
                    .setEventId(ByteString.copyFrom(eventId))
                    .setOriginDeviceId(localDeviceId)
                    .setTextUtf8(text.text)
                    .setContentHash(ByteString.copyFrom(text.hash))
                    .setSensitiveHint(sensitive)
                    .setTimestampUnixMs(System.currentTimeMillis()),
            )
            .build()

        val eventHex = eventId.toHexString()
        val entry = Outstanding(
            peerHex = hex,
            bytes = text.byteLength,
            epoch = epoch,
            settled = CompletableDeferred(),
        )
        // Registered *before* the write. A peer on the same LAN can answer
        // while `send` is still returning, and an entry registered afterwards
        // would miss that verdict and report the clip as unconfirmed.
        mutex.withLock {
            outstanding[eventHex] = entry
            evictOldestLocked()
        }

        val messageId = try {
            send(update.toByteString())
        } catch (e: Exception) {
            Log.w(TAG, "could not send a clipboard update: ${e.javaClass.simpleName}")
            mutex.withLock {
                outstanding.remove(eventHex)
                byMessageId.entries.removeAll { it.value == eventHex }
            }
            return Result.failure(ClipboardSendFailed(SendFailure.NotConnected))
        }

        mutex.withLock { byMessageId[messageId.toByteArray().toHexString()] = eventHex }

        // An event prefix, a count and a flag. Never the text, never the hash
        // in full, never the peer's name.
        Log.i(
            TAG,
            "clipboard update sent to ${peer.toDisplayShort()} " +
                "event=${Redact.eventPrefix(eventId)} bytes=${text.byteLength} " +
                "sensitive=$sensitive",
        )
        publishDelivery(hex, ClipboardDelivery.Enqueued(text.byteLength))
        return Result.success(SendReceipt(eventHex, text.byteLength, entry.settled))
    }

    /**
     * A clipboard that has left, and the means to learn what became of it.
     *
     * Deliberately not a `Result`: there is no success to report yet, and a
     * type that looked like one is how the defect was written in the first
     * place.
     */
    class SendReceipt internal constructor(
        /** The protocol event id, hex. The correlation key, and public. */
        val eventIdHex: String,
        val bytes: Int,
        private val settled: CompletableDeferred<ClipboardDelivery>,
    ) {
        /** What is known right now, before any verdict. */
        val enqueued: ClipboardDelivery get() = ClipboardDelivery.Enqueued(bytes)

        /**
         * Waits for the peer's verdict, for at most [timeoutMs].
         *
         * A timeout is [ClipboardDelivery.Unconfirmed], never a failure and
         * never a success: the clip may have arrived and this end cannot tell.
         * The wait does *not* cancel the send or forget the correlation — a
         * verdict that arrives late still settles the state the screen draws.
         */
        suspend fun awaitVerdict(timeoutMs: Long): ClipboardDelivery =
            withTimeoutOrNull(timeoutMs) { settled.await() }
                ?: ClipboardDelivery.Unconfirmed(
                    ClipboardDelivery.Unconfirmed.Reason.TIMED_OUT,
                    bytes,
                )
    }

    // -----------------------------------------------------------------------
    // Verdicts
    // -----------------------------------------------------------------------

    /**
     * A peer answered in `clipboard.v1`, naming the event it is about.
     *
     * Three things have to agree before anything is settled: the event id is
     * one this device minted, the peer answering is the peer it was sent to,
     * and the session is the one it was sent on. Any of the three failing
     * means the result belongs to something else and is discarded.
     */
    private suspend fun resolveByEventId(peer: Fingerprint, eventId: ByteArray, outcome: Outcome) {
        val hex = peer.toHex()
        val eventHex = eventId.toHexString()
        val entry = mutex.withLock {
            val candidate = outstanding[eventHex] ?: return@withLock null
            // Not ours to settle. Left in place rather than removed: the real
            // peer's answer must still be able to resolve it.
            if (candidate.peerHex != hex) return@withLock null
            if (candidate.epoch != (epochs[hex] ?: 0L)) return@withLock null
            outstanding.remove(eventHex)
            byMessageId.entries.removeAll { it.value == eventHex }
            candidate
        }
        if (entry == null) {
            // Common and harmless: a duplicate result, or one for a send that
            // the session teardown already settled. Logged without the id.
            Log.d(TAG, "clipboard result did not match an outstanding send")
            return
        }
        Log.d(TAG, "peer reported clipboard outcome: $outcome")
        settle(entry, ClipboardDelivery.of(outcome, entry.bytes))
    }

    /**
     * The peer refused the capability rather than the clip.
     *
     * The transport hands this on with `Envelope.correlation_id`, which a
     * refusing peer sets to the `message_id` of the frame it is refusing.
     * That is a real protocol identity, minted here, unguessable and unique —
     * so matching on it is safe in a way that matching on a timestamp or a
     * byte count never would be. A correlation id this device did not mint
     * matches nothing and is ignored.
     */
    suspend fun onPeerRefusal(peer: Fingerprint, correlationId: ByteString, code: ErrorCode) {
        val hex = peer.toHex()
        val messageHex = correlationId.toByteArray().toHexString()
        val entry = mutex.withLock {
            val eventHex = byMessageId[messageHex] ?: return@withLock null
            val candidate = outstanding[eventHex] ?: return@withLock null
            if (candidate.peerHex != hex) return@withLock null
            if (candidate.epoch != (epochs[hex] ?: 0L)) return@withLock null
            outstanding.remove(eventHex)
            byMessageId.remove(messageHex)
            candidate
        } ?: return

        val refusal = when (code) {
            ErrorCode.ERROR_CODE_UNSUPPORTED_CAPABILITY -> ClipboardDelivery.Refusal.NOT_NEGOTIATED
            ErrorCode.ERROR_CODE_NOT_AUTHORIZED -> ClipboardDelivery.Refusal.NOT_AUTHORIZED
            ErrorCode.ERROR_CODE_RATE_LIMITED -> ClipboardDelivery.Refusal.RATE_LIMITED
            else -> ClipboardDelivery.Refusal.OTHER
        }
        // The code, not the peer's message: `Error.message` is a free string
        // from the other end of the wire.
        Log.i(TAG, "peer refused a clipboard frame: $refusal")
        settle(entry, ClipboardDelivery.Refused(refusal, entry.bytes))
    }

    private fun settle(entry: Outstanding, delivery: ClipboardDelivery) {
        entry.settled.complete(delivery)
        publishDelivery(entry.peerHex, delivery)
    }

    private fun publishDelivery(peerHex: String, delivery: ClipboardDelivery) {
        _lastDelivery.value = _lastDelivery.value + (peerHex to delivery)
    }

    /**
     * Keeps the outstanding map bounded.
     *
     * A peer that answers nothing must not be able to grow this without end.
     * The oldest entry is settled as unconfirmed rather than dropped, so a
     * caller waiting on it is answered rather than left. Called under [mutex].
     */
    private fun evictOldestLocked() {
        while (outstanding.size > MAX_OUTSTANDING_SENDS) {
            val oldest = outstanding.entries.first()
            outstanding.remove(oldest.key)
            byMessageId.entries.removeAll { it.value == oldest.key }
            oldest.value.settled.complete(
                ClipboardDelivery.Unconfirmed(
                    ClipboardDelivery.Unconfirmed.Reason.TIMED_OUT,
                    oldest.value.bytes,
                ),
            )
        }
    }

    /** Outstanding sends, for tests and diagnostics. No content. */
    suspend fun outstandingSends(): Int = mutex.withLock { outstanding.size }

    /** Cache sizes, so the bounded-growth property is observable. */
    suspend fun cacheSizes(): Pair<Int, Int> =
        mutex.withLock { events.size() to suppression.size() }

    private fun randomEventId(): ByteArray =
        ByteArray(ClipboardLimits.EVENT_ID_LENGTH).also(random::nextBytes)

    private fun encodeResult(eventId: ByteString, outcome: Outcome): ByteString =
        ClipboardControl.newBuilder()
            .setResult(
                ClipboardResult.newBuilder()
                    .setEventId(eventId)
                    .setOutcome(outcome.proto),
            )
            .build()
            .toByteString()

    private fun ByteArray.toHexString(): String =
        joinToString("") { "%02x".format(it) }

    companion object {
        private const val TAG = "ClipboardSync"

        /**
         * How many sends may await a verdict at once.
         *
         * The protocol does not serialise clipboard operations — nothing in
         * `clipboard_v1.proto` says a sender must wait — so this is a real
         * bound rather than a theoretical one. It is generous for a capability
         * a person drives by hand, and small enough that a peer which answers
         * nothing cannot cost this process memory.
         */
        private const val MAX_OUTSTANDING_SENDS = 32
    }
}

/** Carries a [ClipboardSync.SendFailure] through a [Result]. */
class ClipboardSendFailed(val failure: ClipboardSync.SendFailure) :
    Exception(failure.describe())
