package io.github.yurisismotto.omnibridge.ui

import io.github.yurisismotto.omnibridge.OmniBridgeApp
import io.github.yurisismotto.omnibridge.capability.ClipboardCapability
import io.github.yurisismotto.omnibridge.capability.FilesCapability
import io.github.yurisismotto.omnibridge.clipboard.ClipboardCapabilities
import io.github.yurisismotto.omnibridge.clipboard.ClipboardPolicy
import io.github.yurisismotto.omnibridge.files.FileTransferManager
import io.github.yurisismotto.omnibridge.files.TransferState
import io.github.yurisismotto.omnibridge.proto.Platform
import io.github.yurisismotto.omnibridge.store.TrustStore
import io.github.yurisismotto.omnibridge.ui.theme.OmniBridgeStatus

/**
 * The decisions the UI makes about what to show and what to allow.
 *
 * Kept out of the composables on purpose. These are the rules that actually
 * matter — whether a send button is live, whether a policy is inert, what a
 * connection state is *called* — and pulling them into plain functions means
 * they can be tested on the JVM in milliseconds instead of on a device whose
 * instrumented run destroys the pairing every time.
 *
 * Nothing here decides policy. It reads the grant and the policy the trust
 * store already holds and answers what the screen should do about them; a
 * capability is never widened here, only reflected.
 */
object UiMapping {

    /**
     * How a peer's connection reads on a card.
     *
     * [targeted] is what stops a card lying. The link state is global — there
     * is one session — so without it every paired computer borrowed it, and
     * the U2 §39.17 report recorded exactly that: "the UI shows Connecting…
     * for the unreachable peer *and* for the reachable one, with no
     * indication that the second is never being attempted". A computer nobody
     * is dialling is `Available`, not `Connecting`, and certainly not `Error`
     * for a failure that belongs to a different machine.
     */
    fun statusFor(
        connection: OmniBridgeApp.ConnectionState,
        connected: Boolean,
        targeted: Boolean = true,
    ): OmniBridgeStatus = when {
        connected -> OmniBridgeStatus.Connected
        // Checked before the link state, because the link state is about
        // whichever computer *is* the target.
        !targeted -> OmniBridgeStatus.Available
        connection is OmniBridgeApp.ConnectionState.Connecting -> OmniBridgeStatus.Connecting
        connection is OmniBridgeApp.ConnectionState.Retrying -> OmniBridgeStatus.Connecting
        connection is OmniBridgeApp.ConnectionState.Error -> OmniBridgeStatus.Error
        // Paired and reachable, just not talking right now. Deliberately not
        // "Disconnected", which reads as a fault rather than a resting state.
        else -> OmniBridgeStatus.Available
    }

    /**
     * Whether "Send clipboard" may be pressed, and if not, what to say.
     *
     * ## Why this is a reason and not a boolean — GitHub #8
     *
     * There were four independent conditions and three screens, each of which
     * wrote its own subset inline; this function existed and none of them
     * called it. The home screen and the device card asked for the grant, the
     * policy and a live link, and the send screen asked only for a live link.
     * None asked whether the session that is up had actually **negotiated**
     * `clipboard.v1`, so the button was live over a session that would drop
     * the frame — and the send that followed failed with "Not connected to
     * that computer", which was not true and told the person nothing they
     * could act on.
     *
     * The order of the checks is the order a person can fix them in, which is
     * why the grant is asked before the connection: "turn it on" is an answer,
     * and "you are not connected" over a computer the person never granted
     * anything to is a red herring.
     */
    sealed interface ClipboardSendGate {
        data object Ready : ClipboardSendGate

        /** [reason] is written for a person and names no protocol internals. */
        data class Blocked(val reason: String) : ClipboardSendGate

        val ready: Boolean get() = this is Ready

        /** Null when ready, so a screen can show it under a disabled button. */
        val reasonOrNull: String? get() = (this as? Blocked)?.reason
    }

