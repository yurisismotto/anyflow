package io.github.yurisismotto.omnibridge

import com.google.protobuf.ByteString
import io.github.yurisismotto.omnibridge.identity.Fingerprint
import io.github.yurisismotto.omnibridge.notifications.LockPolicy
import io.github.yurisismotto.omnibridge.notifications.LockState
import io.github.yurisismotto.omnibridge.notifications.NotificationLimits
import io.github.yurisismotto.omnibridge.notifications.NotificationMapping
import io.github.yurisismotto.omnibridge.notifications.NotificationOutboundQueue
import io.github.yurisismotto.omnibridge.notifications.NotificationPolicy
import io.github.yurisismotto.omnibridge.notifications.NotificationSecret
import io.github.yurisismotto.omnibridge.notifications.NotificationSource
import io.github.yurisismotto.omnibridge.notifications.PlatformNotification
import io.github.yurisismotto.omnibridge.proto.capabilities.DismissRequest
import io.github.yurisismotto.omnibridge.proto.capabilities.NotificationControl
import io.github.yurisismotto.omnibridge.proto.capabilities.NotificationOutcome
import io.github.yurisismotto.omnibridge.proto.capabilities.NotificationRole
import io.github.yurisismotto.omnibridge.proto.capabilities.NotificationRoles
import io.github.yurisismotto.omnibridge.proto.capabilities.SyncMarker
import javax.crypto.SecretKey
import javax.crypto.spec.SecretKeySpec
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.cancel
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * U2 P3 — notification roles converging on a session that is already up.
 *
 * # The defect these tests exist for
 *
 * U2 §39.19 and §40.13 recorded the same failure on Ubuntu 24.04, Debian 13
 * and Ubuntu 26.04, across three UPower-era GNOME versions: a paired, healthy,
 * *connected* session in which the person turned notification sharing on and
 * nothing was ever mirrored. The desktop was a `SINK` throughout and said so;
 * the phone went on reporting `The computer announces: no role · epoch 0` for
 * the rest of the session, and the only recovery anyone found was restarting
 * the desktop daemon — after which mirrors appeared in under five seconds.
 *
 * The cause is one line on this side. [NotificationSource.handleInbound]
 * applied the per-peer `notifications.v1` grant to **every** inbound body,
 * including a peer's role announcement — and the desktop announces its roles
 * exactly once, when the session comes up. Because the app deliberately
 * withholds the notifications grant at pairing, the ordinary order of events
 * (pair, then turn sharing on) put that single announcement inside the window
 * where it was refused. `answer` has no id to echo for a `ROLES` body, so it
 * vanished without a reply and without a log line. A daemon restart worked
 * only because it built a new session, whose announcement arrived after the
 * grant existed.
 *
 * A role is not an authorization: [PeerRoleState] is read only to *withhold*
 * content, every send path re-reads the grant for itself, and the desktop's
 * own `handle_control` has always exempted `Roles` from its grant check. The
 * fix is the two ends agreeing.
 *
 * # How these tests are written
 *
 * One ordered producer, driven on the test scheduler, so every assertion reads
 * the wire after the exact events it names — no sleeps, no wall-clock, and no
 * reconnect anywhere. Every fixture is obviously synthetic and the canaries
 * assert none of it is rendered.
 */
@OptIn(ExperimentalCoroutinesApi::class)
class NotificationConvergenceTest {

    private val allowedApp = "example.fixture.app"
    private val peer = Fingerprint(ByteArray(32) { 0x11 })
    private val otherPeer = Fingerprint(ByteArray(32) { 0x22 })

    private val producers = mutableListOf<CoroutineScope>()

    private fun TestScope.producerScope(): CoroutineScope =
        CoroutineScope(UnconfinedTestDispatcher(testScheduler)).also { producers += it }

    @After
    fun stopProducers() {
        producers.forEach { it.cancel() }
        producers.clear()
    }

    // -- fakes ---------------------------------------------------------------

    private class FakeSecretStore : NotificationSecret.Store {
        var stored: SecretKey? = SecretKeySpec(ByteArray(32) { it.toByte() }, "HmacSHA256")
        override fun load(): SecretKey? = stored
        override fun create(): SecretKey =
            SecretKeySpec(ByteArray(32) { (it + 100).toByte() }, "HmacSHA256").also { stored = it }
        override fun destroy() { stored = null }
    }

    private class FakeMetadata(override var notificationSecretGeneration: Int = 1) :
        NotificationSecret.Metadata

    private class FakeListener(
        var active: List<PlatformNotification> = emptyList(),
    ) : NotificationSource.ListenerControl {
        var unbinds = 0
        val cancelled = mutableListOf<String>()
        override fun requestUnbind() { unbinds += 1 }
        override fun activeNotifications(): List<PlatformNotification> = active
        override fun activeNotification(platformKey: String): PlatformNotification? =
            active.firstOrNull { it.platformKey == platformKey }
        override fun cancel(platformKey: String): Boolean {
            cancelled += platformKey
            active = active.filterNot { it.platformKey == platformKey }
            return true
        }
        override fun activePackages(): List<String> = active.map { it.packageName }.distinct()
    }

