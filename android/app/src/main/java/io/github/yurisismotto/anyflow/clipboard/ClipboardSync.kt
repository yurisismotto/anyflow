package io.github.yurisismotto.anyflow.clipboard

import android.os.SystemClock
import android.util.Log
import com.google.protobuf.ByteString
import io.github.yurisismotto.anyflow.identity.Fingerprint
import io.github.yurisismotto.anyflow.proto.capabilities.ClipboardControl
import io.github.yurisismotto.anyflow.proto.capabilities.ClipboardOutcome
import io.github.yurisismotto.anyflow.proto.capabilities.ClipboardResult
import io.github.yurisismotto.anyflow.proto.capabilities.ClipboardUpdate
import java.security.SecureRandom
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock

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

    /** Live sessions, for sends this device starts. Keyed by hex. */
    private val sessions = LinkedHashMap<String, suspend (ByteString) -> Unit>()

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

    private val _lastOutcome = MutableStateFlow<Map<String, Outcome>>(emptyMap())

    /**
     * The last verdict each computer reported for something we sent it,
     * keyed by `fingerprint.toHex()`.
     */
    val lastOutcome: StateFlow<Map<String, Outcome>> = _lastOutcome.asStateFlow()

    /** Called when a clip arrives that needs the person to act. */
    var onClipPending: ((PendingClipInfo) -> Unit)? = null

    // -----------------------------------------------------------------------
    // Sessions
    // -----------------------------------------------------------------------

    suspend fun attachSession(peer: Fingerprint, send: suspend (ByteString) -> Unit) {
        mutex.withLock { sessions[peer.toHex()] = send }
    }

    /**
     * Forgets a peer's session and everything held on its behalf.
     *
     * The pending clip goes with it: a clip from a computer that is no longer
     * connected, which the person never applied, has no reason to stay in
     * memory.
     */
    suspend fun detachSession(peer: Fingerprint) {
        mutex.withLock {
            sessions.remove(peer.toHex())
            pending.remove(peer.toHex())
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
                _lastOutcome.value = _lastOutcome.value + (peer.toHex() to outcome)
                Log.d(TAG, "peer reported clipboard outcome: $outcome")
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
     * **Must be called while AnyFlow is in the foreground.** That is not a
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
    ): Result<Int> {
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

    /** Sends explicit text, bypassing the system clipboard. */
    suspend fun sendText(
        peer: Fingerprint,
        text: ClipboardText,
        sensitive: Boolean,
    ): Result<Int> {
        val policy = authorizer.policyFor(peer)
        if (!policy.allowSend) {
            return Result.failure(ClipboardSendFailed(SendFailure.NotPermitted))
        }

        val send = mutex.withLock { sessions[peer.toHex()] }
            ?: return Result.failure(ClipboardSendFailed(SendFailure.NotConnected))

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

        return try {
            send(update.toByteString())
            Log.i(
                TAG,
                "clipboard update sent to ${peer.toDisplayShort()} " +
                    "event=${Redact.eventPrefix(eventId)} bytes=${text.byteLength} " +
                    "sensitive=$sensitive",
            )
            Result.success(text.byteLength)
        } catch (e: Exception) {
            Log.w(TAG, "could not send a clipboard update: ${e.javaClass.simpleName}")
            Result.failure(ClipboardSendFailed(SendFailure.NotConnected))
        }
    }

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

    companion object {
        private const val TAG = "ClipboardSync"
    }
}

/** Carries a [ClipboardSync.SendFailure] through a [Result]. */
class ClipboardSendFailed(val failure: ClipboardSync.SendFailure) :
    Exception(failure.describe())
