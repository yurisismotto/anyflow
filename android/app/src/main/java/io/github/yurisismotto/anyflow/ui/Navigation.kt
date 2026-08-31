package io.github.yurisismotto.anyflow.ui

import androidx.annotation.DrawableRes
import androidx.compose.runtime.saveable.Saver
import io.github.yurisismotto.anyflow.R

/**
 * Where the app currently is.
 *
 * A sealed hierarchy and a back stack held in the shell, rather than a
 * navigation library. Five destinations and no deep links do not justify
 * another dependency in an app whose size is a feature; when deep links
 * arrive this is the type a NavHost would be built around anyway.
 */
sealed interface Screen {
    /** The tab this destination lives under, for the bottom bar. */
    val tab: Tab

    data object Devices : Screen {
        override val tab = Tab.Devices
    }

    data object Activity : Screen {
        override val tab = Tab.Activity
    }

    data object Settings : Screen {
        override val tab = Tab.Settings
    }

    /** One computer's permissions, automation and trust. */
    data class PeerDetail(val fingerprintHex: String) : Screen {
        override val tab = Tab.Devices
    }

    /** The deliberate act of sending the clipboard somewhere. */
    data class SendClipboard(val fingerprintHex: String) : Screen {
        override val tab = Tab.Devices
    }
}

enum class Tab(val label: String, @DrawableRes val icon: Int) {
    Devices("Devices", R.drawable.ic_home),
    Activity("Activity", R.drawable.ic_activity),
    Settings("Settings", R.drawable.ic_settings),
}

/**
 * Survives configuration change without dragging in a navigation library.
 *
 * Stored as a short list of strings rather than as parcelables, so that
 * rotating the device on a peer detail screen comes back to the same peer.
 */
val ScreenSaver: Saver<Screen, Any> = Saver(
    save = { screen ->
        when (screen) {
            Screen.Devices -> listOf("devices")
            Screen.Activity -> listOf("activity")
            Screen.Settings -> listOf("settings")
            is Screen.PeerDetail -> listOf("peer", screen.fingerprintHex)
            is Screen.SendClipboard -> listOf("send", screen.fingerprintHex)
        }
    },
    restore = { saved ->
        @Suppress("UNCHECKED_CAST")
        val parts = saved as List<String>
        when (parts.firstOrNull()) {
            "activity" -> Screen.Activity
            "settings" -> Screen.Settings
            "peer" -> parts.getOrNull(1)?.let(Screen::PeerDetail) ?: Screen.Devices
            "send" -> parts.getOrNull(1)?.let(Screen::SendClipboard) ?: Screen.Devices
            else -> Screen.Devices
        }
    },
)