    private class FakeAccess(var granted: Boolean = true) : NotificationSource.AccessControl {
        var binds = 0
        override fun isAccessGranted(): Boolean = granted
        override fun requestBind() { binds += 1 }
    }

    /** Everything one peer was sent, in the order it was sent. */
    private class Wire {
        val messages = mutableListOf<NotificationControl>()
        val send: suspend (ByteString) -> Unit = { payload ->
            messages += NotificationControl.parseFrom(payload)
        }

        fun bodies(): List<String> = messages.map {
            when (it.bodyCase) {
                NotificationControl.BodyCase.SYNC ->
                    "sync:${it.sync.phase.name.removePrefix("PHASE_")}"
                NotificationControl.BodyCase.ROLES ->
                    "roles:${it.roles.rolesCount}@${it.roles.epoch}"
                NotificationControl.BodyCase.UPSERT -> "upsert:${it.upsert.appId}"
                NotificationControl.BodyCase.REMOVE -> "remove"
                NotificationControl.BodyCase.RESULT -> "result:${it.result.outcome.name}"
                else -> it.bodyCase.name
            }
        }

        fun roles() = messages.filter { it.bodyCase == NotificationControl.BodyCase.ROLES }
        fun upserts() = messages.filter { it.bodyCase == NotificationControl.BodyCase.UPSERT }
        fun markers() = messages.filter { it.bodyCase == NotificationControl.BodyCase.SYNC }
        fun lastRoles(): NotificationRoles = roles().last().roles
        fun clear() = messages.clear()
    }

    private fun notification(
        key: String = "0|$allowedApp|1|null|10123",
        packageName: String = allowedApp,
        title: String = "FIXTURE-TITLE-CANARY",
        body: String = "FIXTURE-BODY-CANARY",
        ongoing: Boolean = false,
        visibility: Int = NotificationMapping.VISIBILITY_PRIVATE,
        postedAt: Long = 1_700_000_000_000L,
    ) = PlatformNotification(
        platformKey = key,
        packageName = packageName,
        secondaryProfile = false,
        postedAtUnixMs = postedAt,
        ongoing = ongoing,
        clearable = !ongoing,
        visibility = visibility,
        androidImportance = NotificationMapping.ANDROID_IMPORTANCE_DEFAULT,
        category = "msg",
        groupKey = "0|$packageName|g:fixture",
        groupSummary = false,
        title = title,
        body = body,
        hasProgress = false,
        progressCurrent = 0,
        progressMax = 0,
        progressIndeterminate = false,
    )

    /** The desktop's own announcement: `SINK` + `DISMISS_REPORTER`, as Linux sends it. */
    private fun desktopRoles(
        epoch: Int = 1,
        vararg roles: NotificationRole = arrayOf(
            NotificationRole.NOTIFICATION_ROLE_SINK,
            NotificationRole.NOTIFICATION_ROLE_DISMISS_REPORTER,
        ),
    ): NotificationControl = NotificationControl.newBuilder()
        .setRoles(
            NotificationRoles.newBuilder()
                .addAllRoles(roles.toList())
                .setEpoch(epoch),
        )
        .build()

    private class Harness(
        val listener: FakeListener = FakeListener(),
        val access: FakeAccess = FakeAccess(),
        var locked: Boolean = false,
    ) {
        val metadata = FakeMetadata()
        val secretStore = FakeSecretStore()

        /** Per-peer policy, so a grant can be made and withdrawn mid-session. */
        val policies = mutableMapOf<String, NotificationPolicy>()

        val source = NotificationSource(
            ownPackage = "io.github.yurisismotto.omnibridge",
            localDeviceId = "0123456789abcdef0123456789abcdef",
            authorizer = { fingerprint ->
                policies[fingerprint.toHex()] ?: NotificationPolicy.DENIED
            },
            appLabels = { packageName -> "Label for $packageName" },
            secretProvider = { NotificationSecret.loadOrCreate(secretStore, metadata) },
            lockState = LockState { locked },
            access = access,
            queue = NotificationOutboundQueue(NotificationLimits.EVENT_QUEUE_CAPACITY),
        )

        fun allow(peer: Fingerprint, policy: NotificationPolicy = sharing()) {
            policies[peer.toHex()] = policy
        }

        fun deny(peer: Fingerprint) {
            policies[peer.toHex()] = NotificationPolicy.DENIED
        }

        companion object {
            fun sharing(dismissSync: Boolean = false) = NotificationPolicy(
                allowMirror = true,
                allowedApps = setOf("example.fixture.app"),
                allowDismissSync = dismissSync,
            )
        }
    }

    private fun sharing(dismissSync: Boolean = false) = Harness.sharing(dismissSync)

    // ========================================================================
    // §16 A — the baseline
    // ========================================================================

    /**
     * **A — a fresh session is told this device's roles before anything else.**
     *
     * Unchanged by P3, and asserted here so the rest of the file has a
     * baseline that is known not to have moved.
     */
    @Test
    fun `A session establishment announces roles first`() = runTest {
        val harness = Harness()
        harness.allow(peer)
        val wire = Wire()
        harness.source.start(producerScope())
        harness.source.attachListener(harness.listener)
        harness.source.onListenerConnected()
        harness.source.attachSession(peer, "peer-device", wire.send)
        advanceUntilIdle()

        assertEquals("roles:2@1", wire.bodies().first())
    }

