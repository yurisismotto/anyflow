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

    /** How a peer's connection reads on a card. */
    fun statusFor(
        connection: AnyFlowApp.ConnectionState,
        connected: Boolean,
    ): AnyFlowStatus = when {
        connected -> AnyFlowStatus.Connected
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
     * Actions that destroy something and must be presented apart.
     *
     * A list rather than a flag on each button, so that "is this destructive"
     * has one answer rather than one per screen.
     */
    val DESTRUCTIVE_ACTIONS = setOf("forget_device")
}