    /**
     * @param session what the **live** session negotiated, or null when there
     *   is none. Carries the peer it belongs to, because a capability
     *   negotiated with one computer says nothing about another — with two
     *   paired desktops a single global "connected" flag would let the
     *   session with A enable the Send button aimed at B.
     */
    fun clipboardSendGate(
        peer: TrustStore.TrustedPeer,
        session: OmniBridgeApp.LiveSession?,
    ): ClipboardSendGate {
        if (!peer.allows(ClipboardCapability.ID)) {
            return ClipboardSendGate.Blocked(
                "Turn on clipboard sharing for ${peer.deviceName} first.",
            )
        }
        if (!peer.clipboardPolicy.allowSend) {
            return ClipboardSendGate.Blocked(
                "Sending your clipboard to ${peer.deviceName} is turned off.",
            )
        }
        // Identity, not a flag. `session.peerHex` is the fingerprint hex the
        // handshake authenticated, and it is compared to this peer's own.
        if (session == null || session.peerHex != peer.fingerprint.toHex()) {
            return ClipboardSendGate.Blocked(
                "Connect to ${peer.deviceName} to send your clipboard.",
            )
        }
        if (ClipboardCapability.ID !in session.negotiated) {
            // The wording says what is true and what to expect, without
            // naming a capability id. A grant made just now is the ordinary
            // cause, and the session ends and redials on its own.
            return ClipboardSendGate.Blocked(
                "This connection with ${peer.deviceName} has not negotiated " +
                    "clipboard sharing yet.",
            )
        }
        return ClipboardSendGate.Ready
    }

    /** May "Send clipboard" be tapped? */
    fun canSendClipboard(
        peer: TrustStore.TrustedPeer,
        session: OmniBridgeApp.LiveSession?,
    ): Boolean = clipboardSendGate(peer, session).ready

    /** May a file be offered to this peer right now? */
    fun canSendFiles(peer: TrustStore.TrustedPeer, connected: Boolean): Boolean =
        peer.allows(FilesCapability.ID) && connected

    /**
     * Whether the "Apply automatically" switch can be touched.
     *
     * Automatic applying is meaningless when receiving is off at all, so the
     * switch is disabled rather than silently ignored.
     */
    fun autoReceiveEnabled(policy: ClipboardPolicy): Boolean = policy.allowReceive

    /**
     * Whether an auto-send switch may be offered at all.
     *
     * False on Android, and the UI states the reason instead of showing a
     * toggle that cannot work. See [ClipboardCapabilities.AUTO_SEND_REASON].
     */
    fun autoSendOfferable(): Boolean = ClipboardCapabilities.AUTO_SEND_SUPPORTED

    /** What a received clip will do, in words. */
    fun receiveBehaviour(policy: ClipboardPolicy): String = when {
        !policy.allowReceive -> "Clipboard text from this computer is refused."
        policy.mayAutoReceive() -> "Received text replaces your clipboard as it arrives."
        else -> "Received text waits in a notification until you tap Copy."
    }

    /** How a transfer's state reads. */
    fun transferStatus(state: TransferState): OmniBridgeStatus = when (state) {
        TransferState.TRANSFERRING, TransferState.VERIFYING -> OmniBridgeStatus.Transferring
        TransferState.COMPLETED -> OmniBridgeStatus.Success
        // A cancellation is not an error and is not displayed as one.
        TransferState.CANCELLED -> OmniBridgeStatus.Disconnected
        TransferState.FAILED -> OmniBridgeStatus.Error
        TransferState.OFFERED, TransferState.WAITING_ACCEPT -> OmniBridgeStatus.Connecting
    }

    /**
     * Keys for the home screen's lazy list.
     *
     * Namespaced by kind, and that is not tidiness. A pending clip is
     * identified by the fingerprint of the peer it came from, and a device
     * card by the fingerprint of the peer it *is* — so a clip arriving from a
     * paired computer produced two list items with the same key, and Compose
     * throws on a duplicate key rather than tolerating it. The home screen
     * crashed on every received clip until these were prefixed.
     */
    fun offerKey(transferId: String): String = "offer:$transferId"

