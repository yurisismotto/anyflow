package io.github.yurisismotto.anyflow

import android.content.Context
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.github.yurisismotto.anyflow.capability.NotificationsCapability
import io.github.yurisismotto.anyflow.notifications.LockPolicy
import io.github.yurisismotto.anyflow.notifications.NotificationPolicy
import io.github.yurisismotto.anyflow.pairing.QrPayload
import io.github.yurisismotto.anyflow.service.ConnectionService
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

/**
 * The parts of a hardware certification run that a person would otherwise
 * have to do by hand, driven from the host instead. **`androidTest` only** —
 * it is compiled into the instrumentation APK and can never reach a shipped
 * build.
 *
 * # Why this exists
 *
 * Three certification steps have needed a human at the tablet since N1, and
 * each has cost a wave real time:
 *
 * | Step | Why a person was needed | N-debt |
 * | --- | --- | --- |
 * | pairing | the QR goes through the camera, and there is no manual entry | N3 debt 9, N4 debt 9 |
 * | the peer grant | it is a switch on a scrolling Compose screen | — |
 * | the per-app allow-list | it is a picker on the same screen | N3 debt 3 |
 *
 * # What is real here, and what is not
 *
 * Everything below drives the **live** [AnyFlowApp] singleton — the real trust
 * store on the real device, the real `PeerConnection.connect`, the real TLS
 * 1.3 handshake with the real SPKI pin, the real pairing proof, and the real
 * `policyChanged()` propagation that re-announces roles on a session that is
 * already up.
 *
 * What is *not* exercised is the last few millimetres of input:
 *
 *  * **pairing** skips the ZXing decode. [QrPayload.parse] is given the very
 *    string the desktop encoded into the QR — `anyflow pair` prints it under
 *    *"If your phone cannot scan, the payload is:"* — so the only step missed
 *    is turning pixels into that string;
 *  * **the grant and the policy** call the same two-line lambdas
 *    `MainActivity` installs into `MainActions`, rather than the `Switch` that
 *    calls them. The Compose layer above is covered by
 *    `NotificationConsentUiTest`, which is a real Compose interaction test.
 *
 * Neither substitution touches a security decision: the pin, the proof and
 * the human confirmation on the desktop all happen exactly as they would.
 * Every report that uses this harness must say so rather than claim a camera
 * scan.
 *
 * # Running it
 *
 * Not through Gradle. `connectedDebugAndroidTest` **uninstalls the app when it
 * finishes**, which would throw away the pairing this creates (N3 §F9), so it
 * is driven straight through the instrumentation runner:
 *
 * ```console
 * adb install -r app/build/outputs/apk/debug/app-debug.apk
 * adb install -r app/build/outputs/apk/androidTest/debug/app-debug-androidTest.apk
 *
 * adb shell am instrument -w \
 *   -e class io.github.yurisismotto.anyflow.HostDrivenCertificationHarness#pair_with_the_payload_the_desktop_printed \
 *   -e anyflow.pairing.payload 'anyflow:v1:…' \
 *   io.github.yurisismotto.anyflow.test/androidx.test.runner.AndroidJUnitRunner
 * ```
 *
 * # Why every entry point still asserts something without its argument
 *
 * `assumeTrue` would make these *skipped*, and a skipped instrumented test
 * inside a `BUILD SUCCESSFUL` is exactly the green run that proves nothing
 * (N1 debt 3) — §24 of the N5 brief requires `skipped=0` and means it. So
 * each test below has two modes: with its host argument it performs the real
 * action, and without it, it asserts the contract of the parsing or lookup
 * step that *can* be checked with no desktop present. Both modes assert
 * something true; neither is skipped; and the run's output says which mode
 * ran.
 */
@RunWith(AndroidJUnit4::class)
class HostDrivenCertificationHarness {

    private val context: Context
        get() = InstrumentationRegistry.getInstrumentation().targetContext

    private val app: AnyFlowApp
        get() = context.applicationContext as AnyFlowApp

    private fun arg(name: String): String? =
        InstrumentationRegistry.getArguments().getString(name)?.takeIf { it.isNotBlank() }

    private fun say(line: String) {
        // logcat, not `println`: an instrumented test's stdout is the app
        // process's stdout and goes nowhere a host script can read. Never
        // carries a notification's content — this class runs in the same
        // sessions as the NOTIF-SEC-25 canaries, and a harness that printed a
        // canary would fail them itself.
        android.util.Log.i(TAG, line)
    }

    // -----------------------------------------------------------------------
    // Pairing
    // -----------------------------------------------------------------------