    // ========================================================================
    // THE DEFECT OF RECORD
    // ========================================================================

    /**
     * **The certified reproduction, as one assertion.**
     *
     * U2 §39.19 / §40.13. The peer announces `SINK` at the instant the session
     * comes up — which is *before* the person has granted this computer
     * anything, because the app withholds `notifications.v1` at pairing. That
     * announcement must be recorded, or the phone spends the rest of the
     * session believing the computer announced nothing.
     *
     * Against the pre-P3 code this fails: the announcement was refused with
     * the grant check, silently, and `peerRoles` stayed at epoch 0.
     */
    @Test
    fun `an ungranted peer's role announcement is still recorded`() = runTest {
        val harness = Harness()
        harness.deny(peer) // paired, not yet allowed to see notifications
        val wire = Wire()
        harness.source.start(producerScope())
        harness.source.attachListener(harness.listener)
        harness.source.onListenerConnected()
        harness.source.attachSession(peer, "peer-device", wire.send)
        harness.source.onInbound(peer, desktopRoles())
        advanceUntilIdle()

        val status = harness.source.status.value.peers.getValue(peer.toHex())
        assertTrue("the computer said SINK and this phone must remember it", status.peerIsSink)
        assertTrue(status.peerIsDismissReporter)
        assertEquals(1, status.peerEpoch)

        // And recording it widened nothing: an ungranted peer is still sent no
        // content at all.
        harness.source.onPosted(notification())
        advanceUntilIdle()
        assertTrue("a role is not a grant", wire.upserts().isEmpty())
        assertTrue(wire.markers().isEmpty())
    }

    /**
     * **The whole defect, end to end, inside one session.**
     *
     * Brief §6 and §20, as a unit test: a live session, sharing not yet
     * effective, then the person turns it on — no reconnect, no restart, no
     * re-pairing — and the notifications that were *already on the shade*
     * mirror, followed by exactly one mirror for the next one to arrive.
     */
    @Test
    fun `B enabling sharing mid-session converges without a reconnect`() = runTest {
        val listener = FakeListener(
            active = listOf(
                notification(key = "0|$allowedApp|1|a|10123"),
                notification(key = "0|$allowedApp|2|b|10123"),
                notification(key = "0|$allowedApp|3|c|10123"),
            ),
        )
        // Notification access is not yet granted, so this phone cannot source.
        val harness = Harness(listener = listener, access = FakeAccess(granted = false))
        harness.deny(peer)
        val wire = Wire()
        harness.source.start(producerScope())
        harness.source.attachListener(listener)
        harness.source.attachSession(peer, "peer-device", wire.send)
        harness.source.onInbound(peer, desktopRoles())
        advanceUntilIdle()

        // Recorded before: connected, roles announced as nothing, no mirrors.
        val before = harness.source.status.value.peers.getValue(peer.toHex())
        assertFalse(before.localIsSource)
        assertEquals(1L, before.localEpoch) // the empty set, announced honestly
        assertTrue(before.peerIsSink)
        assertEquals(1, before.peerEpoch)
        assertEquals(listOf("roles:0@1"), wire.bodies())
        wire.clear()

        // The person turns it on: the grant, then notification access, then
        // the system binds the listener. No session is touched.
        harness.allow(peer)
        harness.source.policyChanged()
        advanceUntilIdle()
        harness.access.granted = true
        harness.source.policyChanged()
        advanceUntilIdle()
        harness.source.onListenerConnected()
        advanceUntilIdle()

        // The role transition crossed the session that was already up.
        val after = harness.source.status.value.peers.getValue(peer.toHex())
        assertTrue(after.localIsSource)
        assertTrue(after.localIsDismissTarget)
        assertEquals(2L, after.localEpoch)
        assertEquals("roles:2@2", wire.bodies().first())

        // And the snapshot followed it, bracketed, with the three that were
        // already there — and no duplicates.
        assertEquals(
            listOf(
                "roles:2@2",
                "sync:BEGIN",
                "upsert:$allowedApp",
                "upsert:$allowedApp",
                "upsert:$allowedApp",
                "sync:END",
            ),
            wire.bodies(),
        )
        assertEquals(
            "each active notification exactly once",
            3,
            wire.upserts().map { it.upsert.notificationId }.distinct().size,
        )

        // A fourth arrives: exactly one new mirror, and no second snapshot.
        wire.clear()
        harness.source.onPosted(notification(key = "0|$allowedApp|4|d|10123"))
        advanceUntilIdle()
        assertEquals(listOf("upsert:$allowedApp"), wire.bodies())
    }

