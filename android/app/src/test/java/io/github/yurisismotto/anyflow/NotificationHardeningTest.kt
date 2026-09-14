package io.github.yurisismotto.anyflow

import com.google.protobuf.ByteString
import io.github.yurisismotto.anyflow.identity.Fingerprint
import io.github.yurisismotto.anyflow.notifications.LockState
import io.github.yurisismotto.anyflow.notifications.NotificationLimits
import io.github.yurisismotto.anyflow.notifications.NotificationMapping
import io.github.yurisismotto.anyflow.notifications.NotificationOutboundQueue
import io.github.yurisismotto.anyflow.notifications.NotificationPolicy
import io.github.yurisismotto.anyflow.notifications.NotificationSecret
import io.github.yurisismotto.anyflow.notifications.NotificationSource
import io.github.yurisismotto.anyflow.notifications.PlatformNotification
import io.github.yurisismotto.anyflow.proto.capabilities.DismissRequest
import io.github.yurisismotto.anyflow.proto.capabilities.NotificationControl
import io.github.yurisismotto.anyflow.proto.capabilities.NotificationOutcome
import io.github.yurisismotto.anyflow.proto.capabilities.NotificationRole
import io.github.yurisismotto.anyflow.proto.capabilities.NotificationRoles
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
 * N5 — what the Android source does when the platform misbehaves.
 *
 * `NotificationSourceTest` proves the rules with a platform that answers.
 * This file removes that assumption one call at a time: the listener returns
 * `null`, the listener *throws*, the keystore goes away mid-session, the
 * shade is empty after a restart. Every one of them must fail closed, leave
 * no content anywhere, never widen what a peer may do, and never take the
 * single ordered producer down — because the producer dying would silently
 * stop `battery.v1`-adjacent nothing and `notifications.v1` everything, with
 * no error a user could see.
 *
 * It also proves the per-peer isolation that a shared-state bug would break:
 * two computers, one phone, and nothing that belongs to one reaching the
 * other.
 *
 * # Why a throwing listener is worth its own fake
 *
 * `NotificationListenerService`'s methods are documented to return null when
 * the service is not connected, and in practice they throw
 * `SecurityException` when the binding has been torn down underneath the
 * caller — which is exactly what happens the instant a person revokes
 * notification access in Settings. A fake that only ever returned null would
 * test the tidy half of a race the product actually loses.
 */
@OptIn(ExperimentalCoroutinesApi::class)
class NotificationHardeningTest {

    private val ownPackage = "io.github.yurisismotto.anyflow"
    private val localDeviceId = "0123456789abcdef0123456789abcdef"
    private val allowedApp = "example.fixture.app"
    private val otherApp = "example.other.app"

    private val peerA = Fingerprint(ByteArray(32) { 0x11 })
    private val peerB = Fingerprint(ByteArray(32) { 0x22 })

    private val canaryTitle = "N5-HARDENING-TITLE-CANARY"
    private val canaryBody = "N5-HARDENING-BODY-CANARY"

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

        /** The keystore refuses everything, as a locked or wiped TEE would. */
        var unavailable = false

