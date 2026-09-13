package io.github.yurisismotto.anyflow.ui

import androidx.annotation.StringRes
import io.github.yurisismotto.anyflow.R
import io.github.yurisismotto.anyflow.notifications.LockPolicy
import io.github.yurisismotto.anyflow.notifications.NotificationDismissReadiness
import io.github.yurisismotto.anyflow.notifications.NotificationReadiness
import io.github.yurisismotto.anyflow.ui.theme.AnyFlowStatus

/**
 * How a [NotificationReadiness] is presented, and nothing else.
 *
 * Kept apart from the screens so that "which of the eight states is this, and
 * what does it say" is a pure function a JVM test can enumerate — the same
 * reason [UiMapping] exists for transfer failures.
 *
 * Every state resolves to a **word**, never to a colour alone: the badge takes
 * an [AnyFlowStatus] for its dot and icon and a string resource for its label,
 * and there is no path here that produces one without the other.
 */
object NotificationUiMapping {

    /** The short word on the badge. */
    @StringRes
    fun statusLabel(readiness: NotificationReadiness): Int = when (readiness) {
        NotificationReadiness.SHARING_OFF -> R.string.notif_status_sharing_off
        NotificationReadiness.NEEDS_ANDROID_ACCESS -> R.string.notif_status_needs_access
        NotificationReadiness.UNAVAILABLE -> R.string.notif_status_unavailable
        NotificationReadiness.PAUSED -> R.string.notif_status_paused
        NotificationReadiness.NO_APPS -> R.string.notif_status_no_apps
        NotificationReadiness.NOT_CONNECTED -> R.string.notif_status_not_connected
        NotificationReadiness.PEER_NOT_RECEIVING -> R.string.notif_status_peer_not_receiving
        NotificationReadiness.READY -> R.string.notif_status_ready
    }

    /** The sentence under it, which is what makes the state actionable. */
    @StringRes
    fun statusDetail(readiness: NotificationReadiness): Int = when (readiness) {
        NotificationReadiness.SHARING_OFF -> R.string.notif_detail_sharing_off
        NotificationReadiness.NEEDS_ANDROID_ACCESS -> R.string.notif_detail_needs_access
        NotificationReadiness.UNAVAILABLE -> R.string.notif_detail_unavailable
        NotificationReadiness.PAUSED -> R.string.notif_detail_paused
        NotificationReadiness.NO_APPS -> R.string.notif_detail_no_apps
        NotificationReadiness.NOT_CONNECTED -> R.string.notif_detail_not_connected
        NotificationReadiness.PEER_NOT_RECEIVING -> R.string.notif_detail_peer_not_receiving
        NotificationReadiness.READY -> R.string.notif_detail_ready
    }

    /**
     * The dot, icon and hue.
     *
     * Only [NotificationReadiness.READY] is positive. Everything else is
     * either off (neutral) or needs something doing (amber) — and nothing that
     * is *not* mirroring is ever drawn as though it were, because a person
     * who believes their notifications are being shared when they are not is
     * the failure this screen exists to prevent, and so is its opposite.
     */
    fun status(readiness: NotificationReadiness): AnyFlowStatus = when (readiness) {
        NotificationReadiness.READY -> AnyFlowStatus.Connected
        NotificationReadiness.SHARING_OFF -> AnyFlowStatus.Disconnected
        NotificationReadiness.NOT_CONNECTED -> AnyFlowStatus.Available
        NotificationReadiness.PAUSED -> AnyFlowStatus.Disconnected
        NotificationReadiness.NEEDS_ANDROID_ACCESS,
        NotificationReadiness.NO_APPS,
        NotificationReadiness.PEER_NOT_RECEIVING,
        NotificationReadiness.UNAVAILABLE,
        -> AnyFlowStatus.Warning
    }

    /**
     * What dismissal synchronisation is doing, as a sentence.
     *
     * There is no short badge word for this one on purpose: the row is a
     * switch, and the switch's position already says on or off. What a person
     * needs beside it is the sentence that says whether "on" is actually doing
     * anything, and what to do when it is not.
     */
    @StringRes
    fun dismissDetail(readiness: NotificationDismissReadiness): Int = when (readiness) {
        NotificationDismissReadiness.NEEDS_ANDROID_ACCESS ->
            R.string.notif_dismiss_state_needs_access
        NotificationDismissReadiness.OFF -> R.string.notif_dismiss_state_off
        NotificationDismissReadiness.NOT_CONNECTED -> R.string.notif_dismiss_state_not_connected
        NotificationDismissReadiness.PEER_CANNOT_REPORT ->
            R.string.notif_dismiss_state_peer_cannot_report
        NotificationDismissReadiness.ACTIVE -> R.string.notif_dismiss_state_active
    }

    /** The lock-policy option's own name. */
    @StringRes
    fun lockPolicyLabel(policy: LockPolicy): Int = when (policy) {
        LockPolicy.FULL -> R.string.notif_locked_full
        LockPolicy.APP_ONLY -> R.string.notif_locked_app_only
        LockPolicy.SUPPRESS -> R.string.notif_locked_suppress
    }

    /** What choosing it actually does, which is the part that matters. */
    @StringRes
    fun lockPolicyDetail(policy: LockPolicy): Int = when (policy) {
        LockPolicy.FULL -> R.string.notif_locked_full_detail
        LockPolicy.APP_ONLY -> R.string.notif_locked_app_only_detail
        LockPolicy.SUPPRESS -> R.string.notif_locked_suppress_detail
    }
}