    /**
     * **A grant made while the listener is already bound still converges.**
     *
     * The case `onListenerConnected` cannot rescue: another peer is connected
     * and eligible, so the listener never unbinds and never rebinds, the role
     * set does not move, and there is no announcement to ride on. Before P3
     * this peer received its first mirror only when the *next* notification
     * happened to be posted; the shade it was already entitled to never came.
     */
    @Test
    fun `a grant made with the listener already bound sends the snapshot`() = runTest {
        val listener = FakeListener(active = listOf(notification()))
        val harness = Harness(listener = listener)
        harness.allow(otherPeer) // keeps the listener bound throughout
        harness.deny(peer)
        val other = Wire()
        val wire = Wire()
        harness.source.start(producerScope())
        harness.source.attachListener(listener)
        harness.source.onListenerConnected()
        harness.source.attachSession(otherPeer, "other-device", other.send)
        harness.source.attachSession(peer, "peer-device", wire.send)
        harness.source.onInbound(peer, desktopRoles())
        advanceUntilIdle()
        wire.clear()
        val bindsBefore = harness.access.binds
        val unbindsBefore = listener.unbinds

        harness.allow(peer)
        harness.source.policyChanged()
        advanceUntilIdle()

        assertEquals(
            listOf("sync:BEGIN", "upsert:$allowedApp", "sync:END"),
            wire.bodies(),
        )
        assertEquals("the listener never cycled", bindsBefore, harness.access.binds)
        assertEquals(unbindsBefore, listener.unbinds)
    }

    /**
     * **Choosing the apps *after* turning sharing on still resyncs.**
     *
     * The ordinary order, and the one the hardware run caught: a person turns
     * sharing on, gets a snapshot that is correctly empty because no app has
     * been chosen yet, and then chooses one. Before P3 the connection was
     * marked as having had its snapshot and never got another, so everything
     * already on the shade stayed invisible and only the *next* notification to
     * arrive was mirrored.
     *
     * A snapshot is a statement about what this peer is entitled to see, so it
     * is owed again when that changes — and only when it changes, which is the
     * next test.
     */
    @Test
    fun `choosing apps after enabling sharing sends the snapshot`() = runTest {
        val listener = FakeListener(active = listOf(notification()))
        val harness = Harness(listener = listener)
        // Granted, mirroring on — but no app chosen, which is what the toggle
        // leaves behind.
        harness.allow(peer, NotificationPolicy(allowMirror = true, allowedApps = emptySet()))
        val wire = Wire()
        harness.source.start(producerScope())
        harness.source.attachListener(listener)
        harness.source.onListenerConnected()
        harness.source.attachSession(peer, "peer-device", wire.send)
        harness.source.onInbound(peer, desktopRoles())
        advanceUntilIdle()

        // The correct empty bracket: entitled to nothing, told so.
        assertEquals(listOf("roles:2@1", "sync:BEGIN", "sync:END"), wire.bodies())
        wire.clear()

        // The person picks an app. No role changes, no listener event.
        harness.allow(peer, sharing())
        harness.source.policyChanged()
        advanceUntilIdle()

        assertEquals(
            listOf("sync:BEGIN", "upsert:$allowedApp", "sync:END"),
            wire.bodies(),
        )
    }

    /**
     * **A policy write that changes nothing sends no second snapshot.**
     *
     * The other half of the rule above. `knownApps` in particular is rewritten
     * every time the app picker lists the installed applications, and resyncing
     * because a list was drawn would be the announcement storm in snapshot
     * form.
     */
    @Test
    fun `a policy write that changes no entitlement sends no snapshot`() = runTest {
        val listener = FakeListener(active = listOf(notification()))
        val harness = Harness(listener = listener)
        harness.allow(peer)
        val wire = Wire()
        harness.source.start(producerScope())
        harness.source.attachListener(listener)
        harness.source.onListenerConnected()
        harness.source.attachSession(peer, "peer-device", wire.send)
        harness.source.onInbound(peer, desktopRoles())
        advanceUntilIdle()
        val settled = wire.messages.size

        // The picker was opened: same entitlement, a longer `knownApps`.
        harness.allow(
            peer,
            sharing().copy(knownApps = setOf("com.example.one", "com.example.two")),
        )
        repeat(4) { harness.source.policyChanged() }
        // And the lock policy changed, which reduces content but entitles
        // nothing new — it must not replay what the lock withheld.
        harness.allow(
            peer,
            sharing().copy(whenSourceLocked = LockPolicy.FULL),
        )
        harness.source.policyChanged()
        advanceUntilIdle()

        assertEquals("no resync for an unchanged entitlement", settled, wire.messages.size)
    }

    // ========================================================================
    // §7 / §16 C — revocation, the security-critical direction
    // ========================================================================

