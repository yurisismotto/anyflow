package io.github.yurisismotto.anyflow

import io.github.yurisismotto.anyflow.notifications.NotificationGates
import io.github.yurisismotto.anyflow.notifications.NotificationDismissReadiness
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
        allowDismissSync: Boolean = false,
        peerIsDismissReporter: Boolean = true,
    ) = NotificationGates(
        osAccessGranted = osAccessGranted,
        peerGranted = peerGranted,
        allowMirror = allowMirror,
        allowedAppCount = allowedAppCount,
        sourceActive = sourceActive,
        peerConnected = peerConnected,
        peerIsSink = peerIsSink,
        allowDismissSync = allowDismissSync,
        peerIsDismissReporter = peerIsDismissReporter,
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

    // -----------------------------------------------------------------------
    // Dismissal: its own table, because it fails apart from mirroring
    // -----------------------------------------------------------------------

    /**
     * The default. Everything else is open, so this is the setting and nothing
     * else — which is what makes it a choice rather than a fault.
     */
    @Test
    fun `dismiss sync is off by default and that is not a warning`() {
        assertEquals(
            NotificationDismissReadiness.OFF,
            gates().dismissReadiness(),
        )
        assertFalse(gates().dismissReadiness().needsAttention)
    }

    @Test
    fun `dismiss sync on with everything working is active`() {
        assertEquals(
            NotificationDismissReadiness.ACTIVE,
            gates(allowDismissSync = true).dismissReadiness(),
        )
        assertFalse(gates(allowDismissSync = true).dismissReadiness().needsAttention)
    }

    /**
     * The state §16 exists for. Mirroring is `READY` and dismissal is not, and
     * a single "Ready" bit would have to lie about one of them.
     */
    @Test
    fun `mirroring can be ready while dismissal sync is unavailable`() {
        val g = gates(allowDismissSync = true, peerIsDismissReporter = false)
        assertEquals(NotificationReadiness.READY, g.readiness())
        assertEquals(NotificationDismissReadiness.PEER_CANNOT_REPORT, g.dismissReadiness())
        assertTrue(g.dismissReadiness().needsAttention)
    }

    /**
     * No Android notification access means this phone could not cancel
     * anything for anybody, so that is named first — and the fix is in
     * Settings rather than on this screen.
     */
    @Test
    fun `no android access outranks every other dismissal state`() {
        assertEquals(
            NotificationDismissReadiness.NEEDS_ANDROID_ACCESS,
            gates(
                osAccessGranted = false,
                allowDismissSync = true,
                peerIsDismissReporter = false,
            ).dismissReadiness(),
        )
        // Even with the setting off: "it is off" is not the useful sentence
        // when the whole capability is unavailable.
        assertEquals(
            NotificationDismissReadiness.NEEDS_ANDROID_ACCESS,
            gates(osAccessGranted = false).dismissReadiness(),
        )
    }

    /**
     * When it is off, nothing about a computer's capabilities is mentioned.
     *
     * Warning about a peer that cannot report dismissals, for a feature
     * nobody enabled, is noise — and noise is how an amber badge stops meaning
     * anything.
     */
    @Test
    fun `off says nothing about what the computer can do`() {
        assertEquals(
            NotificationDismissReadiness.OFF,
            gates(peerIsDismissReporter = false).dismissReadiness(),
        )
        assertEquals(
            NotificationDismissReadiness.OFF,
            gates(peerConnected = false).dismissReadiness(),
        )
    }

    /**
     * Turning mirroring off makes a stale flag read as off, the same
     * containment rule the runtime applies — so the screen cannot promise
     * something the runtime will refuse.
     */
    @Test
    fun `mirroring off makes a stale dismiss flag read as off`() {
        assertEquals(
            NotificationDismissReadiness.OFF,
            gates(allowMirror = false, allowDismissSync = true).dismissReadiness(),
        )
    }

    /** A device that is merely elsewhere needs nothing doing. */
    @Test
    fun `a disconnected computer is not connected rather than unable`() {
        val g = gates(allowDismissSync = true, peerConnected = false)
        assertEquals(NotificationDismissReadiness.NOT_CONNECTED, g.dismissReadiness())
        assertFalse(g.dismissReadiness().needsAttention)
    }

    @Test
    fun `every dismissal state is reachable from some real gate combination`() {
        val reached = setOf(
            gates(osAccessGranted = false).dismissReadiness(),
            gates().dismissReadiness(),
            gates(allowDismissSync = true, peerConnected = false).dismissReadiness(),
            gates(allowDismissSync = true, peerIsDismissReporter = false).dismissReadiness(),
            gates(allowDismissSync = true).dismissReadiness(),
        )
        assertEquals(NotificationDismissReadiness.entries.toSet(), reached)
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
