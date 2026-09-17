package io.github.yurisismotto.anyflow.ui

import io.github.yurisismotto.anyflow.AnyFlowApp
import io.github.yurisismotto.anyflow.capability.ClipboardCapability
import io.github.yurisismotto.anyflow.capability.FilesCapability
import io.github.yurisismotto.anyflow.clipboard.ClipboardCapabilities
import io.github.yurisismotto.anyflow.clipboard.ClipboardPolicy
import io.github.yurisismotto.anyflow.files.FileTransferManager
import io.github.yurisismotto.anyflow.files.TransferState
import io.github.yurisismotto.anyflow.store.TrustStore
import io.github.yurisismotto.anyflow.ui.theme.AnyFlowStatus

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
        connection: AnyFlowApp.ConnectionState,
        connected: Boolean,
        targeted: Boolean = true,
    ): AnyFlowStatus = when {
        connected -> AnyFlowStatus.Connected
        // Checked before the link state, because the link state is about
        // whichever computer *is* the target.
        !targeted -> AnyFlowStatus.Available
        connection is AnyFlowApp.ConnectionState.Connecting -> AnyFlowStatus.Connecting
        connection is AnyFlowApp.ConnectionState.Retrying -> AnyFlowStatus.Connecting
        connection is AnyFlowApp.ConnectionState.Error -> AnyFlowStatus.Error
        // Paired and reachable, just not talking right now. Deliberately not
        // "Disconnected", which reads as a fault rather than a resting state.
        else -> AnyFlowStatus.Available
    }

    /**
     * May "Send clipboard" be tapped?
     *
     * Three separate conditions, and collapsing them is how a person comes to
     * believe sync is running when it is not: the capability must be granted,
     * the direction must be permitted, and a session must exist.
     */
    fun canSendClipboard(peer: TrustStore.TrustedPeer, connected: Boolean): Boolean =
        peer.allows(ClipboardCapability.ID) && peer.clipboardPolicy.allowSend && connected

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
    fun transferStatus(state: TransferState): AnyFlowStatus = when (state) {
        TransferState.TRANSFERRING, TransferState.VERIFYING -> AnyFlowStatus.Transferring
        TransferState.COMPLETED -> AnyFlowStatus.Success
        // A cancellation is not an error and is not displayed as one.
        TransferState.CANCELLED -> AnyFlowStatus.Disconnected
        TransferState.FAILED -> AnyFlowStatus.Error
        TransferState.OFFERED, TransferState.WAITING_ACCEPT -> AnyFlowStatus.Connecting
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
     */
    fun sendButtonLabel(attempt: SendAttempt): String = when (attempt) {
        is SendAttempt.Idle -> "Send"
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
        return stated ?: "AnyFlow could not send that file."
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
}