    /**
     * **C — revoking mid-session stops content and gives up the role.**
     *
     * The certified narrowing path (ADR-0017): withdrawing the last eligible
     * peer's grant unbinds the listener, which narrows the announced set on
     * the session that is already up, which is what the desktop acts on to
     * take its mirrors off the screen. No reconnect, and no restart.
     */
    @Test
    fun `C revoking sharing mid-session narrows the role and stops content`() = runTest {
        val listener = FakeListener(active = listOf(notification()))
        val harness = Harness(listener = listener)
        harness.allow(peer)
        val wire = Wire()
        harness.source.start(producerScope())
        harness.source.attachListener(listener)
        harness.source.onListenerConnected()
        harness.source.attachSession(peer, "peer-device", wire.send)
        harness.source.onInbound(peer, desktopRoles())
        advanceUntilIdle()
        assertTrue(wire.upserts().isNotEmpty())
        wire.clear()
        // A baseline, not zero: `onListenerConnected` with no session attached
        // yet already released the listener once, which is the existing
        // "bound only while an eligible peer is connected" rule doing its job.
        val unbindsBefore = listener.unbinds

        harness.deny(peer)
        harness.source.policyChanged()
        advanceUntilIdle()
        // The last eligible peer is gone, so the platform is asked to unbind.
        assertEquals(unbindsBefore + 1, listener.unbinds)
        harness.source.onListenerDisconnected()
        advanceUntilIdle()

        val status = harness.source.status.value.peers.getValue(peer.toHex())
        assertFalse("authority given up", status.localIsSource)
        assertFalse(status.localIsDismissTarget)
        assertEquals(2L, status.localEpoch)
        assertEquals("roles:0@2", wire.bodies().last())

        // New content is refused from here on, and nothing else was sent.
        wire.clear()
        harness.source.onPosted(notification(key = "0|$allowedApp|9|z|10123"))
        advanceUntilIdle()
        assertTrue("fail-closed", wire.messages.isEmpty())
    }

    /**
     * **Revocation bites even while this device can still physically source.**
     *
     * The multi-peer shape: another computer keeps the listener bound, so the
     * role set does not narrow and must not — a role says what the machine can
     * do. What stops is the *content*, through the per-peer grant, re-read for
     * every notification. This is the property that makes the convergence work
     * above safe, and it is asserted separately from the role so that neither
     * can be made to stand in for the other.
     */
    @Test
    fun `revocation stops content for one peer while another keeps sourcing`() = runTest {
        val listener = FakeListener(active = listOf(notification()))
        val harness = Harness(listener = listener)
        harness.allow(peer)
        harness.allow(otherPeer)
        val wire = Wire()
        val other = Wire()
        harness.source.start(producerScope())
        harness.source.attachListener(listener)
        harness.source.onListenerConnected()
        harness.source.attachSession(peer, "peer-device", wire.send)
        harness.source.attachSession(otherPeer, "other-device", other.send)
        harness.source.onInbound(peer, desktopRoles())
        harness.source.onInbound(otherPeer, desktopRoles())
        advanceUntilIdle()
        wire.clear()
        other.clear()
        val unbindsBefore = listener.unbinds

        harness.deny(peer)
        harness.source.policyChanged()
        advanceUntilIdle()
        assertEquals(
            "the listener stays bound for the other peer",
            unbindsBefore,
            listener.unbinds,
        )

        harness.source.onPosted(notification(key = "0|$allowedApp|7|g|10123"))
        advanceUntilIdle()

        assertTrue("the revoked peer is sent nothing", wire.upserts().isEmpty())
        assertEquals("the other peer is unaffected", 1, other.upserts().size)
    }

    // ========================================================================
    // §8 / §16 D — re-enable in the same session
    // ========================================================================

    /**
     * **D — disabled → enabled → disabled → enabled, inside one session.**
     *
     * Brief §8. Each transition is monotonic, the second enable resyncs rather
     * than resuming mid-stream, and no old terminal state is replayed.
     */
    @Test
    fun `D the full toggle cycle converges inside one session`() = runTest {
        val listener = FakeListener(active = listOf(notification()))
        val harness = Harness(listener = listener, access = FakeAccess(granted = false))
        harness.deny(peer)
        val wire = Wire()
        harness.source.start(producerScope())
        harness.source.attachListener(listener)
        harness.source.attachSession(peer, "peer-device", wire.send)
        harness.source.onInbound(peer, desktopRoles())
        advanceUntilIdle()

        // ON
        harness.allow(peer)
        harness.access.granted = true
        harness.source.policyChanged()
        advanceUntilIdle()
        harness.source.onListenerConnected()
        advanceUntilIdle()
        assertEquals(2L, harness.source.status.value.peers.getValue(peer.toHex()).localEpoch)
        assertEquals(1, wire.markers().count { it.sync.phase == SyncMarker.Phase.PHASE_BEGIN })

        // OFF
        harness.deny(peer)
        harness.source.policyChanged()
        advanceUntilIdle()
        harness.source.onListenerDisconnected()
        advanceUntilIdle()
        assertEquals(3L, harness.source.status.value.peers.getValue(peer.toHex()).localEpoch)

        // ON again — and the peer is resynced, not resumed.
        wire.clear()
        harness.allow(peer)
        harness.source.policyChanged()
        advanceUntilIdle()
        harness.source.onListenerConnected()
        advanceUntilIdle()

        val status = harness.source.status.value.peers.getValue(peer.toHex())
        assertEquals(4L, status.localEpoch)
        assertTrue(status.localIsSource)
        assertEquals(
            listOf("roles:2@4", "sync:BEGIN", "upsert:$allowedApp", "sync:END"),
            wire.bodies(),
        )
        // One session throughout: the peer's own epoch never restarted, which
        // is what proves no reconnect happened.
        assertEquals(1, status.peerEpoch)
    }

    // ========================================================================
    // §9 / §16 E — recomputation that changes nothing
    // ========================================================================