    fun clipKey(peerHex: String): String = "clip:$peerHex"

    fun peerKey(fingerprintHex: String): String = "peer:$fingerprintHex"

    fun transferKey(transferId: String): String = "transfer:$transferId"

    /**
     * How far a Sharesheet send has got.
     *
     * The Sharesheet screen has no second chance to explain itself: it is a
     * modal over someone else's app, and when it is wrong the person's only
     * recourse is to close it and guess. So a send has only observable
     * outcomes and no silent one — which is what issue #12 was. [Sending]
     * must always be replaced, never merely entered.
     */
    sealed interface SendAttempt {
        /** Nothing started, or a failure the user may retry from. */
        data object Idle : SendAttempt

        /** The offer is out and no answer has come back. */
        data object Sending : SendAttempt

        /**
         * Shared text reached the computer. Terminal, and the screen closes.
         *
         * A clipboard update is delivered by the time `sendText` returns.
         * There is no transfer to follow, which is exactly what separates it
         * from [Offered].
         */
        data object Sent : SendAttempt

        /**
         * A file offer is out, and [transferId] is the attempt it created.
         *
         * The id is the load-bearing field and the whole of UX-DEBT-01. This
         * screen used to find "its" transfer by matching the display
         * filename against every transfer the app had ever seen, so a file
         * that had once been declined could never be offered again — see
         * [sendSurface]. `files.offer` has always returned the id of the
         * transfer it minted; it was simply thrown away here.
         */
        data class Offered(val transferId: String) : SendAttempt

        /** Terminal for this attempt, and retryable. */
        data class Failed(val message: String) : SendAttempt

        /**
         * Whether the Send button may be pressed.
         *
         * A whitelist rather than a list of exclusions: a state added later
         * is not sendable until somebody says it is, which is the safe
         * direction for a button that starts a transfer.
         */
        val canSend: Boolean get() = this is Idle || this is Failed
    }

    /**
     * What the Send button says, given where the attempt has got to.
     *
     * Shared by both Sharesheet screens so the two cannot drift, and stated
     * here rather than inline so the property issue #12 violated — that
     * "Sending…" is never what a finished attempt reads as — is a test rather
     * than a reading of a nested conditional.
     *
     * @param idleLabel what the untouched button says. The exchange screens
     *   name the destination in it — "Send file to fedora" — which is a
     *   *live* peer name assembled by the caller, never a constant. It
     *   deliberately applies to the idle state only: once an attempt is under
     *   way the button is reporting the attempt, and a button still offering
     *   to send to a named computer while that send is in flight would be
     *   describing something that has already happened.
     */
    fun sendButtonLabel(attempt: SendAttempt, idleLabel: String = "Send"): String =
        when (attempt) {
            is SendAttempt.Idle -> idleLabel
            is SendAttempt.Sending, is SendAttempt.Sent, is SendAttempt.Offered -> "Sending…"
            is SendAttempt.Failed -> "Try again"
        }

    /**
     * The outcome of one `files.offer`, as the screen should show it.
     *
     * The whole of issue #12 lives in this function being called at all.
     * `Result` has no failure branch you are forced to take, so the failure
     * branch was simply absent and the screen kept a state the transfer layer
     * had already abandoned. Stated as a total function over `Result`, there
     * is no path that returns [SendAttempt.Sending] and none that returns
     * nothing.
     */
    fun sendOutcome(result: Result<String>): SendAttempt = result.fold(
        // The success value is the transfer id, and it is kept. Discarding it
        // is what forced the screen to identify its own transfer by filename.
        onSuccess = { SendAttempt.Offered(it) },
        onFailure = { SendAttempt.Failed(sendFailureMessage(it)) },
    )

