package io.github.yurisismotto.omnibridge.clipboard

import android.Manifest
import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import androidx.core.content.ContextCompat
import io.github.yurisismotto.omnibridge.R
import io.github.yurisismotto.omnibridge.identity.Fingerprint
import io.github.yurisismotto.omnibridge.ui.MainActivity

/**
 * "Clipboard received from <computer>" — the documented fallback.
 *
 * This is what happens when `autoReceive` is off, which is the default, and
 * it is also the safety net for any case where `setPrimaryClip` is refused.
 * The clip is already in memory at this point; the notification exists so a
 * person can choose to apply it.
 *
 * ## What is *not* in the notification
 *
 * The clipboard text, in any form — not as the title, not as the body, not as
 * a `bigText` preview, not in the label of the action. A notification is
 * shown on the lock screen by default and is readable by any notification
 * listener the user has installed; putting a clip there would undo the
 * capability's entire privacy argument. What is shown is the computer's name,
 * the size, and whether the clip was marked sensitive.
 */
class ClipboardNotifications(context: Context) {

    private val appContext = context.applicationContext

    /** Offers one held clip to the person. */
    fun show(clip: ClipboardSync.PendingClipInfo) {
        if (!canPost()) return
        ensureChannel()

        val open = PendingIntent.getActivity(
            appContext,
            // Distinct per peer, so two computers do not overwrite each
            // other's notification or each other's pending intent.
            clip.peer.toHex().hashCode(),
            Intent(appContext, MainActivity::class.java)
                .setAction(ACTION_APPLY_CLIP)
                .putExtra(EXTRA_PEER_FINGERPRINT, clip.peer.toHex())
                .addFlags(Intent.FLAG_ACTIVITY_CLEAR_TOP or Intent.FLAG_ACTIVITY_SINGLE_TOP),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )

        val body = buildString {
            append(appContext.getString(R.string.clip_received_body, clip.bytes))
            if (clip.sensitive) {
                append(' ')
                append(appContext.getString(R.string.clip_marked_sensitive))
            }
        }

        val notification = Notification.Builder(appContext, CHANNEL_ID)
            .setContentTitle(
                appContext.getString(R.string.clip_received_title, clip.peerName),
            )
            .setContentText(body)
            .setSmallIcon(android.R.drawable.ic_menu_upload)
            .setAutoCancel(true)
            .setContentIntent(open)
            // The action opens the app rather than applying in place. That is
            // deliberate: applying from a notification would put a clip on the
            // clipboard from a surface the person may be looking at on a lock
            // screen, without the app ever being on screen to say which
            // computer it came from.
            .addAction(
                Notification.Action.Builder(
                    null,
                    appContext.getString(R.string.clip_action_copy),
                    open,
                ).build(),
            )
            .build()

        runCatching {
            manager().notify(notificationId(clip.peer), notification)
        }
    }

    /** Withdraws a peer's notification once its clip is applied or gone. */
    fun clear(peer: Fingerprint) {
        runCatching { manager().cancel(notificationId(peer)) }
    }

    private fun notificationId(peer: Fingerprint): Int =
        NOTIFICATION_ID_BASE + (peer.toHex().hashCode() and 0xffff)

    private fun manager(): NotificationManager =
        appContext.getSystemService(NotificationManager::class.java)

    private fun canPost(): Boolean {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU) return true
        return ContextCompat.checkSelfPermission(
            appContext,
            Manifest.permission.POST_NOTIFICATIONS,
        ) == PackageManager.PERMISSION_GRANTED
    }

    private fun ensureChannel() {
        val channel = NotificationChannel(
            CHANNEL_ID,
            appContext.getString(R.string.channel_clipboard),
            // DEFAULT rather than HIGH: a received clip is worth noticing but
            // is not urgent, and a heads-up banner for every clip would be
            // intolerable with auto-receive off.
            NotificationManager.IMPORTANCE_DEFAULT,
        ).apply {
            description = appContext.getString(R.string.channel_clipboard_description)
            // No preview on a lock screen. The notification carries no clip
            // text at all, so this is belt and braces — and it is the right
            // default for a channel whose whole subject is the clipboard.
            lockscreenVisibility = Notification.VISIBILITY_PRIVATE
        }
        runCatching { manager().createNotificationChannel(channel) }
    }

    companion object {
        private const val CHANNEL_ID = "clipboard"
        private const val NOTIFICATION_ID_BASE = 1000

        /** Tells [MainActivity] to offer a held clip. */
        const val ACTION_APPLY_CLIP = "io.github.yurisismotto.omnibridge.APPLY_CLIP"
        const val EXTRA_PEER_FINGERPRINT = "peer_fingerprint"
    }
}