    /**
     * **E — repeated events that change no role announce nothing.**
     *
     * Brief §9. The original bug must not be replaced by a noisy one: a
     * duplicate settings callback, a repeated grant write and a second
     * listener-ready must all be free.
     */
    @Test
    fun `E an unchanged role set produces no epoch bump and no traffic`() = runTest {
        val listener = FakeListener(active = listOf(notification()))
        val harness = Harness(listener = listener)
        harness.allow(peer)
        val wire = Wire()
        harness.source.start(producerScope())
        harness.source.attachListener(listener)
        harness.source.onListenerConnected()
        harness.source.attachSession(peer, "peer-device", wire.send)
        harness.source.onInbound(peer, desktopRoles())
        advanceUntilIdle()
        val settled = wire.messages.size
        val epoch = harness.source.status.value.peers.getValue(peer.toHex()).localEpoch

        repeat(8) {
            harness.source.policyChanged()
            harness.source.onListenerConnected()
            harness.source.onInbound(peer, desktopRoles())
        }
        advanceUntilIdle()

        assertEquals(
            "no announcement storm",
            epoch,
            harness.source.status.value.peers.getValue(peer.toHex()).localEpoch,
        )
        assertEquals("no repeated snapshot and no repeated content", settled, wire.messages.size)
    }

    // ========================================================================
    // §16 F / G — epoch discipline
    // ========================================================================

    /**
     * **F — an older epoch cannot restore authority that was given up.**
     *
     * The dangerous mutation named in brief §17. A desktop that narrowed to no
     * roles at epoch 2 must not be re-widened by a replay of its epoch-1
     * `SINK`.
     */
    @Test
    fun `F a stale epoch is ignored`() = runTest {
        val harness = Harness()
        harness.allow(peer)
        val wire = Wire()
        harness.source.start(producerScope())
        harness.source.attachListener(harness.listener)
        harness.source.onListenerConnected()
        harness.source.attachSession(peer, "peer-device", wire.send)
        harness.source.onInbound(peer, desktopRoles(epoch = 1))
        harness.source.onInbound(peer, desktopRoles(epoch = 2, roles = emptyArray()))
        advanceUntilIdle()
        assertFalse(harness.source.status.value.peers.getValue(peer.toHex()).peerIsSink)

        // The replay.
        harness.source.onInbound(peer, desktopRoles(epoch = 1))
        advanceUntilIdle()

        val status = harness.source.status.value.peers.getValue(peer.toHex())
        assertFalse("a replayed announcement may not re-widen", status.peerIsSink)
        assertEquals(2, status.peerEpoch)

        // And with no SINK, nothing is sent however ready this device is.
        wire.clear()
        harness.source.onPosted(notification())
        advanceUntilIdle()
        assertTrue(wire.upserts().isEmpty())
    }

    /**
     * **G — a duplicate of the current epoch is inert.**
     *
     * *Equal* is refused as well as lower: a duplicate carrying a different
     * set must not take effect, or a replay could widen by repetition.
     */
    @Test
    fun `G a duplicate epoch cannot change the set`() = runTest {
        val harness = Harness()
        harness.allow(peer)
        val wire = Wire()
        harness.source.start(producerScope())
        harness.source.attachListener(harness.listener)
        harness.source.onListenerConnected()
        harness.source.attachSession(peer, "peer-device", wire.send)
        harness.source.onInbound(
            peer,
            desktopRoles(epoch = 1, roles = arrayOf(NotificationRole.NOTIFICATION_ROLE_SINK)),
        )
        advanceUntilIdle()
        assertFalse(harness.source.status.value.peers.getValue(peer.toHex()).peerIsDismissReporter)

        // Same epoch, an extra role.
        harness.source.onInbound(peer, desktopRoles(epoch = 1))
        advanceUntilIdle()

        val status = harness.source.status.value.peers.getValue(peer.toHex())
        assertFalse("a duplicate epoch may not add a role", status.peerIsDismissReporter)
        assertTrue(status.peerIsSink)
        assertEquals(1, status.peerEpoch)
    }

    /** Epoch 0 is the wire's "unset" and is refused outright. */
    @Test
    fun `an unset epoch is refused`() = runTest {
        val harness = Harness()
        harness.allow(peer)
        val wire = Wire()
        harness.source.start(producerScope())
        harness.source.attachListener(harness.listener)
        harness.source.onListenerConnected()
        harness.source.attachSession(peer, "peer-device", wire.send)
        harness.source.onInbound(peer, desktopRoles(epoch = 0))
        advanceUntilIdle()

        val status = harness.source.status.value.peers.getValue(peer.toHex())
        assertFalse(status.peerIsSink)
        assertEquals(0, status.peerEpoch)
    }

    // ========================================================================
    // §10 / §16 H — reconnect
    // ========================================================================