        override fun load(): SecretKey? = if (unavailable) null else stored
        override fun create(): SecretKey {
            if (unavailable) throw IllegalStateException("keystore unavailable")
            return SecretKeySpec(ByteArray(32) { (it + 100).toByte() }, "HmacSHA256")
                .also { stored = it }
        }
        override fun destroy() { stored = null }
    }

    private class FakeMetadata(override var notificationSecretGeneration: Int = 1) :
        NotificationSecret.Metadata

    /**
     * A platform seam that can be broken in each direction independently.
     *
     * Each switch is a separate failure because they are separate failures on
     * a real device: a listener that has lost its binding throws from every
     * method, while one that is merely starting up returns null from the bulk
     * query and answers the keyed one.
     */
    private class BreakableListener(
        var active: List<PlatformNotification>? = emptyList(),
    ) : NotificationSource.ListenerControl {
        var unbinds = 0
        val cancelled = mutableListOf<String>()

        var activeNotificationsThrows = false
        var activeNotificationThrows = false
        var activePackagesThrows = false
        var cancelThrows = false
        var cancelSucceeds = true

        override fun requestUnbind() { unbinds += 1 }

        override fun activeNotifications(): List<PlatformNotification>? {
            if (activeNotificationsThrows) throw SecurityException("listener not connected")
            return active
        }

        override fun activeNotification(platformKey: String): PlatformNotification? {
            if (activeNotificationThrows) throw SecurityException("listener not connected")
            return active?.firstOrNull { it.platformKey == platformKey }
        }

        override fun cancel(platformKey: String): Boolean {
            cancelled += platformKey
            if (cancelThrows) throw SecurityException("listener not connected")
            if (!cancelSucceeds) return false
            active = active?.filterNot { it.platformKey == platformKey }
            return true
        }

        override fun activePackages(): List<String>? {
            if (activePackagesThrows) throw SecurityException("listener not connected")
            return active?.map { it.packageName }?.distinct()
        }
    }

    private class FakeAccess(var granted: Boolean = true) : NotificationSource.AccessControl {
        var binds = 0
        override fun isAccessGranted(): Boolean = granted
        override fun requestBind() { binds += 1 }
    }

    private class Wire {
        val messages = mutableListOf<NotificationControl>()
        val send: suspend (ByteString) -> Unit = { payload ->
            messages += NotificationControl.parseFrom(payload)
        }

        fun upserts() = messages.filter { it.bodyCase == NotificationControl.BodyCase.UPSERT }
        fun removes() = messages.filter { it.bodyCase == NotificationControl.BodyCase.REMOVE }
        fun roles() = messages.filter { it.bodyCase == NotificationControl.BodyCase.ROLES }
        fun results() = messages.filter { it.bodyCase == NotificationControl.BodyCase.RESULT }
        fun syncs() = messages.filter { it.bodyCase == NotificationControl.BodyCase.SYNC }
        fun rendered(): String = messages.joinToString("\n") { it.toString() }
    }

    private fun notification(
        key: String = "0|$allowedApp|1|null|10123",
        packageName: String = allowedApp,
        title: String = "N5-HARDENING-TITLE-CANARY",
        body: String = "N5-HARDENING-BODY-CANARY",
        ongoing: Boolean = false,
    ) = PlatformNotification(
        platformKey = key,
        packageName = packageName,
        secondaryProfile = false,
        postedAtUnixMs = 1_700_000_000_000L,
        ongoing = ongoing,
        clearable = !ongoing,
        visibility = NotificationMapping.VISIBILITY_PRIVATE,
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

    private fun sinkRoles(epoch: Int = 1, dismissReporter: Boolean = false): NotificationControl =
        NotificationControl.newBuilder()
            .setRoles(
                NotificationRoles.newBuilder()
                    .addRoles(NotificationRole.NOTIFICATION_ROLE_SINK)
                    .also {
                        if (dismissReporter) {
                            it.addRoles(NotificationRole.NOTIFICATION_ROLE_DISMISS_REPORTER)
                        }
                    }
                    .setEpoch(epoch),
            )
            .build()

    private fun dismiss(id: ByteString, origin: String = "0123456789abcdef0123456789abcdef") =
        NotificationControl.newBuilder()
            .setDismiss(
                DismissRequest.newBuilder()
                    .setNotificationId(id)
                    .setOriginDeviceId(origin),
            )
            .build()

    private class Harness(
        val listener: BreakableListener = BreakableListener(),
        val access: FakeAccess = FakeAccess(),
        var policy: NotificationPolicy = NotificationPolicy(
            allowedApps = setOf("example.fixture.app"),
        ),
        var locked: Boolean = false,
        val secretStore: FakeSecretStore = FakeSecretStore(),
        queueCapacity: Int = NotificationLimits.EVENT_QUEUE_CAPACITY,
    ) {
        val metadata = FakeMetadata()
        var policyOverride: ((Fingerprint) -> NotificationPolicy)? = null

        val source = NotificationSource(
            ownPackage = "io.github.yurisismotto.anyflow",
            localDeviceId = "0123456789abcdef0123456789abcdef",
            authorizer = { fingerprint -> policyOverride?.invoke(fingerprint) ?: policy },
            appLabels = { packageName -> "Label for $packageName" },
            secretProvider = { NotificationSecret.loadOrCreate(secretStore, metadata) },
            lockState = LockState { locked },
            access = access,
            queue = NotificationOutboundQueue(queueCapacity),
        )
    }

    /** A connected, granted, sourcing phone with one sink peer attached. */
    private fun TestScope.sourcing(
        harness: Harness,
        wire: Wire,
        peer: Fingerprint = peerA,
        dismissReporter: Boolean = false,
    ) {
        harness.source.start(producerScope())
        harness.source.attachListener(harness.listener)
        harness.source.onListenerConnected()
        harness.source.attachSession(peer, "peer-device", wire.send)
        harness.source.onInbound(peer, sinkRoles(dismissReporter = dismissReporter))
        advanceUntilIdle()
    }

    // -----------------------------------------------------------------------
    // §17 — the listener fails
    // -----------------------------------------------------------------------

    /**
     * A bulk query that returns null sends no snapshot, and does not send an
     * empty one.
     *
     * The difference matters more than it looks: an *empty* snapshot is a
     * statement that nothing is active, and the sink honours it by removing
     * every mirror. "I could not ask" must never be encoded as "there is
     * nothing".
     */
    @Test
    fun `a listener that cannot list the shade sends no snapshot at all`() = runTest {
        val harness = Harness(listener = BreakableListener(active = null))
        val wire = Wire()
        sourcing(harness, wire)

        assertTrue(
            "an unreadable shade must not be reported as an empty one: ${wire.syncs()}",
            wire.syncs().isEmpty(),
        )
        assertTrue(wire.upserts().isEmpty())
        // And the session is still usable: a live notification still flows.
        harness.source.onPosted(notification())
        advanceUntilIdle()
        assertEquals(1, wire.upserts().size)
    }

    /**
     * A bulk query that **throws** is the same answer, and does not kill the
     * single ordered producer.
     *
     * If the producer died here, every later notification, removal and role
     * narrowing would be silently dropped for the life of the process — the
     * worst available failure, because nothing on either screen would say so.
     */
    @Test
    fun `a listener that throws while listing the shade leaves the producer alive`() = runTest {
        val harness = Harness()
        harness.listener.activeNotificationsThrows = true
        val wire = Wire()
        sourcing(harness, wire)

        assertTrue("no snapshot may be produced from a throw", wire.syncs().isEmpty())
        assertEquals("the role announcement still happened", 1, wire.roles().size)

        // The producer survived: the next event is handled normally.
        harness.listener.activeNotificationsThrows = false
        harness.source.onPosted(notification())
        advanceUntilIdle()
        assertEquals(1, wire.upserts().size)
    }

    /**
     * The keyed lookup throwing during a dismissal fails closed.
     *
     * Clearability is re-checked against the live platform on every
     * `DismissRequest`. A lookup that throws is "I cannot tell", and the only
     * safe reading of that is "do not cancel".
     */
    @Test
    fun `a keyed lookup that throws refuses the dismissal and cancels nothing`() = runTest {
        val harness = Harness(
            policy = NotificationPolicy(
                allowedApps = setOf("example.fixture.app"),
                allowDismissSync = true,
            ),
        )
        harness.listener.active = listOf(notification())
        val wire = Wire()
        sourcing(harness, wire, dismissReporter = true)

        harness.source.onPosted(notification())
        advanceUntilIdle()
        val id = wire.upserts().single().upsert.notificationId

        harness.listener.activeNotificationThrows = true
        harness.source.onInbound(peerA, dismiss(id))
        advanceUntilIdle()

        assertTrue(
            "nothing may be cancelled on a lookup this device could not perform",
            harness.listener.cancelled.isEmpty(),
        )
        val outcome = wire.results().last().result.outcome
        assertNotEquals(NotificationOutcome.NOTIFICATION_OUTCOME_REMOVED, outcome)
        assertNotEquals(NotificationOutcome.NOTIFICATION_OUTCOME_UNSPECIFIED, outcome)
    }

    /**
     * `cancel` throwing is answered, once, and does not retry.
     *
     * A retry loop here would be a loop against a platform that has already
     * said no, on a path a remote peer can trigger.
     */
    @Test
    fun `a cancel that throws is answered once and never retried`() = runTest {
        val harness = Harness(
            policy = NotificationPolicy(
                allowedApps = setOf("example.fixture.app"),
                allowDismissSync = true,
            ),
        )
        harness.listener.active = listOf(notification())
        val wire = Wire()
        sourcing(harness, wire, dismissReporter = true)

        harness.source.onPosted(notification())
        advanceUntilIdle()
        val id = wire.upserts().single().upsert.notificationId

        harness.listener.cancelThrows = true
        harness.source.onInbound(peerA, dismiss(id))
        advanceUntilIdle()

        assertEquals("exactly one attempt", 1, harness.listener.cancelled.size)
        val outcome = wire.results().last().result.outcome
        assertNotEquals(NotificationOutcome.NOTIFICATION_OUTCOME_REMOVED, outcome)

        // A second request is handled the same way rather than compounding.
        harness.source.onInbound(peerA, dismiss(id))
        advanceUntilIdle()
        assertTrue(harness.listener.cancelled.size <= 2)
        assertEquals("the producer is still ordered and alive", 1, wire.roles().size)
    }

    /** The picker's query throwing yields no packages rather than a crash. */
    @Test
    fun `a package query that throws yields nothing and does not crash`() = runTest {
        val harness = Harness()
        harness.listener.active = listOf(notification())
        val wire = Wire()
        sourcing(harness, wire)

        harness.listener.activePackagesThrows = true
        assertEquals(emptySet<String>(), harness.source.activePackages())

        harness.listener.activePackagesThrows = false
        assertEquals(setOf(allowedApp), harness.source.activePackages())
    }

    /**
     * A keystore that goes away mid-session narrows the source role rather
     * than mirroring with a key it cannot derive.
     */
    @Test
    fun `a keystore lost mid-session narrows the role and stops emission`() = runTest {
        val harness = Harness()
        val wire = Wire()
        sourcing(harness, wire)
        harness.source.onPosted(notification())
        advanceUntilIdle()
        assertEquals(1, wire.upserts().size)

        harness.secretStore.unavailable = true
        harness.secretStore.stored = null
        harness.source.policyChanged()
        harness.source.onPosted(notification(key = "0|$allowedApp|2|null|10123"))
        advanceUntilIdle()

        assertEquals("nothing more may be mirrored", 1, wire.upserts().size)
        val last = wire.roles().last().roles
        assertFalse(
            "a phone that cannot derive an identity is not a source: $last",
            last.rolesList.contains(NotificationRole.NOTIFICATION_ROLE_SOURCE),
        )
    }

    // -----------------------------------------------------------------------
    // §12 — two computers, one phone
    // -----------------------------------------------------------------------

    /**
     * One computer's allow-list never widens another's.
     *
     * The policy is asked per peer on every notification, so a shared cache
     * or a captured answer would show up here and nowhere else.
     */
    @Test
    fun `an app allowed for one computer is not mirrored to another`() = runTest {
        val harness = Harness()
        val wireA = Wire()
        val wireB = Wire()
        harness.policyOverride = { fingerprint ->
            if (fingerprint == peerA) {
                NotificationPolicy(allowedApps = setOf(allowedApp))
            } else {
                NotificationPolicy(allowedApps = setOf(otherApp))
            }
        }
        harness.source.start(producerScope())
        harness.source.attachListener(harness.listener)
        harness.source.onListenerConnected()
        harness.source.attachSession(peerA, "computer-a", wireA.send)
        harness.source.attachSession(peerB, "computer-b", wireB.send)
        harness.source.onInbound(peerA, sinkRoles())
        harness.source.onInbound(peerB, sinkRoles())
        advanceUntilIdle()

        harness.source.onPosted(notification(packageName = allowedApp))
        advanceUntilIdle()

        assertEquals("A named this app", 1, wireA.upserts().size)
        assertEquals("B did not name it", 0, wireB.upserts().size)
        assertFalse(
            "not a byte of the other computer's traffic may carry it",
            wireB.rendered().contains(canaryTitle) || wireB.rendered().contains(canaryBody),
        )
    }

    /** Revoking one computer's grant leaves the other mirroring. */
    @Test
    fun `revoking one computer does not stop the other`() = runTest {
        val harness = Harness()
        val wireA = Wire()
        val wireB = Wire()
        var aGranted = true
        harness.policyOverride = { fingerprint ->
            if (fingerprint == peerA && !aGranted) {
                NotificationPolicy.DENIED
            } else {
                NotificationPolicy(allowedApps = setOf(allowedApp))
            }
        }
        harness.source.start(producerScope())
        harness.source.attachListener(harness.listener)
        harness.source.onListenerConnected()
        harness.source.attachSession(peerA, "computer-a", wireA.send)
        harness.source.attachSession(peerB, "computer-b", wireB.send)
        harness.source.onInbound(peerA, sinkRoles())
        harness.source.onInbound(peerB, sinkRoles())
        advanceUntilIdle()

        harness.source.onPosted(notification())
        advanceUntilIdle()
        assertEquals(1, wireA.upserts().size)
        assertEquals(1, wireB.upserts().size)

        aGranted = false
        harness.source.policyChanged()
        harness.source.onPosted(notification(key = "0|$allowedApp|2|null|10123"))
        advanceUntilIdle()

        assertEquals("A is revoked", 1, wireA.upserts().size)
        assertEquals("B is untouched", 2, wireB.upserts().size)
    }

    /**
     * A computer that never announced `SINK` is sent nothing, while the one
     * that did keeps receiving.
     */
    @Test
    fun `a peer that announced no role does not silence the one that did`() = runTest {
        val harness = Harness()
        val wireA = Wire()
        val wireB = Wire()
        harness.source.start(producerScope())
        harness.source.attachListener(harness.listener)
        harness.source.onListenerConnected()
        harness.source.attachSession(peerA, "computer-a", wireA.send)
        harness.source.attachSession(peerB, "computer-b", wireB.send)
        harness.source.onInbound(peerA, sinkRoles())
        advanceUntilIdle()

        harness.source.onPosted(notification())
        advanceUntilIdle()

        assertEquals(1, wireA.upserts().size)
        assertEquals(0, wireB.upserts().size)
        assertTrue(
            "B was still told this device's roles, which is not content",
            wireB.roles().isNotEmpty(),
        )
    }

    /**
     * A computer's session ending does not disturb the other's.
     *
     * The phone keeps one map of active identities, not one per peer, so a
     * detach that cleared it would take the other computer's state with it.
     */
    @Test
    fun `one computer disconnecting leaves the other's stream intact`() = runTest {
        val harness = Harness()
        val wireA = Wire()
        val wireB = Wire()
        harness.source.start(producerScope())
        harness.source.attachListener(harness.listener)
        harness.source.onListenerConnected()
        harness.source.attachSession(peerA, "computer-a", wireA.send)
        harness.source.attachSession(peerB, "computer-b", wireB.send)
        harness.source.onInbound(peerA, sinkRoles())
        harness.source.onInbound(peerB, sinkRoles())
        advanceUntilIdle()

        harness.source.onPosted(notification())
        advanceUntilIdle()
        harness.source.detachSession(peerA)
        advanceUntilIdle()

        harness.source.onRemoved("0|$allowedApp|1|null|10123")
        advanceUntilIdle()

        assertEquals("B is told the notification is gone", 1, wireB.removes().size)
        assertEquals("A left before it happened", 0, wireA.removes().size)
    }

    // -----------------------------------------------------------------------
    // §16 — process recovery
    // -----------------------------------------------------------------------

    /**
     * After a process restart with an empty shade, a dismissal for an
     * identity from the previous life cancels nothing.
     *
     * The id map is **rebuilt from the live shade**, never restored from
     * disk. So an identity the desktop still remembers, from before the
     * phone's process died, maps to nothing — and mapping it to *something*
     * would be the one way this design could cancel a notification nobody
     * asked about.
     */
    @Test
    fun `an identity from a previous process cancels nothing after a restart`() = runTest {
        val first = Harness(
            policy = NotificationPolicy(
                allowedApps = setOf("example.fixture.app"),
                allowDismissSync = true,
            ),
        )
        first.listener.active = listOf(notification())
        val wireOne = Wire()
        sourcing(first, wireOne, dismissReporter = true)
        first.source.onPosted(notification())
        advanceUntilIdle()
        val id = wireOne.upserts().single().upsert.notificationId

        // A new process: a new source over the same secret store, and a shade
        // that no longer holds it.
        val second = Harness(
            listener = BreakableListener(active = emptyList()),
            policy = NotificationPolicy(
                allowedApps = setOf("example.fixture.app"),
                allowDismissSync = true,
            ),
        )
        val wireTwo = Wire()
        sourcing(second, wireTwo, dismissReporter = true)

        second.source.onInbound(peerA, dismiss(id))
        advanceUntilIdle()

        assertTrue(
            "an unmapped identity must cancel nothing at all",
            second.listener.cancelled.isEmpty(),
        )
        assertEquals(
            NotificationOutcome.NOTIFICATION_OUTCOME_UNKNOWN_NOTIFICATION,
            wireTwo.results().last().result.outcome,
        )
    }

    /**
     * The same shade, after a restart, derives the same identities — so the
     * desktop's mirrors are replaced rather than duplicated.
     *
     * The complement of the test above, and the reason it is safe for the map
     * to be rebuilt rather than persisted.
     */
    @Test
    fun `the same shade derives the same identities across a restart`() = runTest {
        val store = FakeSecretStore()
        val live = listOf(notification())

        val first = Harness(listener = BreakableListener(active = live), secretStore = store)
        val wireOne = Wire()
        sourcing(first, wireOne)
        val firstIds = wireOne.upserts().map { it.upsert.notificationId }

        val second = Harness(listener = BreakableListener(active = live), secretStore = store)
        val wireTwo = Wire()
        sourcing(second, wireTwo)
        val secondIds = wireTwo.upserts().map { it.upsert.notificationId }

        assertTrue("the snapshot must not be empty, or this proves nothing", firstIds.isNotEmpty())
        assertEquals(firstIds, secondIds)
    }

    // -----------------------------------------------------------------------
    // The property every failure above must not break
    // -----------------------------------------------------------------------

    /**
     * No failure path renders a title, a body or a raw platform key.
     *
     * Driven rather than argued: every seam is broken in turn over one
     * session, and the whole of what was sent, plus the status the UI reads,
     * is searched for the canaries afterwards.
     */
    @Test
    fun `no failure path renders notification content or a platform key`() = runTest {
        val platformKey = "0|$allowedApp|1|null|10123"
        val harness = Harness(
            policy = NotificationPolicy(
                allowedApps = setOf("example.fixture.app"),
                allowDismissSync = true,
            ),
        )
        harness.listener.active = listOf(notification(key = platformKey))
        val wire = Wire()
        sourcing(harness, wire, dismissReporter = true)

        harness.source.onPosted(notification(key = platformKey))
        advanceUntilIdle()
        val id = wire.upserts().single().upsert.notificationId

        harness.listener.activeNotificationThrows = true
        harness.source.onInbound(peerA, dismiss(id))
        advanceUntilIdle()
        harness.listener.activeNotificationThrows = false

        harness.listener.cancelThrows = true
        harness.source.onInbound(peerA, dismiss(id))
        advanceUntilIdle()
        harness.listener.cancelThrows = false

        harness.listener.activeNotificationsThrows = true
        harness.source.policyChanged()
        advanceUntilIdle()

        val rendered = buildString {
            append(harness.source.status.value.toString())
            append('\n')
            append(harness.source.describeQueue())
            append('\n')
            // Everything sent, minus the upsert that legitimately carries the
            // content: what is under test is whether a *failure* leaked it.
            append(
                wire.messages
                    .filterNot { it.bodyCase == NotificationControl.BodyCase.UPSERT }
                    .joinToString("\n"),
            )
        }
        assertTrue("the capture is empty, so it proves nothing", rendered.isNotEmpty())
        for (canary in listOf(canaryTitle, canaryBody, platformKey, allowedApp)) {
            assertFalse(
                "a failure path rendered '$canary' in:\n$rendered",
                rendered.contains(canary),
            )
        }
    }
}