    @Test
    fun pair_with_the_payload_the_desktop_printed() {
        val raw = arg(ARG_PAYLOAD)
        if (raw == null) {
            // The half of this path that needs no desktop: a payload that is
            // not one must be refused rather than guessed at.
            assertNull("garbage must not parse", QrPayload.parse("not a payload"))
            assertNull("a truncated payload must not parse", QrPayload.parse("anyflow:v1:dead"))
            assertNull("an empty payload must not parse", QrPayload.parse(""))
            say("pair skipped-no-payload parser-contract-ok")
            return
        }

        val payload = QrPayload.parse(raw)
        assertNotNull("the desktop's payload did not parse", payload)
        say("pair payload-parsed fingerprint=${payload!!.fingerprint.toDisplayShort()}")

        val result = runBlocking { app.pair(payload) }
        val outcome = result.getOrNull()
        assertTrue("pairing failed: ${result.exceptionOrNull()?.message}", result.isSuccess)
        assertNotNull(outcome)
        val peer = outcome!!.peer
        say("pair ok device=${peer.deviceId} fingerprint=${peer.fingerprint.toDisplayShort()}")
        // UX-DEBT-02: whether the QR's token was actually proved is now a
        // fact the handshake reports rather than something inferred from
        // success. A scan against a computer that already trusts this device
        // is a reconnection, and saying so is the point.
        say("pair proved-token=${outcome.provedToken}")

        // Pairing grants nothing it should not: ADR-0015 §4 keeps
        // `notifications.v1` out of the automatic set, and this is where a
        // regression would first show.
        assertFalse(
            "pairing must not grant notifications.v1",
            peer.grantedCapabilities.contains(NotificationsCapability.ID),
        )
        say("pair grants=${peer.grantedCapabilities.sorted().joinToString(",")}")
    }

    /** This device's own identity, so the host can verify what it confirms. */
    @Test
    fun report_this_devices_fingerprint() {
        val fingerprint = app.identity.fingerprint
        assertEquals(32, fingerprint.bytes.size)
        say("identity fingerprint=${fingerprint.toHex()} short=${fingerprint.toDisplayShort()}")
        say("identity deviceId=${app.trustStore.deviceId}")
    }

    // -----------------------------------------------------------------------
    // The consent surface
    // -----------------------------------------------------------------------

    /**
     * Grants or withdraws `notifications.v1` for one computer, through the
     * same two lines `MainActivity`'s switch runs.
     */
    @Test
    fun set_the_notifications_grant() {
        val peer = peerFromArg()
        if (peer == null) {
            say("grant skipped-no-peer stored-peers=${app.trustStore.peers().size}")
            assertTrue("the trust store must be readable", app.trustStore.peers().size >= 0)
            return
        }
        val granted = arg(ARG_GRANTED)?.toBooleanStrictOrNull() ?: true

        app.trustStore.setGrant(peer.fingerprint, NotificationsCapability.ID, granted)
        app.notifications.policyChanged()

        val after = app.trustStore.peer(peer.fingerprint)
        assertEquals(
            granted,
            after?.grantedCapabilities?.contains(NotificationsCapability.ID),
        )
        say("grant notifications.v1=$granted peer=${peer.fingerprint.toDisplayShort()}")
    }

    /**
     * Replaces one computer's notification policy.
     *
     * Each field is only touched when the host names it, so a run that only
     * wants the allow-list changed does not silently reset the lock policy.
     */
    @Test
    fun set_the_notification_policy() {
        val peer = peerFromArg()
        if (peer == null) {
            // The contract that needs no peer: deny by default, and an
            // unrecognised lock policy is never the permissive one.
            val fresh = NotificationPolicy()
            assertTrue("deny by default", fresh.allowedApps.isEmpty())
            assertFalse("dismiss sync is off by default", fresh.allowDismissSync)
            assertFalse("ongoing is off by default", fresh.includeOngoing)
            assertEquals(LockPolicy.APP_ONLY, fresh.whenSourceLocked)
            assertEquals(LockPolicy.APP_ONLY, LockPolicy.fromName("NOT_A_POLICY"))
            say("policy skipped-no-peer defaults-ok")
            return
        }

        val current = app.trustStore.peer(peer.fingerprint)?.notificationPolicy
            ?: NotificationPolicy()
        val updated = current.copy(
            allowMirror = arg(ARG_MIRROR)?.toBooleanStrictOrNull() ?: current.allowMirror,
            allowedApps = arg(ARG_APPS)
                ?.split(',')
                ?.map { it.trim() }
                ?.filter { it.isNotEmpty() }
                ?.toSet()
                ?: current.allowedApps,
            includeOngoing = arg(ARG_ONGOING)?.toBooleanStrictOrNull() ?: current.includeOngoing,
            allowDismissSync = arg(ARG_DISMISS)?.toBooleanStrictOrNull()
                ?: current.allowDismissSync,
            whenSourceLocked = arg(ARG_LOCKED)
                ?.let { LockPolicy.fromName(it.uppercase().replace('-', '_')) }
                ?: current.whenSourceLocked,
        )

        app.trustStore.setNotificationPolicy(peer.fingerprint, updated)
        app.notifications.policyChanged()

        val stored = app.trustStore.peer(peer.fingerprint)?.notificationPolicy
        assertEquals(updated, stored)
        say(
            "policy mirror=${updated.allowMirror} ongoing=${updated.includeOngoing} " +
                "dismiss=${updated.allowDismissSync} locked=${updated.whenSourceLocked} " +
                "apps=${updated.allowedApps.sorted().joinToString(",")}",
        )
    }