    /**
     * What the file half of the Sharesheet should be showing.
     *
     * ## UX-DEBT-01
     *
     * The screen used to decide this with
     *
     * ```kotlin
     * val mine = transfers.filter { it.sending && it.filename == name }
     * ```
     *
     * and show the Send button only when that came back empty. Three things
     * made that a dead end rather than a nicety:
     *
     *  * `files.visible` is every transfer of the process, terminal ones
     *    included — `FileTransferManager` never prunes its map, deliberately,
     *    because the Activity screen is a history;
     *  * so one decline left a permanent row for that display name, and the
     *    Send button never came back. Re-sharing the same file showed
     *    "declined" and nothing to press, for the life of the app. The
     *    previous sprint had to use a second filename to test an Accept;
     *  * and a *display name* is not an identity. Two different URIs that
     *    happen to be called `photo.jpg` aliased onto each other, and a
     *    transfer to one computer hid the Send button for another.
     *
     * The fix is to stop guessing. `files.offer` mints a fresh random id per
     * attempt and returns it; [SendAttempt.Offered] keeps it, and this
     * function follows *that* transfer and no other. A screen that has not
     * offered anything shows the button, whatever history holds.
     *
     * Presentation identity and protocol identity are now the same value,
     * which is safe precisely because the value is the protocol's — minted
     * from randomness, never derived from the file.
     */
    sealed interface SendSurface {
        /** No attempt from this screen is outstanding: offer the button. */
        data object Offer : SendSurface

        /**
         * This screen's attempt is running.
         *
         * [transfer] is null only in the instant between `offer` returning an
         * id and its row reaching the UI flow. Neither a button nor a row is
         * right in that instant, and showing the button would be how a
         * double tap becomes two transfers.
         */
        data class InFlight(val transfer: FileTransferManager.TransferUi?) : SendSurface

        /**
         * This screen's attempt reached a terminal state — declined, failed,
         * cancelled or completed. The row says which, and a retry is offered
         * beside it.
         */
        data class Ended(val transfer: FileTransferManager.TransferUi) : SendSurface
    }

    fun sendSurface(
        attempt: SendAttempt,
        transfers: List<FileTransferManager.TransferUi>,
    ): SendSurface = when (attempt) {
        is SendAttempt.Offered -> {
            // By id alone. Not by filename, not by peer, not by position.
            val mine = transfers.firstOrNull { it.transferId == attempt.transferId }
            when {
                mine == null -> SendSurface.InFlight(null)
                mine.state.isTerminal -> SendSurface.Ended(mine)
                else -> SendSurface.InFlight(mine)
            }
        }
        // Idle, Sending, Failed and the text-only Sent have no transfer to
        // follow, so the button is the whole screen.
        else -> SendSurface.Offer
    }

    /**
     * Whether a tap may start a new attempt right now.
     *
     * The one gate, so "can this start a transfer" has a single answer rather
     * than one per button. A settled attempt is always restartable — that is
     * the point of UX-DEBT-01 — and a running one never is, which is what
     * stops repeated taps turning into a pile of concurrent offers.
     */
    fun canStartSend(attempt: SendAttempt, surface: SendSurface): Boolean = when (surface) {
        is SendSurface.Offer -> attempt.canSend
        is SendSurface.Ended -> true
        is SendSurface.InFlight -> false
    }

    /**
     * What the button under a settled attempt says.
     *
     * A completed send is not a failure and must not be offered as one:
     * "Try again" over a file that arrived intact would read as though it had
     * not.
     */
    fun retryButtonLabel(state: TransferState): String = when (state) {
        TransferState.COMPLETED -> "Send again"
        else -> "Try again"
    }

    /**
     * The computers a file may actually be offered to.
     *
     * Stated here rather than inline in the screen so that a retry and a
     * first attempt cannot drift apart: both ask this, both at the moment of
     * the tap, so a grant withdrawn between the decline and the retry removes
     * the destination instead of being carried over from the earlier attempt.
     */
    fun fileDestinations(
        peers: List<TrustStore.TrustedPeer>,
    ): List<TrustStore.TrustedPeer> = peers.filter { it.allows(FilesCapability.ID) }