    /**
     * **H — a reconnect starts from one coherent current state.**
     *
     * Brief §10. Both role states are per connection, so a new session begins
     * at epoch 1 on both sides and needs no manual toggle to converge.
     */
    @Test
    fun `H a reconnect converges with no manual toggle`() = runTest {
        val listener = FakeListener(active = listOf(notification()))
        val harness = Harness(listener = listener)
        harness.allow(peer)
        val first = Wire()
        harness.source.start(producerScope())
        harness.source.attachListener(listener)
        harness.source.onListenerConnected()
        harness.source.attachSession(peer, "peer-device", first.send)
        harness.source.onInbound(peer, desktopRoles())
        advanceUntilIdle()
        assertEquals(1, first.markers().count { it.sync.phase == SyncMarker.Phase.PHASE_BEGIN })

        harness.source.detachSession(peer)
        advanceUntilIdle()

        val second = Wire()
        harness.source.attachSession(peer, "peer-device", second.send)
        harness.source.onInbound(peer, desktopRoles())
        advanceUntilIdle()

        val status = harness.source.status.value.peers.getValue(peer.toHex())
        assertEquals("a fresh connection has been told nothing", 1L, status.localEpoch)
        assertEquals(1, status.peerEpoch)
        assertTrue(status.localIsSource)
        assertEquals(
            listOf("roles:2@1", "sync:BEGIN", "upsert:$allowedApp", "sync:END"),
            second.bodies(),
        )
        assertEquals("and no duplicate mirror", 1, second.upserts().size)
    }

    // ========================================================================
    // §16 I / §22 — multi-peer isolation
    // ========================================================================

    /**
     * **I — a role or grant change for one peer never widens another.**
     *
     * Brief §22, on the architecture P1 left behind: the tablet may trust three
     * desktops. Peer A is allowed to see notifications and peer B is not, and
     * B's own state is unmoved by everything that happens to A.
     */
    @Test
    fun `I role and grant state is scoped to one peer`() = runTest {
        val listener = FakeListener(active = listOf(notification()))
        val harness = Harness(listener = listener)
        harness.allow(peer)
        harness.deny(otherPeer)
        val a = Wire()
        val b = Wire()
        harness.source.start(producerScope())
        harness.source.attachListener(listener)
        harness.source.onListenerConnected()
        harness.source.attachSession(peer, "a-device", a.send)
        harness.source.attachSession(otherPeer, "b-device", b.send)
        // Only A's computer announces SINK.
        harness.source.onInbound(peer, desktopRoles())
        advanceUntilIdle()

        val peers = harness.source.status.value.peers
        assertTrue(peers.getValue(peer.toHex()).peerIsSink)
        assertFalse(
            "B's computer said nothing and must not inherit A's claim",
            peers.getValue(otherPeer.toHex()).peerIsSink,
        )
        assertEquals(0, peers.getValue(otherPeer.toHex()).peerEpoch)

        // A gets the snapshot; B gets its roles and nothing else.
        assertTrue(a.upserts().isNotEmpty())
        assertEquals(listOf("roles:2@1"), b.bodies())

        // A new notification goes to A alone.
        a.clear()
        b.clear()
        harness.source.onPosted(notification(key = "0|$allowedApp|5|e|10123"))
        advanceUntilIdle()
        assertEquals(1, a.upserts().size)
        assertTrue("B is ungranted and never claimed SINK", b.messages.isEmpty())

        // B's computer announcing SINK still gets it nothing: the grant is a
        // separate question and B has none.
        harness.source.onInbound(otherPeer, desktopRoles())
        advanceUntilIdle()
        assertTrue(harness.source.status.value.peers.getValue(otherPeer.toHex()).peerIsSink)
        assertTrue("recording a role authorizes nothing", b.upserts().isEmpty())
        assertTrue(b.markers().isEmpty())
    }

    // ========================================================================
    // §14 — dismiss sync, and §15 — lock policy
    // ========================================================================

    /**
     * **K — dismiss sync still works after converging mid-session.**
     *
     * Brief §14, targeted. A clearable notification mirrored by the
     * convergence path is cancelled on this phone when the computer asks, and
     * the answer says so.
     */
    @Test
    fun `K dismiss sync is correct after a mid-session convergence`() = runTest {
        val listener = FakeListener(active = listOf(notification()))
        val harness = Harness(listener = listener, access = FakeAccess(granted = false))
        harness.deny(peer)
        val wire = Wire()
        harness.source.start(producerScope())
        harness.source.attachListener(listener)
        harness.source.attachSession(peer, "peer-device", wire.send)
        harness.source.onInbound(
            peer,
            desktopRoles(
                roles = arrayOf(
                    NotificationRole.NOTIFICATION_ROLE_SINK,
                    NotificationRole.NOTIFICATION_ROLE_DISMISS_REPORTER,
                ),
            ),
        )
        advanceUntilIdle()

        harness.allow(peer, sharing(dismissSync = true))
        harness.access.granted = true
        harness.source.policyChanged()
        advanceUntilIdle()
        harness.source.onListenerConnected()
        advanceUntilIdle()

        val mirrored = wire.upserts().single().upsert
        assertTrue(mirrored.dismissible)
        wire.clear()

        harness.source.onInbound(
            peer,
            NotificationControl.newBuilder()
                .setDismiss(
                    DismissRequest.newBuilder()
                        .setNotificationId(mirrored.notificationId)
                        .setOriginDeviceId("0123456789abcdef0123456789abcdef"),
                )
                .build(),
        )
        advanceUntilIdle()

        assertEquals(listOf("0|$allowedApp|1|null|10123"), listener.cancelled)
        assertEquals(
            NotificationOutcome.NOTIFICATION_OUTCOME_REMOVED,
            wire.messages.last().result.outcome,
        )
    }

