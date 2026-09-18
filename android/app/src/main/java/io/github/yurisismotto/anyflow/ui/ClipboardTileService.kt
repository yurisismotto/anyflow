package io.github.yurisismotto.anyflow.ui

import android.app.PendingIntent
import android.content.Intent
import android.os.Build
import android.service.quicksettings.Tile
import android.service.quicksettings.TileService
import io.github.yurisismotto.anyflow.AnyFlowApp
import io.github.yurisismotto.anyflow.capability.ClipboardCapability
import java.security.SecureRandom

/**
 * A "Send clipboard" tile in the Quick Settings panel.
 *
 * ## What a tile can and cannot do, and why this one opens the app
 *
 * A [TileService] has no input focus — the Quick Settings panel belongs to
 * System UI, not to us — so `getPrimaryClip` returns null inside `onClick`.
 * That is not a bug to route around; it is the same Android 10 restriction
 * that shapes the whole capability, and every way to defeat it (an
 * accessibility service, becoming the default IME, an invisible activity that
 * grabs focus for a frame) is either forbidden or user-hostile.
 *
 * So the tile does the one supported thing: it brings AnyFlow to the
 * foreground with an explicit "send the clipboard" request. The Activity then
 * has focus, reads the clipboard, and shows the peer — including the
 * sensitive-clip confirmation if the platform marked it. The person still
 * taps once more, and they can see where the text is going when they do.
 *
 * That makes the tile a *shortcut*, not a privileged path. It is worth having
 * anyway: it removes the "find the app, scroll to the computer" step from
 * what is otherwise a two-second action.
 *
 * ## `startActivityAndCollapse` changed in Android 14
 *
 * Taking an `Intent` was deprecated and then made to throw
 * `UnsupportedOperationException` on API 34+; a `PendingIntent` is required.
 * Both are handled, because this app supports API 29 upward.
 *
 * ## Why every press carries a fresh id — GitHub #7
 *
 * `MainActivity` cannot act on this the moment it arrives: Android refuses
 * `getPrimaryClip` to an app without window focus, so the request has to wait
 * for `onWindowFocusChanged` (see [ClipboardShortcut]). Waiting means the
 * request outlives the callback that delivered it, and `getIntent()` keeps
 * returning the launch intent — so a rotation would replay it and send the
 * clipboard again.
 *
 * A random id per press is what tells the two apart. It is minted here, at
 * the one place that knows a person actually pressed the tile, and
 * `FLAG_UPDATE_CURRENT` is what makes the reused `PendingIntent` carry the
 * new one. It is an idempotency token and nothing else: it is not a
 * credential, it authorizes nothing, and the send it leads to still asks the
 * trust store for the grant and the policy.
 */
class ClipboardTileService : TileService() {

    override fun onStartListening() {
        super.onStartListening()
        updateTile()
    }

    private fun updateTile() {
        val tile = qsTile ?: return
        val app = application as? AnyFlowApp
        // A tile that is active-looking with nothing paired would be a lie.
        val usable = runCatching {
            app?.trustStore?.peers()?.any {
                it.allows(ClipboardCapability.ID) && it.clipboardPolicy.allowSend
            } == true
        }.getOrDefault(false)

        tile.state = if (usable) Tile.STATE_INACTIVE else Tile.STATE_UNAVAILABLE
        tile.subtitle = if (usable) null else "No computer set up"
        runCatching { tile.updateTile() }
    }

    override fun onClick() {
        super.onClick()

        val intent = Intent(this, MainActivity::class.java)
            .setAction(MainActivity.ACTION_SEND_CLIPBOARD)
            .putExtra(MainActivity.EXTRA_REQUEST_ID, newRequestId())
            .addFlags(
                Intent.FLAG_ACTIVITY_NEW_TASK or
                    Intent.FLAG_ACTIVITY_CLEAR_TOP or
                    Intent.FLAG_ACTIVITY_SINGLE_TOP,
            )

        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
            val pending = PendingIntent.getActivity(
                this,
                0,
                intent,
                PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
            )
            startActivityAndCollapse(pending)
        } else {
            @Suppress("DEPRECATION")
            startActivityAndCollapse(intent)
        }
    }

    /**
     * 16 random bytes as hex.
     *
     * Random rather than a counter: a counter would restart with the process,
     * and a restarted counter colliding with an id `MainActivity` had already
     * consumed is exactly the replay this is here to stop.
     */
    private fun newRequestId(): String =
        ByteArray(REQUEST_ID_BYTES)
            .also(random::nextBytes)
            .joinToString("") { "%02x".format(it) }

    private companion object {
        const val REQUEST_ID_BYTES = 16
        val random = SecureRandom()
    }
}