    /**
     * What a completed scan is called.
     *
     * A scan has two honest endings and they are not the same event:
     *
     *  * the computer demanded the code's single-use token, this phone proved
     *    it, and a person at the keyboard confirmed the fingerprint — trust
     *    was established, or re-established after a revoke;
     *  * the computer already trusted this phone and asked for nothing, so
     *    the code was not spent and nothing about trust changed.
     *
     * Reporting the second as the first is what let UX-HARDENING §20 test 8a
     * record a reconnection as a fresh pairing. The wording here is the only
     * thing that tells them apart on screen.
     */
    fun pairedMessage(deviceName: String, provedToken: Boolean): String =
        if (provedToken) {
            "Paired with $deviceName."
        } else {
            "$deviceName already trusts this device. Reconnected; the code was not used."
        }

    /**
     * Turns a failed `files.offer` into something safe to put on a screen.
     *
     * The capability states its own refusals — not connected, not authorized,
     * too many at once, unusable name, unreadable file — as
     * `IllegalStateException` with a message written for a person and
     * containing no file data. Those are shown as written.
     *
     * Anything else is replaced. This is the load-bearing half: a platform
     * exception raised while opening a `content://` URI carries the whole URI
     * in its message, and on many providers that URI and the display name are
     * enough to identify the file. `FileNotFoundException: No content
     * provider: content://…` on a Sharesheet screen is a small leak of what
     * someone was trying to send, to whoever is looking at the phone. The
     * capability already restates that one — this is the backstop that keeps
     * a future path from reintroducing it.
     */
    fun sendFailureMessage(error: Throwable?): String {
        val stated = (error as? IllegalStateException)?.message?.takeIf { it.isNotBlank() }
        return stated ?: "OmniBridge could not send that file."
    }

    /**
     * Actions that destroy something and must be presented apart.
     *
     * A list rather than a flag on each button, so that "is this destructive"
     * has one answer rather than one per screen.
     */
    val DESTRUCTIVE_ACTIONS = setOf(
        // Kept under its original name: the *presentation* rule this set
        // encodes has not changed, and renaming it would only make the
        // history harder to read. What the button does changed — it revokes
        // rather than deletes — and that is `ACTION_REVOKE` below.
        "forget_device",
        ACTION_REVOKE,
        ACTION_REMOVE_FROM_LIST,
    )

    /** Withdraw trust from a computer. The row stays, marked Revoked. */
    const val ACTION_REVOKE = "revoke_device"

    /** Take an already revoked computer off the list, keeping the tombstone. */
    const val ACTION_REMOVE_FROM_LIST = "remove_from_list"

    /** Connect to a computer, grant it something, choose it as a target. */
    const val ACTION_CONNECT = "connect"

    /**
     * What one device row offers, decided once.
     *
     * Stated here rather than as `if (peer.revoked)` in each screen so the
     * two rules that matter cannot drift apart between the list and the
     * detail screen:
     *
     *  * a **revoked** computer offers exactly one thing — taking it off the
     *    list. Not Connect, not a grant switch, and above all not a second
     *    Revoke, because "revoke" on something already revoked reads as
     *    though it would do something;
     *  * a **trusted** computer never offers "Remove from list". Removing is
     *    tidying-up, and tidying-up must not be a route to withdrawing trust
     *    without saying so. The store refuses it too; this is the half that
     *    means a person is never shown it.
     */
    fun deviceActions(peer: TrustStore.TrustedPeer): Set<String> = when {
        peer.revoked -> setOf(ACTION_REMOVE_FROM_LIST)
        else -> setOf(ACTION_CONNECT, ACTION_REVOKE)
    }

    /**
     * How a device row describes itself to an assistive technology.
     *
     * The name and the state in one string, because a badge that is a
     * separate object from the name it belongs to is two unrelated
     * announcements. Never colour alone, and never an icon alone: the state
     * is a word.
     */
    fun deviceStateDescription(peer: TrustStore.TrustedPeer): String =
        if (peer.revoked) {
            "${peer.deviceName}, revoked. This device can no longer connect."
        } else {
            peer.deviceName
        }

