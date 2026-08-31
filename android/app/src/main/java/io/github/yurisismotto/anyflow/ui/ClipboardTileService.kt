package io.github.yurisismotto.anyflow.ui

import android.app.PendingIntent
import android.content.Intent
import android.os.Build
import android.service.quicksettings.Tile
import android.service.quicksettings.TileService
import io.github.yurisismotto.anyflow.AnyFlowApp
import io.github.yurisismotto.anyflow.capability.ClipboardCapability

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
}
