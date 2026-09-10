package io.github.yurisismotto.anyflow

import io.github.yurisismotto.anyflow.notifications.NotificationGates
import io.github.yurisismotto.anyflow.notifications.NotificationReadiness
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The three gates, and the eight states they produce.
 *
 * This is the decision table the consent UI renders, and it is here rather
 * than in a Compose test because the property that matters is not how it is
 * drawn: it is that a UI can never say "Ready" while any gate is shut, and
 * can never say "Off" for a device that is merely disconnected. Both of those
 * are lies a single switch would tell.
 */
class NotificationReadinessTest {

    /** Everything open. Each test below shuts exactly one thing. */
    private fun gates(
        osAccessGranted: Boolean = true,
        peerGranted: Boolean = true,
        allowMirror: Boolean = true,
        allowedAppCount: Int = 3,
        sourceActive: Boolean = true,
        peerConnected: Boolean = true,
        peerIsSink: Boolean = true,
    ) = NotificationGates(
        osAccessGranted = osAccessGranted,
        peerGranted = peerGranted,
        allowMirror = allowMirror,
        allowedAppCount = allowedAppCount,
        sourceActive = sourceActive,
        peerConnected = peerConnected,
        peerIsSink = peerIsSink,
    )

    @Test
    fun `everything open is ready`() {
        assertEquals(NotificationReadiness.READY, gates().readiness())
        assertTrue(gates().readiness().isReady)
    }

    @Test
    fun `a device with no grant is off, whatever else is true`() {
        // Including a device with OS access, a live session and a computer
        // announcing SINK: the grant is the thing that was never given.
        assertEquals(
            NotificationReadiness.SHARING_OFF,
            gates(peerGranted = false).readiness(),
        )
    }

    @Test
    fun `the grant is asked about before the operating system permission`() {
        // Both are missing. Telling somebody to change a device-wide Android
        // setting for a computer they have not authorised would be asking for
        // the wrong permission first.
        assertEquals(
            NotificationReadiness.SHARING_OFF,
            gates(peerGranted = false, osAccessGranted = false).readiness(),
        )
    }

    @Test
    fun `a granted device without android access needs android access`() {
        assertEquals(
            NotificationReadiness.NEEDS_ANDROID_ACCESS,
            gates(osAccessGranted = false).readiness(),
        )
    }

    @Test
    fun `mirroring switched off reads as paused, not as off`() {
        // "Off" and "paused" have different fixes: one is a grant, the other
        // is a switch on this screen.
        assertEquals(NotificationReadiness.PAUSED, gates(allowMirror = false).readiness())
    }

    @Test
    fun `an empty app list is its own state and is not an error`() {
        assertEquals(
            NotificationReadiness.NO_APPS,
            gates(allowedAppCount = 0).readiness(),
        )
    }

    @Test
    fun `no apps is reported even when the device is offline`() {
        // The most useful thing a person can do in that state is choose an
        // application, and they can do it without the computer being on.
        assertEquals(
            NotificationReadiness.NO_APPS,
            gates(allowedAppCount = 0, peerConnected = false).readiness(),
        )
    }

    @Test
    fun `a configured device with no session is not connected`() {
        val readiness = gates(peerConnected = false, sourceActive = false).readiness()
        assertEquals(NotificationReadiness.NOT_CONNECTED, readiness)
        // Configured: the next session mirrors without anybody doing anything.
        assertTrue(readiness.isConfigured)
        assertFalse(readiness.isReady)
    }

    @Test
    fun `the listener not being bound while idle is not an anomaly`() {
        // With no session the listener is deliberately unbound — that is the
        // property `META_DATA_DEFAULT_AUTOBIND=false` buys. Reporting it as
        // "Unavailable" would make the designed idle state look broken.
        assertEquals(
            NotificationReadiness.NOT_CONNECTED,
            gates(sourceActive = false, peerConnected = false).readiness(),
        )
    }

    @Test
    fun `the listener not being bound during a session is unavailable`() {
        assertEquals(
            NotificationReadiness.UNAVAILABLE,
            gates(sourceActive = false).readiness(),
        )
    }

    @Test
    fun `a computer that has not claimed sink is reported as not receiving`() {
        // The state N2 measured on hardware: this phone granted and sourcing,
        // the computer connected, and the computer's own grant absent — so it
        // announced no role and nothing moved. A single switch would have had
        // to draw this as either "on" or "off", and both are wrong.
        val readiness = gates(peerIsSink = false).readiness()
        assertEquals(NotificationReadiness.PEER_NOT_RECEIVING, readiness)
        assertFalse(readiness.isReady)
        assertTrue(readiness.isConfigured)
    }

    @Test
    fun `only the ready state claims to be mirroring`() {
        for (readiness in NotificationReadiness.entries) {
            assertEquals(
                "$readiness",
                readiness == NotificationReadiness.READY,
                readiness.isReady,
            )
        }
    }

    @Test
    fun `every state is reachable from some real gate combination`() {
        // A state nothing can produce is a state nobody ever debugged, and a
        // state two combinations produce is one that hides a difference.
        val reached = setOf(
            gates(peerGranted = false).readiness(),
            gates(osAccessGranted = false).readiness(),
            gates(allowMirror = false).readiness(),
            gates(allowedAppCount = 0).readiness(),
            gates(peerConnected = false).readiness(),
            gates(sourceActive = false).readiness(),
            gates(peerIsSink = false).readiness(),
            gates().readiness(),
        )
        assertEquals(NotificationReadiness.entries.toSet(), reached)
    }
}