    /**
     * Everything the Send clipboard screen decides, in one value.
     *
     * Pulled out of the composable so the states that matter can be tested on
     * the JVM: empty, content present, sensitive, and each of the four ways
     * the send can be blocked. A rule that only exists inside a `@Composable`
     * is a rule that only an instrumented test can reach, and the send gate
     * is too important for that — this screen was once the most permissive of
     * the three Send clipboard affordances precisely because its condition
     * was written inline and nothing checked it.
     */
    data class SendClipboardUi(
        /** Whether the primary action may be pressed. */
        val enabled: Boolean,
        /**
         * Why not, or null when [enabled].
         *
         * Always populated when the button is off. A disabled button with no
         * explanation is the "dead control" the guidelines forbid.
         */
        val blockedReason: String?,
        /** Whether there is a clip at all — decides empty state vs preview. */
        val hasContent: Boolean,
        /** Whether the clip is present but withheld from the preview. */
        val contentHidden: Boolean,
    )

    /**
     * @param gate what the session and the grants permit, from
     *   [clipboardSendGate].
     * @param preview what this phone could read from the clipboard, or null.
     */
    fun sendClipboardUi(
        gate: ClipboardSendGate,
        preview: ClipboardPreview?,
    ): SendClipboardUi {
        // The gate is checked first: "connect to this computer" is a more
        // useful thing to say than "copy something" when both are true, and
        // it is the one the person has to act on first anyway.
        val reason = gate.reasonOrNull
            ?: EMPTY_CLIPBOARD_REASON.takeIf { preview == null }
        return SendClipboardUi(
            enabled = gate.ready && preview != null,
            blockedReason = reason,
            hasContent = preview != null,
            contentHidden = preview != null && preview.text == null,
        )
    }

    /**
     * Said when the clipboard is empty.
     *
     * Deliberately not "OmniBridge could not read your clipboard": Android
     * only permits a clipboard read while the app has focus, this screen has
     * already taken its one chance, and implying that a retry might work
     * would be inviting the person to do something that cannot help.
     */
    const val EMPTY_CLIPBOARD_REASON: String = "There is nothing to send."

    /**
     * A line about the link, shown under the device list — or null.
     *
     * @property isProblem whether it should wear the caution tone. A retry in
     *   progress is not a fault: it is the app doing its job, and painting it
     *   amber would make an ordinary Wi-Fi blip look like a failure.
     */
    data class ConnectionNotice(
        val title: String,
        val body: String?,
        val isProblem: Boolean,
    )

    /**
     * What, if anything, the Devices screen should add under the device list.
     *
     * Returns null for every state a device card already expresses, which is
     * most of them. The Devices screen used to carry a whole second card —
     * status badge, Connect, Disconnect — duplicating the selected device's
     * own row; this replaces it, and the rule it encodes is *only say what
     * the card above cannot*.
     *
     * `Connected` and `Idle` are therefore null: the card shows `Connected`
     * with a `Disconnect`, or `Available` with a `Connect`, and repeating
     * that is exactly the duplication this exists to remove. `Connecting` is
     * null too — the card's badge already says the word and there is no
     * detail to add.
     *
     * What survives is the detail a badge has no room for: a retry countdown
     * with its reason, and the text of a terminal error. Neither is
     * recoverable from the device card, and dropping them to tidy the screen
     * would be hiding a warning a person needs.
     *
     * @param mustChoose several devices are paired and none is the target —
     *   the one case where the screen must say something the cards cannot,
     *   because the answer is "pick one", not a property of any single row.
     */
    fun connectionNotice(
        connection: OmniBridgeApp.ConnectionState,
        mustChoose: Boolean,
    ): ConnectionNotice? = when (connection) {
        is OmniBridgeApp.ConnectionState.Retrying -> ConnectionNotice(
            title = "Reconnecting in ${connection.inSeconds}s",
            body = connection.reason,
            isProblem = false,
        )
        is OmniBridgeApp.ConnectionState.Error -> ConnectionNotice(
            title = "Could not connect",
            body = connection.message,
            isProblem = true,
        )
        // Nothing to add: the device card carries the whole story.
        is OmniBridgeApp.ConnectionState.Connected,
        is OmniBridgeApp.ConnectionState.Connecting,
        is OmniBridgeApp.ConnectionState.Idle,
        -> if (mustChoose) {
            ConnectionNotice(
                title = "Choose a device",
                body = "Several devices are paired. Use Connect on the one you want.",
                isProblem = false,
            )
        } else {
            null
        }
    }

