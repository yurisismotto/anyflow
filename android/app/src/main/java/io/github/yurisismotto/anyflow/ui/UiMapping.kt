package io.github.yurisismotto.anyflow.ui

import io.github.yurisismotto.anyflow.AnyFlowApp
import io.github.yurisismotto.anyflow.capability.ClipboardCapability
import io.github.yurisismotto.anyflow.capability.FilesCapability
import io.github.yurisismotto.anyflow.clipboard.ClipboardCapabilities
import io.github.yurisismotto.anyflow.clipboard.ClipboardPolicy
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
     * recourse is to close it and guess. So a send has exactly three
     * observable outcomes and no fourth, silent one — which is what issue #12
     * was. [Sending] must always be replaced, never merely entered.
     */
    sealed interface SendAttempt {
        /** Nothing started, or a failure the user may retry from. */
        data object Idle : SendAttempt

        /** The offer is out and no answer has come back. */
        data object Sending : SendAttempt

        /** The offer was accepted for transfer; progress takes over. */
        data object Sent : SendAttempt

        /** Terminal for this attempt, and retryable. */
        data class Failed(val message: String) : SendAttempt

        /** Whether the Send button may be pressed. */
        val canSend: Boolean get() = this !is Sending && this !is Sent
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
        is SendAttempt.Sending, is SendAttempt.Sent -> "Sending…"
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
        onSuccess = { SendAttempt.Sent },
        onFailure = { SendAttempt.Failed(sendFailureMessage(it)) },
    )

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
    val DESTRUCTIVE_ACTIONS = setOf("forget_device")
}