    /**
     * **An ongoing notification is still declined, with no retry loop.**
     *
     * The negative half of brief §14: convergence must not have widened what a
     * peer may cancel.
     */
    @Test
    fun `an ongoing notification is not dismissed after convergence`() = runTest {
        val ongoing = notification(key = "0|$allowedApp|8|h|10123", ongoing = true)
        val listener = FakeListener(active = listOf(ongoing))
        val harness = Harness(listener = listener)
        // An ongoing notification is off by default and has to be asked for,
        // which is itself part of the deny-by-default shape this must not
        // widen. It is turned on here so there is something non-dismissible on
        // the wire to refuse a dismissal for.
        harness.allow(
            peer,
            NotificationPolicy(
                allowMirror = true,
                allowedApps = setOf("example.fixture.app"),
                includeOngoing = true,
                allowDismissSync = true,
            ),
        )
        val wire = Wire()
        harness.source.start(producerScope())
        harness.source.attachListener(listener)
        harness.source.onListenerConnected()
        harness.source.attachSession(peer, "peer-device", wire.send)
        harness.source.onInbound(peer, desktopRoles())
        advanceUntilIdle()

        val mirrored = wire.upserts().single().upsert
        assertFalse(mirrored.dismissible)
        wire.clear()

        repeat(3) {
            harness.source.onInbound(
                peer,
                NotificationControl.newBuilder()
                    .setDismiss(
                        DismissRequest.newBuilder()
                            .setNotificationId(mirrored.notificationId)
                            .setOriginDeviceId("0123456789abcdef0123456789abcdef"),
                    )
                    .build(),
            )
        }
        advanceUntilIdle()

        assertTrue("the original stays", listener.cancelled.isEmpty())
        assertNotEquals(
            NotificationOutcome.NOTIFICATION_OUTCOME_REMOVED,
            wire.messages.last().result.outcome,
        )
        assertEquals("one answer each, no retry loop", 3, wire.messages.size)
    }

    /**
     * **L — the lock policy still governs what a converged session sends.**
     *
     * Brief §15, targeted. Converging roles must not become a way past the
     * lock-state reduction: the snapshot a newly-enabled peer receives obeys
     * the same policy a live notification would.
     */
    @Test
    fun `L the lock policy still applies to a mid-session snapshot`() = runTest {
        val listener = FakeListener(active = listOf(notification()))
        val harness = Harness(listener = listener, access = FakeAccess(granted = false))
        harness.locked = true
        harness.deny(peer)
        val wire = Wire()
        harness.source.start(producerScope())
        harness.source.attachListener(listener)
        harness.source.attachSession(peer, "peer-device", wire.send)
        harness.source.onInbound(peer, desktopRoles())
        advanceUntilIdle()

        harness.allow(
            peer,
            NotificationPolicy(
                allowMirror = true,
                allowedApps = setOf("example.fixture.app"),
                whenSourceLocked = LockPolicy.APP_ONLY,
            ),
        )
        harness.access.granted = true
        harness.source.policyChanged()
        advanceUntilIdle()
        harness.source.onListenerConnected()
        advanceUntilIdle()

        val upsert = wire.upserts().single().upsert
        assertTrue("the reduction was applied", upsert.redacted)
        assertEquals("", upsert.title)
        assertEquals("", upsert.body)

        // Unlocking replays no withheld history: what was reduced stays
        // reduced until the notification itself changes.
        wire.clear()
        harness.locked = false
        harness.source.policyChanged()
        advanceUntilIdle()
        assertTrue("no withheld history is replayed", wire.messages.isEmpty())
    }

    // ========================================================================
    // §23 — privacy
    // ========================================================================

    /**
     * **A role announcement carries counts and an epoch, and nothing else.**
     *
     * Brief §23. The convergence traffic this branch adds must not be able to
     * carry content, and the canaries prove it by absence on the one message
     * type P3 made more frequent.
     */
    @Test
    fun `role announcements carry no notification content`() = runTest {
        val listener = FakeListener(active = listOf(notification()))
        val harness = Harness(listener = listener, access = FakeAccess(granted = false))
        harness.deny(peer)
        val wire = Wire()
        harness.source.start(producerScope())
        harness.source.attachListener(listener)
        harness.source.attachSession(peer, "peer-device", wire.send)
        harness.source.onInbound(peer, desktopRoles())
        advanceUntilIdle()

        harness.allow(peer)
        harness.access.granted = true
        harness.source.policyChanged()
        advanceUntilIdle()

        val rendered = wire.roles().joinToString("\n") { it.toString() }
        assertFalse(rendered.contains("FIXTURE-TITLE-CANARY"))
        assertFalse(rendered.contains("FIXTURE-BODY-CANARY"))
        assertFalse(rendered.contains(allowedApp))
        assertFalse("no platform key", rendered.contains("10123"))
    }
}