    /**
     * What kind of machine is on the other end, as far as anything has said.
     *
     * Three cases and no guessing. [Unknown] is a real answer, not a gap to
     * be filled with the likeliest platform: the exchange hero draws a device
     * glyph from this, and drawing a monitor for a peer that has never stated
     * a platform would put the same untrue "Desktop · Linux" claim back on
     * the screen in pictures, one layer below where it was fixed.
     */
    enum class DeviceKind { Desktop, Mobile, Unknown }

    /** The kind a stated platform implies, or [DeviceKind.Unknown]. */
    fun deviceKind(platform: Platform?): DeviceKind = when (platform) {
        Platform.PLATFORM_LINUX -> DeviceKind.Desktop
        Platform.PLATFORM_ANDROID -> DeviceKind.Mobile
        else -> DeviceKind.Unknown
    }

    /**
     * The kind of machine a peer is, **while its own session is up**.
     *
     * The same rule, and the same identity comparison, as
     * [peerIdentityLine]: a platform arrives with a session and this app does
     * not keep it, so a peer with no live session of its own is
     * [DeviceKind.Unknown] rather than whatever it last said. A session with
     * another computer says nothing about this one.
     */
    fun peerDeviceKind(
        peer: TrustStore.TrustedPeer,
        session: OmniBridgeApp.LiveSession?,
    ): DeviceKind =
        deviceKind(session?.takeIf { it.peerHex == peer.fingerprint.toHex() }?.platform)

    /**
     * How a platform reads on screen, or null when nothing said.
     *
     * `PLATFORM_UNSPECIFIED` and any value this build does not know are
     * deliberately null rather than "Unknown": a device whose platform was
     * never stated should show nothing there, not a label asserting that the
     * platform is the unknown one.
     */
    fun platformLabel(platform: Platform): String? = when (platform) {
        Platform.PLATFORM_LINUX -> "Desktop · Linux"
        Platform.PLATFORM_ANDROID -> "Phone or tablet · Android"
        else -> null
    }

    /**
     * The line under a device's name.
     *
     * The peer's platform **while a session is up**, and its short
     * fingerprint otherwise.
     *
     * Three screens used to print a literal `"Desktop · Linux"` here, with
     * nothing behind it: every paired device was labelled Linux, including a
     * phone. The platform is real — `DeviceInfo.platform` is field 3 of the
     * HELLO — but it arrives *with a session* and this app does not keep it,
     * because a stored platform is a claim about a machine that is not
     * talking to us, made by a message that arrived days ago.
     *
     * So the line says the platform when there is a live session to back it,
     * and falls back to the pinned fingerprint when there is not. The
     * fingerprint is not a consolation prize: it is what tells two computers
     * called `fedora` apart, and what a person compares while pairing.
     */
    fun peerIdentityLine(
        peer: TrustStore.TrustedPeer,
        session: OmniBridgeApp.LiveSession? = null,
    ): String {
        // Identity, not a flag: a session with another computer says nothing
        // about this one. The same comparison the send gate makes.
        val mine = session?.takeIf { it.peerHex == peer.fingerprint.toHex() }
        return mine?.let { platformLabel(it.platform) } ?: peer.fingerprint.toDisplayShort()
    }
}