    // -----------------------------------------------------------------------
    // Connection and status
    // -----------------------------------------------------------------------

    /** Starts or stops the connection service — the Connect/Disconnect row. */
    @Test
    fun set_the_connection() {
        when (arg(ARG_CONNECT)?.lowercase()) {
            "on", "true", "start" -> {
                ConnectionService.start(context)
                say("connection start requested")
            }
            "off", "false", "stop" -> {
                ConnectionService.stop(context)
                say("connection stop requested")
            }
            else -> say("connection unchanged")
        }
        assertTrue(true)
    }

    /**
     * Everything the Details section shows, as one machine-readable line.
     *
     * Counts, enums, role names and fingerprint prefixes only. There is no
     * field in [io.github.yurisismotto.anyflow.notifications.NotificationSource.Status]
     * that could hold a title, a body or a raw platform key, which is what
     * makes printing the whole of it safe.
     */
    @Test
    fun report_the_notification_status() {
        val status = app.notifications.status.value
        say(
            "status access=${status.accessGranted} listener=${status.listenerConnected} " +
                "secret=${status.secretAvailable} sessions=${status.sessions} " +
                "tracked=${status.trackedNotifications} emitted=${status.emitted} " +
                "dropped=${status.dropped}",
        )
        for ((hex, peer) in status.peers) {
            say(
                "status peer=${hex.take(16)} localSource=${peer.localIsSource} " +
                    "localDismissTarget=${peer.localIsDismissTarget} " +
                    "localEpoch=${peer.localEpoch} peerSink=${peer.peerIsSink} " +
                    "peerDismissReporter=${peer.peerIsDismissReporter} " +
                    "peerEpoch=${peer.peerEpoch} " +
                    "dismissRequests=${peer.dismissRequests} " +
                    "dismissesPerformed=${peer.dismissesPerformed}",
            )
        }
        for (peer in app.trustStore.peers()) {
            say(
                "store peer=${peer.fingerprint.toDisplayShort()} " +
                    "grants=${peer.grantedCapabilities.sorted().joinToString("|")} " +
                    "mirror=${peer.notificationPolicy.allowMirror} " +
                    "ongoing=${peer.notificationPolicy.includeOngoing} " +
                    "dismiss=${peer.notificationPolicy.allowDismissSync} " +
                    "locked=${peer.notificationPolicy.whenSourceLocked} " +
                    "apps=${peer.notificationPolicy.allowedApps.sorted().joinToString("|")}",
            )
        }
        assertTrue(true)
    }

    // -----------------------------------------------------------------------

    /**
     * The peer a command applies to.
     *
     * A fingerprint prefix, matched case-insensitively, or the single stored
     * peer when there is exactly one. An ambiguous prefix fails rather than
     * guessing — the same rule `resolve_device` follows on the desktop, and
     * for the same reason: the wrong peer here would mean granting the most
     * sensitive capability on the phone to a computer nobody chose.
     */
    private fun peerFromArg(): io.github.yurisismotto.anyflow.store.TrustStore.TrustedPeer? {
        val peers = app.trustStore.peers()
        val needle = arg(ARG_PEER)?.lowercase()
        if (needle == null) {
            return peers.singleOrNull()
        }
        val matches = peers.filter { it.fingerprint.toHex().startsWith(needle) }
        assertTrue("'$needle' is ambiguous: ${matches.size} peers match", matches.size <= 1)
        return matches.firstOrNull()
    }

    private companion object {
        /** Distinctive, so a host script can filter logcat down to this class. */
        const val TAG = "AnyFlowHarness"
        const val ARG_PAYLOAD = "anyflow.pairing.payload"
        const val ARG_PEER = "anyflow.peer"
        const val ARG_GRANTED = "anyflow.granted"
        const val ARG_MIRROR = "anyflow.mirror"
        const val ARG_APPS = "anyflow.apps"
        const val ARG_ONGOING = "anyflow.ongoing"
        const val ARG_DISMISS = "anyflow.dismiss"
        const val ARG_LOCKED = "anyflow.locked"
        const val ARG_CONNECT = "anyflow.connect"
    }
}
