package io.github.yurisismotto.anyflow

import io.github.yurisismotto.anyflow.ui.Screen
import io.github.yurisismotto.anyflow.ui.ScreenSaver
import io.github.yurisismotto.anyflow.ui.Tab
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/**
 * Where the consent screens sit, and where Back goes from them.
 *
 * The app has no navigation library — five destinations became seven and that
 * is still not a dependency — so the two properties a library would have given
 * are asserted here instead: a rotation or a process death lands on the same
 * screen, and Back walks the hierarchy rather than jumping to the top.
 *
 * The second one matters more than it sounds. The app picker is reached from
 * the notification screen, which is reached from the device card. A Back that
 * went straight to the device list would drop a person out of the middle of
 * granting a permission, and they would have to find their way in again.
 */
class NotificationNavigationTest {

    private val hex = "7e637b4e937b7732"

    private fun roundTrip(screen: Screen): Screen? {
        // `Saver` is a pure Kotlin interface: the save/restore pair is exactly
        // what the platform does across a configuration change, and it needs no
        // device to exercise.
        val saved = with(ScreenSaver) {
            object : androidx.compose.runtime.saveable.SaverScope {
                override fun canBeSaved(value: Any) = true
            }.save(screen)
        }
        return saved?.let { ScreenSaver.restore(it) }
    }

    @Test
    fun the_notification_screen_survives_a_configuration_change() {
        assertEquals(Screen.PeerNotifications(hex), roundTrip(Screen.PeerNotifications(hex)))
    }

    @Test
    fun the_app_picker_survives_a_configuration_change() {
        // Rotating the device halfway through choosing applications must not
        // throw the choice away, or lose which computer it was being made for.
        assertEquals(Screen.AppPicker(hex), roundTrip(Screen.AppPicker(hex)))
    }

    @Test
    fun a_restored_destination_keeps_the_computer_it_was_about() {
        val restored = roundTrip(Screen.AppPicker(hex)) as Screen.AppPicker
        assertEquals(hex, restored.fingerprintHex)
    }

    @Test
    fun a_saved_destination_with_no_peer_falls_back_to_the_device_list() {
        // A truncated or hand-edited saved state must not resolve to a screen
        // about *some* computer; it resolves to no computer at all.
        assertEquals(Screen.Devices, ScreenSaver.restore(listOf("apps")))
        assertEquals(Screen.Devices, ScreenSaver.restore(listOf("notifications")))
    }

    @Test
    fun back_walks_the_hierarchy_rather_than_jumping_to_the_top() {
        assertEquals(Screen.PeerNotifications(hex), Screen.AppPicker(hex).parent)
        assertEquals(Screen.PeerDetail(hex), Screen.PeerNotifications(hex).parent)
        assertEquals(Screen.Devices, Screen.PeerDetail(hex).parent)
    }

    @Test
    fun the_roots_have_no_parent_and_therefore_no_back_arrow() {
        assertNull(Screen.Devices.parent)
        assertNull(Screen.Files.parent)
        assertNull(Screen.Settings.parent)
    }

    @Test
    fun the_consent_screens_live_under_the_devices_tab() {
        // They are per-computer settings, so they belong where the computer is
        // — not in a standalone notifications section that would imply a
        // device-wide switch this feature does not have.
        assertEquals(Tab.Devices, Screen.PeerNotifications(hex).tab)
        assertEquals(Tab.Devices, Screen.AppPicker(hex).tab)
    }
}
