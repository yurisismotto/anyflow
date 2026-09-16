package io.github.yurisismotto.anyflow.ui

import android.graphics.drawable.Drawable
import androidx.compose.runtime.Immutable
import io.github.yurisismotto.anyflow.AnyFlowApp
import io.github.yurisismotto.anyflow.capability.NotificationsCapability
import io.github.yurisismotto.anyflow.clipboard.ClipboardPolicy
import io.github.yurisismotto.anyflow.clipboard.ClipboardSync
import io.github.yurisismotto.anyflow.files.FileTransferManager
import io.github.yurisismotto.anyflow.identity.Fingerprint
import io.github.yurisismotto.anyflow.notifications.NotificationApp
import io.github.yurisismotto.anyflow.notifications.NotificationGates
import io.github.yurisismotto.anyflow.notifications.NotificationPolicy
import io.github.yurisismotto.anyflow.notifications.NotificationSource
import io.github.yurisismotto.anyflow.store.PeerTarget
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
    /**
     * The computer the person chose to connect to, as a fingerprint hex.
     *
     * Observed from the trust store rather than held in a screen, because the
     * connection service writes it too — it is the identity carried in a start
     * intent — and a copy owned by the UI would disagree the moment a share
     * sheet re-pointed the link.
     */
    val selectedPeerHex: String?,
    val offers: List<FileTransferManager.IncomingOffer>,
    val transfers: List<FileTransferManager.TransferUi>,
    val pendingClips: List<ClipboardSync.PendingClipInfo>,
    val clipboardOutcomes: Map<String, ClipboardSync.Outcome>,
    val remoteBatteryPercent: Int?,
    /**
     * Android's own notification access, as the platform reports it **now**.
     *
     * Re-read on every resume rather than remembered: a person can revoke it
     * in Settings while this screen is open, and a cached "yes" is exactly the
     * stale answer that would leave a consent screen claiming to work.
     */
    val notificationAccessGranted: Boolean,
    /** What `notifications.v1` is actually doing. Counts and roles, no content. */
    val notifications: NotificationSource.Status,
    /**
     * Whether this device has a second profile at all.
     *
     * Decides only whether the work-profile switch is offered or explained as
     * inapplicable. It never changes what is shared: the switch is off by
     * default and an application still has to be chosen either way.
     */
    val hasWorkProfile: Boolean,
) {
    /** The fingerprint of the peer with a live session, if any. */
    val connectedFingerprintShort: String?
        get() = (connection as? AnyFlowApp.ConnectionState.Connected)?.fingerprintShort

    /**
     * Which computer this phone is pointed at, and why — or why it is not.
     *
     * The same function the connection service resolves its destination with,
     * over the same two inputs, so the screen cannot show one target while the
     * service dials another. Both replaced `peers.first()`, which is how the
     * quick actions used to offer "Send files" for a desktop that had been
     * powered off for a week.
     */
    val target: PeerTarget.Resolution
        get() = PeerTarget.resolve(peers, selectedPeerHex)

    /** The computer the buttons act on, or null when the person must choose. */
    val targetPeer: TrustStore.TrustedPeer?
        get() = target.peerOrNull()

    /** True when several computers are trusted and none has been chosen. */
    val mustChooseTarget: Boolean
        get() = target is PeerTarget.Resolution.MustChoose

    /** Whether [peer] is the one the connection is pointed at. */
    fun isTarget(peer: TrustStore.TrustedPeer): Boolean =
        targetPeer?.fingerprint?.contentEquals(peer.fingerprint) == true

    fun isConnected(peer: TrustStore.TrustedPeer): Boolean =
        connectedFingerprintShort == peer.fingerprint.toDisplayShort()

    fun peerByHex(hex: String): TrustStore.TrustedPeer? =
        peers.firstOrNull { it.fingerprint.toHex() == hex }

    /** True when nothing at all is happening — what the Activity tab shows. */
    val hasActivity: Boolean
        get() = offers.isNotEmpty() || transfers.isNotEmpty() || pendingClips.isNotEmpty()

    /**
     * The notification gates for one computer, assembled from live state.
     *
     * Assembled here, once, rather than in each screen: the whole point of
     * [NotificationGates] is that a UI cannot accidentally answer two of the
     * questions and infer the third. The dismissal half is assembled from the
     * same place for the same reason — it has two gates of its own, and a
     * screen that guessed one of them from the mirroring state would be
     * exactly the collapse the type exists to prevent.
     */
    fun notificationGates(peer: TrustStore.TrustedPeer): NotificationGates {
        val policy = peer.notificationPolicy
        val granted = peer.allows(NotificationsCapability.ID)
        val peerStatus = notifications.peers[peer.fingerprint.toHex()]
        return NotificationGates(
            osAccessGranted = notificationAccessGranted,
            peerGranted = granted,
            allowMirror = policy.allowMirror,
            allowedAppCount = policy.allowedApps.size,
            sourceActive = notifications.listenerConnected &&
                notifications.accessGranted &&
                notifications.secretAvailable,
            peerConnected = peerStatus != null,
            peerIsSink = peerStatus?.peerIsSink == true,
            allowDismissSync = policy.allowDismissSync,
            peerIsDismissReporter = peerStatus?.peerIsDismissReporter == true,
        )
    }
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
    /**
     * Connects to one named computer.
     *
     * Takes a fingerprint because the certified defect was that it did not:
     * the action used to be `() -> Unit`, so the identity of the row the
     * person tapped stopped at the UI and the service resolved a destination
     * of its own from trust-store order. The pinned identity now travels all
     * the way to the socket.
     */
    val onConnect: (Fingerprint) -> Unit,
    val onDisconnect: () -> Unit,
    val onForget: (TrustStore.TrustedPeer) -> Unit,
    val onSetFilesGrant: (TrustStore.TrustedPeer, Boolean) -> Unit,
    val onSetClipboardGrant: (TrustStore.TrustedPeer, Boolean) -> Unit,
    val onSetBatteryGrant: (TrustStore.TrustedPeer, Boolean) -> Unit,
    val onSetClipboardPolicy: (TrustStore.TrustedPeer, ClipboardPolicy) -> Unit,
    /**
     * Grants or withdraws `notifications.v1` for one computer.
     *
     * Writes the canonical grant through the trust store, the same one the
     * clipboard and files switches write. There is no second permission
     * database: the listener lifecycle, the filter and the role announcement
     * all read that one.
     */
    val onSetNotificationsGrant: (TrustStore.TrustedPeer, Boolean) -> Unit,
    /** Replaces one computer's notification policy. Never widens another's. */
    val onSetNotificationPolicy: (TrustStore.TrustedPeer, NotificationPolicy) -> Unit,
    /**
     * Opens Android's own notification-access screen, on AnyFlow's own switch.
     *
     * The permission is granted there and nowhere else — there is no dialog
     * here that could grant it, and the real state is re-read when the person
     * comes back rather than assumed from the fact that they left.
     */
    val onOpenNotificationAccess: () -> Unit,
    /**
     * The picker's list for one computer.
     *
     * Suspending because it is a hundred binder calls; it must not run during
     * composition or on the main thread.
     */
    val loadNotificationApps: suspend (TrustStore.TrustedPeer) -> List<NotificationApp>,
    /** One application's icon, loaded locally and never transmitted. */
    val loadAppIcon: suspend (String) -> Drawable?,
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
