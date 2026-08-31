package io.github.yurisismotto.anyflow.ui

import androidx.compose.runtime.Immutable
import io.github.yurisismotto.anyflow.AnyFlowApp
import io.github.yurisismotto.anyflow.clipboard.ClipboardPolicy
import io.github.yurisismotto.anyflow.clipboard.ClipboardSync
import io.github.yurisismotto.anyflow.files.FileTransferManager
import io.github.yurisismotto.anyflow.identity.Fingerprint
import io.github.yurisismotto.anyflow.store.TrustStore

/**
 * Everything the UI draws, in one immutable snapshot.
 *
 * Collected once in the Activity and passed down, so a screen cannot reach
 * back into the application object and read something that is not observed —
 * the bug that used to leave a grant change invisible until the screen was
 * recreated.
 */
@Immutable
data class MainUiState(
    val ownDeviceName: String,
    val ownFingerprint: String,
    val keyBackingDescription: String,
    val connection: AnyFlowApp.ConnectionState,
    val peers: List<TrustStore.TrustedPeer>,
    val offers: List<FileTransferManager.IncomingOffer>,
    val transfers: List<FileTransferManager.TransferUi>,
    val pendingClips: List<ClipboardSync.PendingClipInfo>,
    val clipboardOutcomes: Map<String, ClipboardSync.Outcome>,
    val remoteBatteryPercent: Int?,
) {
    /** The fingerprint of the peer with a live session, if any. */
    val connectedFingerprintShort: String?
        get() = (connection as? AnyFlowApp.ConnectionState.Connected)?.fingerprintShort

    fun isConnected(peer: TrustStore.TrustedPeer): Boolean =
        connectedFingerprintShort == peer.fingerprint.toDisplayShort()

    fun peerByHex(hex: String): TrustStore.TrustedPeer? =
        peers.firstOrNull { it.fingerprint.toHex() == hex }

    /** True when nothing at all is happening — what the Activity tab shows. */
    val hasActivity: Boolean
        get() = offers.isNotEmpty() || transfers.isNotEmpty() || pendingClips.isNotEmpty()
}

/**
 * What the UI can ask for.
 *
 * A holder rather than a dozen lambda parameters per screen. Every one of
 * these already existed before this sprint; the visual work re-presents them
 * and adds none.
 */
@Immutable
data class MainActions(
    val onPair: () -> Unit,
    val onConnect: () -> Unit,
    val onDisconnect: () -> Unit,
    val onForget: (TrustStore.TrustedPeer) -> Unit,
    val onSetFilesGrant: (TrustStore.TrustedPeer, Boolean) -> Unit,
    val onSetClipboardGrant: (TrustStore.TrustedPeer, Boolean) -> Unit,
    val onSetBatteryGrant: (TrustStore.TrustedPeer, Boolean) -> Unit,
    val onSetClipboardPolicy: (TrustStore.TrustedPeer, ClipboardPolicy) -> Unit,
    val onSendClipboard: (Fingerprint) -> Unit,
    val onApplyClip: (Fingerprint) -> Unit,
    val onDismissClip: (Fingerprint) -> Unit,
    val onRespondToOffer: (String, Boolean) -> Unit,
    val onCancelTransfer: (String) -> Unit,
    val onPickFileFor: (Fingerprint) -> Unit,
    /**
     * Reads the clipboard for the send screen's preview.
     *
     * Returns null when the platform refuses or there is nothing there. The
     * call happens from a focused Activity, which is the only state in which
     * Android permits it.
     */
    val readClipboardPreview: () -> ClipboardPreview?,
)

/**
 * What the Send clipboard screen may show about the clip.
 *
 * ## Why [text] can be null while [bytes] is not
 *
 * The reference design shows a preview of the clipboard before sending, and
 * for ordinary text that is genuinely useful — it is how a person notices
 * they are about to send the wrong thing.
 *
 * A clip the source app marked sensitive is different. `EXTRA_IS_SENSITIVE`
 * is set by password managers precisely so that surfaces like this one do not
 * render the value, and putting a recovery code on screen inside a
 * shoulder-surfable preview would defeat the one hint we are given. So for a
 * sensitive clip [text] is null and the UI shows the size and the destination
 * only — the same rule the confirmation dialog has always followed.
 *
 * Neither field is ever logged or persisted. This object lives for as long as
 * the screen is composed and no longer.
 */
@Immutable
data class ClipboardPreview(
    val text: String?,
    val bytes: Int,
    val sensitive: Boolean,
)
