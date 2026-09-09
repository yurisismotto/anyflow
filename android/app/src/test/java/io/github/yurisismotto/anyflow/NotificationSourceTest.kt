package io.github.yurisismotto.anyflow

import com.google.protobuf.ByteString
import io.github.yurisismotto.anyflow.identity.Fingerprint
import io.github.yurisismotto.anyflow.notifications.LockPolicy
import io.github.yurisismotto.anyflow.notifications.LockState
import io.github.yurisismotto.anyflow.notifications.NotificationLimits
import io.github.yurisismotto.anyflow.notifications.NotificationMapping
import io.github.yurisismotto.anyflow.notifications.NotificationPolicy
import io.github.yurisismotto.anyflow.notifications.NotificationOutboundQueue
import io.github.yurisismotto.anyflow.notifications.NotificationSecret
import io.github.yurisismotto.anyflow.notifications.NotificationSource
import io.github.yurisismotto.anyflow.notifications.PlatformNotification
import io.github.yurisismotto.anyflow.proto.capabilities.NotificationControl
import io.github.yurisismotto.anyflow.proto.capabilities.NotificationOutcome
import io.github.yurisismotto.anyflow.proto.capabilities.NotificationRole
import io.github.yurisismotto.anyflow.proto.capabilities.NotificationRoles
import io.github.yurisismotto.anyflow.proto.capabilities.SyncMarker
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
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The Android source adapter, end to end, on the JVM.
 *
 * Everything the platform provides is a seam here — the listener, the trust
 * store, the keystore, the keyguard — so what is exercised is the part N1
 * actually owns: three independent permissions, deny-by-default filtering,
 * identity derivation, ordering, the snapshot bracket, and role narrowing.
 *
 * Every fixture is obviously synthetic. On the certification hardware the
 * platform's own OTP redaction did not fire at all (POC-NOTIF-01), so
 * notification text is treated as fully sensitive user data everywhere,
 * including in tests: nothing here reads like a real notification and the
 * canaries below assert that none of it is rendered anywhere.
 */
@OptIn(ExperimentalCoroutinesApi::class)
class NotificationSourceTest {

    private val ownPackage = "io.github.yurisismotto.anyflow"
    private val localDeviceId = "0123456789abcdef0123456789abcdef"
    private val allowedApp = "example.fixture.app"
    private val deniedApp = "example.denied.app"

    private val peer = Fingerprint(ByteArray(32) { 0x11 })
    private val otherPeer = Fingerprint(ByteArray(32) { 0x22 })

    /** Canary strings: if any of these is ever rendered, something leaked. */
    private val fixtureTitle = "FIXTURE-TITLE-CANARY"
    private val fixtureBody = "FIXTURE-BODY-CANARY"

    private val producers = mutableListOf<CoroutineScope>()

    /**
     * A scope for the one ordered producer.
     *
     * Unconfined against the test scheduler, so an event offered from a test
     * body is drained before the next line runs and the assertions read like
     * the sequence they are describing. The scope is not the test's own: the
     * producer is an endless loop, and a `runTest` scope would wait for it.
     */
    private fun TestScope.producerScope(): CoroutineScope =
        CoroutineScope(UnconfinedTestDispatcher(testScheduler)).also { producers += it }

    @After
    fun stopProducers() {
        producers.forEach { it.cancel() }
        producers.clear()
    }

    // -- fakes ---------------------------------------------------------------

    private open class FakeSecretStore : NotificationSecret.Store {
        var stored: SecretKey? = SecretKeySpec(ByteArray(32) { it.toByte() }, "HmacSHA256")
        override fun load(): SecretKey? = stored
        override fun create(): SecretKey =
            SecretKeySpec(ByteArray(32) { (it + 100).toByte() }, "HmacSHA256").also { stored = it }
        override fun destroy() { stored = null }
    }

    private class FakeMetadata(override var notificationSecretGeneration: Int = 1) :
        NotificationSecret.Metadata

    /**
     * The service half of the platform seam: what only a bound service can do.
     */
    private class FakeListener(
        var active: List<PlatformNotification>? = emptyList(),
    ) : NotificationSource.ListenerControl {
        var unbinds = 0
        override fun requestUnbind() { unbinds += 1 }
        override fun activeNotifications(): List<PlatformNotification>? = active
    }

    /**
     * The context-free half: the OS grant, and the request to bind.
     *
     * Separate because `requestRebind` is static on the platform, and has to
     * be — the first bind is requested when no service instance exists.
     */
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
                NotificationControl.BodyCase.ROLES -> "roles:${it.roles.rolesCount}@${it.roles.epoch}"
                NotificationControl.BodyCase.UPSERT -> "upsert:${it.upsert.appId}"
                NotificationControl.BodyCase.REMOVE -> "remove"
                NotificationControl.BodyCase.RESULT -> "result:${it.result.outcome.name}"
                else -> it.bodyCase.name
            }
        }

        fun upserts() = messages.filter { it.bodyCase == NotificationControl.BodyCase.UPSERT }
        fun removes() = messages.filter { it.bodyCase == NotificationControl.BodyCase.REMOVE }
        fun roles() = messages.filter { it.bodyCase == NotificationControl.BodyCase.ROLES }
    }

    private fun notification(
        key: String = "0|$allowedApp|1|null|10123",
        packageName: String = allowedApp,
        title: String = "FIXTURE-TITLE-CANARY",
        body: String = "FIXTURE-BODY-CANARY",
        ongoing: Boolean = false,
        secondaryProfile: Boolean = false,
        visibility: Int = NotificationMapping.VISIBILITY_PRIVATE,
        postedAt: Long = 1_700_000_000_000L,
    ) = PlatformNotification(
        platformKey = key,
        packageName = packageName,
        secondaryProfile = secondaryProfile,
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

    private fun sinkRoles(epoch: Int = 1): NotificationControl =
        NotificationControl.newBuilder()
            .setRoles(
                NotificationRoles.newBuilder()
                    .addRoles(NotificationRole.NOTIFICATION_ROLE_SINK)
                    .setEpoch(epoch),
            )
            .build()

    /** One assembled source, with everything switched on unless overridden. */
    private class Harness(
        val listener: FakeListener = FakeListener(),
        val access: FakeAccess = FakeAccess(),
        var policy: NotificationPolicy = NotificationPolicy(allowedApps = setOf("example.fixture.app")),
        var locked: Boolean = false,
        var secretStore: FakeSecretStore = FakeSecretStore(),
        queueCapacity: Int = NotificationLimits.EVENT_QUEUE_CAPACITY,
    ) {
        val metadata = FakeMetadata()
        var policyOverride: ((Fingerprint) -> NotificationPolicy)? = null

        val source = NotificationSource(
            ownPackage = "io.github.yurisismotto.anyflow",
            localDeviceId = "0123456789abcdef0123456789abcdef",
            authorizer = { fingerprint ->
                policyOverride?.invoke(fingerprint) ?: policy
            },
            appLabels = { packageName -> "Label for $packageName" },
            secretProvider = { NotificationSecret.loadOrCreate(secretStore, metadata) },
            lockState = LockState { locked },
            access = access,
            queue = NotificationOutboundQueue(queueCapacity),
        )
    }

    // -- the happy path ------------------------------------------------------

    @Test
    fun `an allowed notification reaches a granted sink peer`() = runTest {
        val harness = Harness()
        val wire = Wire()
        harness.source.start(producerScope())
        harness.source.attachListener(harness.listener)
        harness.source.onListenerConnected()
        harness.source.attachSession(peer, "peer-device", wire.send)
        harness.source.onInbound(peer, sinkRoles())
        advanceUntilIdle()

        harness.source.onPosted(notification())
        advanceUntilIdle()

        val upsert = wire.upserts().single().upsert
        assertEquals(allowedApp, upsert.appId)
        assertEquals("Label for $allowedApp", upsert.appLabel)
        assertEquals(fixtureTitle, upsert.title)
        assertEquals(fixtureBody, upsert.body)
        assertEquals(16, upsert.notificationId.size())
        assertEquals(32, upsert.contentHash.size())
        assertEquals(8, upsert.groupId.size())
        assertEquals(localDeviceId, upsert.originDeviceId)
        assertFalse(upsert.redacted)
        assertTrue(upsert.dismissible)
    }

    /** The role announcement comes first, before any content. */
    @Test
    fun `roles are announced before anything else`() = runTest {
        val harness = Harness()
        val wire = Wire()
        harness.source.start(producerScope())
        harness.source.attachListener(harness.listener)
        harness.source.onListenerConnected()
        harness.source.attachSession(peer, "peer-device", wire.send)
        advanceUntilIdle()

        assertEquals("roles:1@1", wire.bodies().first())
        assertEquals(
            listOf(NotificationRole.NOTIFICATION_ROLE_SOURCE),
            wire.roles().first().roles.rolesList,
        )
    }

    // -- the three independent permissions -----------------------------------

    /**
     * **Holding Android notification access alone sends nothing to anyone.**
     * The listener is connected and access is granted; the peer simply has no
     * `notifications.v1` grant, and nothing crosses the wire.
     */
    @Test
    fun `an ungranted peer is sent no notification content`() = runTest {
        val harness = Harness(policy = NotificationPolicy.DENIED)
        val wire = Wire()
        harness.source.start(producerScope())
        harness.source.attachListener(harness.listener)
        harness.source.onListenerConnected()
        harness.source.attachSession(peer, "peer-device", wire.send)
        harness.source.onInbound(peer, sinkRoles())
        advanceUntilIdle()

        harness.source.onPosted(notification())
        advanceUntilIdle()

        assertTrue(wire.upserts().isEmpty())
        // It is still told what this device can do — a role is not a grant.
        assertEquals(1, wire.roles().size)
    }

    /** No notification access means no `SOURCE` role and no content. */
    @Test
    fun `without notification access nothing is sourced and no source role is claimed`() =
        runTest {
            val harness = Harness(access = FakeAccess(granted = false))
            val wire = Wire()
            harness.source.start(producerScope())
            harness.source.attachListener(harness.listener)
            harness.source.attachSession(peer, "peer-device", wire.send)
            harness.source.onInbound(peer, sinkRoles())
            advanceUntilIdle()

            harness.source.onPosted(notification())
            advanceUntilIdle()

            assertEquals(0, wire.roles().first().roles.rolesCount)
            assertTrue(wire.upserts().isEmpty())
        }

    /** And a peer that never claimed `SINK` is never sent an upsert. */
    @Test
    fun `a peer that never claimed sink receives no upsert`() = runTest {
        val harness = Harness()
        val wire = Wire()
        harness.source.start(producerScope())
        harness.source.attachListener(harness.listener)
        harness.source.onListenerConnected()
        harness.source.attachSession(peer, "peer-device", wire.send)
        advanceUntilIdle()

        harness.source.onPosted(notification())
        advanceUntilIdle()

        assertTrue(wire.upserts().isEmpty())
    }

    /** A usable secret is the third leg: without one, nothing is a source. */
    @Test
    fun `an unavailable secret store makes the device announce no source role`() = runTest {
        val harness = Harness()
        harness.secretStore.stored = null
        harness.metadata.notificationSecretGeneration = 0
        val wire = Wire()

        // With a store that cannot even create, there is no secret at all.
        val broken = Harness(secretStore = object : FakeSecretStore() {
            override fun load(): SecretKey? = throw IllegalStateException("keystore down")
        })
        broken.source.start(producerScope())
        broken.source.attachListener(broken.listener)
        broken.source.onListenerConnected()
        broken.source.attachSession(peer, "peer-device", wire.send)
        broken.source.onInbound(peer, sinkRoles())
        advanceUntilIdle()

        broken.source.onPosted(notification())
        advanceUntilIdle()

        assertEquals(0, wire.roles().first().roles.rolesCount)
        assertTrue(wire.upserts().isEmpty())
    }

    // -- the hard rules, through the whole pipeline ---------------------------

    /**
     * The own-package rule, exercised where it matters: through the real
     * producer, with a policy that explicitly names our own package.
     */
    @Test
    fun `our own notification is never mirrored, even when explicitly allowed`() = runTest {
        val harness = Harness(
            policy = NotificationPolicy(
                allowedApps = setOf(allowedApp, "io.github.yurisismotto.anyflow"),
                includeOngoing = true,
                whenSourceLocked = LockPolicy.FULL,
            ),
        )
        val wire = Wire()
        harness.source.start(producerScope())
        harness.source.attachListener(harness.listener)
        harness.source.onListenerConnected()
        harness.source.attachSession(peer, "peer-device", wire.send)
        harness.source.onInbound(peer, sinkRoles())
        advanceUntilIdle()

        // The ongoing-connection foreground-service notification, which exists
        // on every running install.
        harness.source.onPosted(
            notification(
                key = "0|$ownPackage|1|null|10999",
                packageName = ownPackage,
                ongoing = true,
            ),
        )
        harness.source.onPosted(notification())
        advanceUntilIdle()

        assertEquals(1, wire.upserts().size)
        assertEquals(allowedApp, wire.upserts().single().upsert.appId)
    }

    @Test
    fun `a denied app is never mirrored`() = runTest {
        val harness = Harness()
        val wire = Wire()
        harness.source.start(producerScope())
        harness.source.attachListener(harness.listener)
        harness.source.onListenerConnected()
        harness.source.attachSession(peer, "peer-device", wire.send)
        harness.source.onInbound(peer, sinkRoles())
        advanceUntilIdle()

        harness.source.onPosted(notification(key = "0|$deniedApp|1|null|10124", packageName = deniedApp))
        advanceUntilIdle()

        assertTrue(wire.upserts().isEmpty())
    }

    @Test
    fun `a secret-visibility notification is never mirrored`() = runTest {
        val harness = Harness()
        val wire = Wire()
        harness.source.start(producerScope())
        harness.source.attachListener(harness.listener)
        harness.source.onListenerConnected()
        harness.source.attachSession(peer, "peer-device", wire.send)
        harness.source.onInbound(peer, sinkRoles())
        advanceUntilIdle()

        harness.source.onPosted(
            notification(visibility = NotificationMapping.VISIBILITY_SECRET),
        )
        advanceUntilIdle()

        assertTrue(wire.upserts().isEmpty())
    }

    // -- identity ------------------------------------------------------------

    /** An update keeps the identity and changes the content. */
    @Test
    fun `an update keeps the same notification id`() = runTest {
        val harness = Harness()
        val wire = Wire()
        harness.source.start(producerScope())
        harness.source.attachListener(harness.listener)
        harness.source.onListenerConnected()
        harness.source.attachSession(peer, "peer-device", wire.send)
        harness.source.onInbound(peer, sinkRoles())
        advanceUntilIdle()

        harness.source.onPosted(notification(title = "FIXTURE-TITLE-CANARY"))
        advanceUntilIdle()
        harness.source.onPosted(notification(title = "FIXTURE-TITLE-CANARY EDITED"))
        advanceUntilIdle()

        val upserts = wire.upserts()
        assertEquals(2, upserts.size)
        assertEquals(upserts[0].upsert.notificationId, upserts[1].upsert.notificationId)
        assertTrue(upserts[0].upsert.contentHash != upserts[1].upsert.contentHash)
    }

    /** An identical re-post is not sent twice. */
    @Test
    fun `an identical repost produces no second message`() = runTest {
        val harness = Harness()
        val wire = Wire()
        harness.source.start(producerScope())
        harness.source.attachListener(harness.listener)
        harness.source.onListenerConnected()
        harness.source.attachSession(peer, "peer-device", wire.send)
        harness.source.onInbound(peer, sinkRoles())
        advanceUntilIdle()

        harness.source.onPosted(notification())
        advanceUntilIdle()
        harness.source.onPosted(notification())
        advanceUntilIdle()

        assertEquals(1, wire.upserts().size)
    }

    /** The removal names the same identity the upsert did. */
    @Test
    fun `a removal targets the id the upsert carried`() = runTest {
        val harness = Harness()
        val wire = Wire()
        harness.source.start(producerScope())
        harness.source.attachListener(harness.listener)
        harness.source.onListenerConnected()
        harness.source.attachSession(peer, "peer-device", wire.send)
        harness.source.onInbound(peer, sinkRoles())
        advanceUntilIdle()

        val fixture = notification()
        harness.source.onPosted(fixture)
        advanceUntilIdle()
        harness.source.onRemoved(fixture.platformKey)
        advanceUntilIdle()

        assertEquals(
            wire.upserts().single().upsert.notificationId,
            wire.removes().single().remove.notificationId,
        )
        // No removal reason: the message has two fields and neither is one.
        assertEquals(localDeviceId, wire.removes().single().remove.originDeviceId)
    }

    /** A peer is never told about a notification it was never sent. */
    @Test
    fun `a removal for a never-mirrored notification tells nobody`() = runTest {
        val harness = Harness()
        val wire = Wire()
        harness.source.start(producerScope())
        harness.source.attachListener(harness.listener)
        harness.source.onListenerConnected()
        harness.source.attachSession(peer, "peer-device", wire.send)
        harness.source.onInbound(peer, sinkRoles())
        advanceUntilIdle()

        val denied = notification(key = "0|$deniedApp|1|null|10124", packageName = deniedApp)
        harness.source.onPosted(denied)
        harness.source.onRemoved(denied.platformKey)
        advanceUntilIdle()

        assertTrue(wire.removes().isEmpty())
    }

    // -- ordering ------------------------------------------------------------

    /**
     * The property one ordered producer exists to guarantee: a removal never
     * overtakes the upsert it refers to. If it could, the sink would be left
     * showing a notification the phone no longer has — permanently, because
     * the removal has already happened.
     */
    @Test
    fun `a removal never overtakes its upsert`() = runTest {
        val harness = Harness()
        val wire = Wire()
        harness.source.start(producerScope())
        harness.source.attachListener(harness.listener)
        harness.source.onListenerConnected()
        harness.source.attachSession(peer, "peer-device", wire.send)
        harness.source.onInbound(peer, sinkRoles())
        advanceUntilIdle()

        for (index in 0 until 25) {
            val fixture = notification(key = "0|$allowedApp|$index|null|10123")
            harness.source.onPosted(fixture)
            advanceUntilIdle()
            harness.source.onRemoved(fixture.platformKey)
            advanceUntilIdle()
        }

        val ordered = wire.bodies().filter { it.startsWith("upsert") || it == "remove" }
        assertEquals(50, ordered.size)
        for (index in ordered.indices step 2) {
            assertTrue(ordered[index].startsWith("upsert"))
            assertEquals("remove", ordered[index + 1])
        }
    }

    // -- the snapshot --------------------------------------------------------

    /**
     * `BEGIN`, the currently-active items, `END` — and only what a live
     * notification would pass, because the snapshot is a reconciliation
     * mechanism and not a privileged path.
     */
    @Test
    fun `the snapshot brackets exactly the active, allowed notifications`() = runTest {
        val active = listOf(
            notification(key = "0|$allowedApp|1|null|10123", postedAt = 3),
            notification(key = "0|$deniedApp|2|null|10124", packageName = deniedApp, postedAt = 2),
            notification(key = "0|$ownPackage|3|null|10999", packageName = ownPackage, postedAt = 1),
        )
        val harness = Harness(listener = FakeListener(active = active))
        val wire = Wire()
        harness.source.start(producerScope())
        harness.source.attachListener(harness.listener)
        harness.source.onListenerConnected()
        harness.source.attachSession(peer, "peer-device", wire.send)
        harness.source.onInbound(peer, sinkRoles())
        advanceUntilIdle()

        val bodies = wire.bodies()
        val begin = bodies.indexOf("sync:BEGIN")
        val end = bodies.indexOf("sync:END")
        assertTrue("a snapshot must be bracketed", begin >= 0 && end > begin)
        assertEquals(listOf("upsert:$allowedApp"), bodies.subList(begin + 1, end))

        // One snapshot, one sync id, and the two markers agree.
        val markers = wire.messages.filter { it.bodyCase == NotificationControl.BodyCase.SYNC }
        assertEquals(2, markers.size)
        assertEquals(markers[0].sync.syncId, markers[1].sync.syncId)
        assertEquals(NotificationLimits.SYNC_ID_LENGTH, markers[0].sync.syncId.size())
        assertEquals(SyncMarker.Phase.PHASE_BEGIN, markers[0].sync.phase)
        assertEquals(SyncMarker.Phase.PHASE_END, markers[1].sync.phase)
    }

    @Test
    fun `a snapshot is sent once per connection`() = runTest {
        val harness = Harness(listener = FakeListener(active = listOf(notification())))
        val wire = Wire()
        harness.source.start(producerScope())
        harness.source.attachListener(harness.listener)
        harness.source.onListenerConnected()
        harness.source.attachSession(peer, "peer-device", wire.send)
        harness.source.onInbound(peer, sinkRoles(1))
        harness.source.onInbound(peer, sinkRoles(2))
        advanceUntilIdle()

        assertEquals(1, wire.bodies().count { it == "sync:BEGIN" })
    }

    @Test
    fun `the snapshot is bounded`() = runTest {
        val active = (0 until NotificationLimits.MAX_SNAPSHOT_ENTRIES + 50).map {
            notification(key = "0|$allowedApp|$it|null|10123", postedAt = it.toLong())
        }
        val harness = Harness(listener = FakeListener(active = active))
        val wire = Wire()
        harness.source.start(producerScope())
        harness.source.attachListener(harness.listener)
        harness.source.onListenerConnected()
        harness.source.attachSession(peer, "peer-device", wire.send)
        harness.source.onInbound(peer, sinkRoles())
        advanceUntilIdle()

        assertEquals(NotificationLimits.MAX_SNAPSHOT_ENTRIES, wire.upserts().size)
        assertEquals(1, wire.bodies().count { it == "sync:END" })
    }

    /**
     * A restart re-derives the same ids over the same shade, because the
     * secret is persistent and the map is rebuilt rather than restored. This
     * is what makes a reconnect invisible instead of a wall of duplicates.
     */
    @Test
    fun `a restart derives the same ids for the same active notifications`() = runTest {
        val active = listOf(notification())
        val store = FakeSecretStore()

        val first = Wire()
        val harnessOne = Harness(listener = FakeListener(active = active), secretStore = store)
        harnessOne.source.start(producerScope())
        harnessOne.source.attachListener(harnessOne.listener)
        harnessOne.source.onListenerConnected()
        harnessOne.source.attachSession(peer, "peer-device", first.send)
        harnessOne.source.onInbound(peer, sinkRoles())
        advanceUntilIdle()

        val second = Wire()
        val harnessTwo = Harness(listener = FakeListener(active = active), secretStore = store)
        harnessTwo.source.start(producerScope())
        harnessTwo.source.attachListener(harnessTwo.listener)
        harnessTwo.source.onListenerConnected()
        harnessTwo.source.attachSession(peer, "peer-device", second.send)
        harnessTwo.source.onInbound(peer, sinkRoles())
        advanceUntilIdle()

        assertEquals(
            first.upserts().single().upsert.notificationId,
            second.upserts().single().upsert.notificationId,
        )
    }

    /** A rotated secret is a new id space: an identity reset resets mirrors. */
    @Test
    fun `a rotated secret changes every id`() = runTest {
        val active = listOf(notification())
        val store = FakeSecretStore()

        val before = Wire()
        val one = Harness(listener = FakeListener(active = active), secretStore = store)
        one.source.start(producerScope())
        one.source.attachListener(one.listener)
        one.source.onListenerConnected()
        one.source.attachSession(peer, "peer-device", before.send)
        one.source.onInbound(peer, sinkRoles())
        advanceUntilIdle()

        store.destroy()

        val after = Wire()
        val two = Harness(listener = FakeListener(active = active), secretStore = store)
        two.source.start(producerScope())
        two.source.attachListener(two.listener)
        two.source.onListenerConnected()
        two.source.attachSession(peer, "peer-device", after.send)
        two.source.onInbound(peer, sinkRoles())
        advanceUntilIdle()

        assertTrue(
            before.upserts().single().upsert.notificationId !=
                after.upserts().single().upsert.notificationId,
        )
    }

    // -- role narrowing ------------------------------------------------------

    /**
     * The revocation story, and it must be structurally true rather than a
     * promise: the phone says it is no longer a source, on the session that is
     * already up, before anything else happens — and stops sending.
     */
    @Test
    fun `revoking notification access narrows the role and stops emission`() = runTest {
        val listener = FakeListener()
        val harness = Harness(listener = listener)
        val wire = Wire()
        harness.source.start(producerScope())
        harness.source.attachListener(listener)
        harness.source.onListenerConnected()
        harness.source.attachSession(peer, "peer-device", wire.send)
        harness.source.onInbound(peer, sinkRoles())
        advanceUntilIdle()

        harness.source.onPosted(notification(key = "0|$allowedApp|1|null|10123"))
        advanceUntilIdle()
        assertEquals(1, wire.upserts().size)

        // The user revokes access in Settings: the platform fires
        // onListenerDisconnected on a healthy session.
        harness.access.granted = false
        harness.source.onListenerDisconnected()
        advanceUntilIdle()

        harness.source.onPosted(notification(key = "0|$allowedApp|2|null|10123"))
        advanceUntilIdle()

        val roles = wire.roles()
        assertEquals(2, roles.size)
        assertEquals(1, roles[0].roles.epoch)
        assertEquals(1, roles[0].roles.rolesCount)
        assertEquals(2, roles[1].roles.epoch)
        assertEquals(0, roles[1].roles.rolesCount)
        // Nothing further was emitted, and no reconnect was needed to say so.
        assertEquals(1, wire.upserts().size)
    }

    // -- inbound -------------------------------------------------------------

    /**
     * N1 is a source and nothing else. A `DismissRequest` is answered
     * `REJECTED_ROLE` and **nothing happens on the device**: there is no path
     * from an inbound message to `cancelNotification` in this wave.
     */
    @Test
    fun `a dismiss request is refused and executes nothing`() = runTest {
        val harness = Harness()
        val wire = Wire()
        harness.source.start(producerScope())
        harness.source.attachListener(harness.listener)
        harness.source.onListenerConnected()
        harness.source.attachSession(peer, "peer-device", wire.send)
        harness.source.onInbound(peer, sinkRoles())
        advanceUntilIdle()

        val fixture = notification()
        harness.source.onPosted(fixture)
        advanceUntilIdle()

        harness.source.onInbound(
            peer,
            NotificationControl.newBuilder()
                .setDismiss(
                    io.github.yurisismotto.anyflow.proto.capabilities.DismissRequest.newBuilder()
                        .setNotificationId(wire.upserts().single().upsert.notificationId)
                        .setOriginDeviceId(localDeviceId),
                )
                .build(),
        )
        advanceUntilIdle()

        assertEquals(
            NotificationOutcome.NOTIFICATION_OUTCOME_REJECTED_ROLE,
            wire.messages.last().result.outcome,
        )
        // The notification is still tracked: nothing cancelled it.
        assertTrue(wire.removes().isEmpty())
    }

    /** A bad-width identifier is refused and **not answered**. */
    @Test
    fun `a malformed identifier is not answered`() = runTest {
        val harness = Harness()
        val wire = Wire()
        harness.source.start(producerScope())
        harness.source.attachListener(harness.listener)
        harness.source.onListenerConnected()
        harness.source.attachSession(peer, "peer-device", wire.send)
        harness.source.onInbound(peer, sinkRoles())
        advanceUntilIdle()
        val before = wire.messages.size

        harness.source.onInbound(
            peer,
            NotificationControl.newBuilder()
                .setDismiss(
                    io.github.yurisismotto.anyflow.proto.capabilities.DismissRequest.newBuilder()
                        .setNotificationId(ByteString.copyFrom(ByteArray(4)))
                        .setOriginDeviceId(localDeviceId),
                )
                .build(),
        )
        advanceUntilIdle()

        assertEquals(before, wire.messages.size)
    }

    /** An ungranted peer's message is refused, and the grant is re-read. */
    @Test
    fun `an ungranted peer's message answers not authorized`() = runTest {
        val harness = Harness(policy = NotificationPolicy.DENIED)
        val wire = Wire()
        harness.source.start(producerScope())
        harness.source.attachListener(harness.listener)
        harness.source.onListenerConnected()
        harness.source.attachSession(peer, "peer-device", wire.send)
        advanceUntilIdle()

        harness.source.onInbound(
            peer,
            NotificationControl.newBuilder()
                .setDismiss(
                    io.github.yurisismotto.anyflow.proto.capabilities.DismissRequest.newBuilder()
                        .setNotificationId(ByteString.copyFrom(ByteArray(16)))
                        .setOriginDeviceId(localDeviceId),
                )
                .build(),
        )
        advanceUntilIdle()

        assertEquals(
            NotificationOutcome.NOTIFICATION_OUTCOME_NOT_AUTHORIZED,
            wire.messages.last().result.outcome,
        )
    }

    // -- no relay ------------------------------------------------------------

    /**
     * A notification from one peer is never forwarded to another, and it is
     * enforced by absence: the only function that turns a notification into
     * outbound traffic takes a *local platform notification*, and there is no
     * code path from an inbound `NotificationUpsert` to an outbound one.
     */
    @Test
    fun `an inbound upsert is refused and never relayed`() = runTest {
        val harness = Harness()
        val one = Wire()
        val two = Wire()
        harness.source.start(producerScope())
        harness.source.attachListener(harness.listener)
        harness.source.onListenerConnected()
        harness.source.attachSession(peer, "peer-device-a", one.send)
        harness.source.attachSession(otherPeer, "peer-device-b", two.send)
        harness.source.onInbound(peer, sinkRoles())
        harness.source.onInbound(otherPeer, sinkRoles())
        advanceUntilIdle()

        val inbound = NotificationControl.newBuilder()
            .setUpsert(
                io.github.yurisismotto.anyflow.proto.capabilities.NotificationUpsert.newBuilder()
                    .setNotificationId(ByteString.copyFrom(ByteArray(16) { 0x5a }))
                    .setOriginDeviceId(localDeviceId)
                    .setAppId("example.relay.attempt")
                    .setTitle("FIXTURE-RELAY-CANARY"),
            )
            .build()
        harness.source.onInbound(peer, inbound)
        advanceUntilIdle()

        assertTrue(two.upserts().isEmpty())
        assertEquals(
            NotificationOutcome.NOTIFICATION_OUTCOME_REJECTED_ROLE,
            one.messages.last().result.outcome,
        )
    }

    // -- binding lifecycle ---------------------------------------------------

    /**
     * The claim "AnyFlow reads your notifications only while a granted
     * computer is connected", made structural: the binding follows live peer
     * state in both directions.
     */
    @Test
    fun `the listener is unbound when the last eligible peer goes away`() = runTest {
        val listener = FakeListener()
        val harness = Harness(listener = listener)
        val wire = Wire()
        harness.source.start(producerScope())
        harness.source.attachListener(listener)
        // The eligible peer arrives first, then the bind it asked for lands.
        harness.source.attachSession(peer, "peer-device", wire.send)
        harness.source.onListenerConnected()
        advanceUntilIdle()
        assertEquals("an eligible peer keeps the listener bound", 0, listener.unbinds)

        harness.source.detachSession(peer)
        advanceUntilIdle()
        assertEquals(1, listener.unbinds)
    }

    /**
     * A bind that lands after the peer that asked for it has gone is released
     * at once.
     *
     * `requestRebind` is asynchronous — the system starts the service in its
     * own time — so this race is real, and losing it would leave the listener
     * bound and reading for nobody. It was found on hardware rather than
     * reasoned about: on the SM-X620 the bind arrived after the requesting
     * session had already been torn down, and the listener stayed bound.
     */
    @Test
    fun `a bind that lands with no eligible peer is released immediately`() = runTest {
        val listener = FakeListener()
        val harness = Harness(listener = listener)
        harness.source.start(producerScope())
        harness.source.attachListener(listener)

        harness.source.onListenerConnected()
        advanceUntilIdle()

        assertTrue("a bind with nobody eligible must be released", listener.unbinds >= 1)
    }

    @Test
    fun `an ungranted peer is not an eligible peer`() = runTest {
        val listener = FakeListener()
        val harness = Harness(policy = NotificationPolicy.DENIED, listener = listener)
        val wire = Wire()
        harness.source.start(producerScope())
        harness.source.attachListener(listener)
        harness.source.onListenerConnected()
        harness.source.attachSession(peer, "peer-device", wire.send)
        advanceUntilIdle()

        // Connected, but not eligible: the listener is released rather than
        // left bound reading notifications for nobody, and no bind is ever
        // requested on an ungranted peer's behalf.
        assertTrue(listener.unbinds >= 1)
        assertEquals(0, harness.access.binds)
    }

    @Test
    fun `a granted peer connecting while unbound requests a rebind`() = runTest {
        val listener = FakeListener()
        val harness = Harness(listener = listener)
        val wire = Wire()
        harness.source.start(producerScope())
        harness.source.attachListener(listener)
        // Not connected: the system has not bound us.
        harness.source.attachSession(peer, "peer-device", wire.send)
        advanceUntilIdle()

        assertEquals(1, harness.access.binds)
    }

    // -- the canaries --------------------------------------------------------

    /**
     * Nothing anywhere in this capability's own state renders a notification.
     *
     * The strings are distinctive so that a leak would be unmistakable, and
     * the objects checked are the ones a log line, an exception message or a
     * crash report would actually reach.
     */
    @Test
    fun `no capability state renders notification content`() = runTest {
        val harness = Harness()
        val wire = Wire()
        harness.source.start(producerScope())
        harness.source.attachListener(harness.listener)
        harness.source.onListenerConnected()
        harness.source.attachSession(peer, "peer-device", wire.send)
        harness.source.onInbound(peer, sinkRoles())
        advanceUntilIdle()

        val fixture = notification()
        harness.source.onPosted(fixture)
        advanceUntilIdle()

        val renderings = listOf(
            fixture.toString(),
            harness.source.describeQueue(),
            harness.source.status.value.toString(),
            harness.policy.describe(),
        )
        for (rendered in renderings) {
            assertFalse("leaked a title: $rendered", rendered.contains(fixtureTitle))
            assertFalse("leaked a body: $rendered", rendered.contains(fixtureBody))
            assertFalse("leaked a platform key: $rendered", rendered.contains("|null|10123"))
        }
    }

    /** The status a UI could show holds counts and states, never content. */
    @Test
    fun `status reports counts and nothing else`() = runTest {
        val harness = Harness()
        val wire = Wire()
        harness.source.start(producerScope())
        harness.source.attachListener(harness.listener)
        harness.source.onListenerConnected()
        harness.source.attachSession(peer, "peer-device", wire.send)
        harness.source.onInbound(peer, sinkRoles())
        advanceUntilIdle()
        harness.source.onPosted(notification())
        advanceUntilIdle()

        val status = harness.source.status.value
        assertTrue(status.listenerConnected)
        assertTrue(status.accessGranted)
        assertTrue(status.secretAvailable)
        assertEquals(1, status.sessions)
        assertEquals(1, status.trackedNotifications)
        assertTrue(status.emitted > 0)
    }

    // -- backpressure --------------------------------------------------------

    /**
     * A burst of updates to one notification stays one notification, the last
     * state is what the peer ends up with, and the removal arrives last.
     *
     * The offering side never blocks — the listener callback runs on the
     * phone's main thread, so a full queue must never be a stall — and the
     * queue's own overflow behaviour is pinned in `NotificationQueueTest`,
     * where it can be exercised without a scheduler in the way.
     */
    @Test
    fun `a burst keeps one identity and the removal still arrives last`() = runTest {
        val harness = Harness(queueCapacity = 8)
        val wire = Wire()
        harness.source.start(producerScope())
        harness.source.attachListener(harness.listener)
        harness.source.onListenerConnected()
        harness.source.attachSession(peer, "peer-device", wire.send)
        harness.source.onInbound(peer, sinkRoles())
        advanceUntilIdle()

        repeat(500) { index ->
            harness.source.onPosted(notification(title = "FIXTURE-TITLE-CANARY $index"))
        }
        harness.source.onRemoved("0|$allowedApp|1|null|10123")
        advanceUntilIdle()

        // One identity throughout, however many updates it received.
        assertEquals(1, wire.upserts().map { it.upsert.notificationId }.distinct().size)
        // The last state delivered is the last state posted.
        assertEquals(
            "FIXTURE-TITLE-CANARY 499",
            wire.upserts().last().upsert.title,
        )
        // And the terminal event is last, never overtaken.
        assertEquals(1, wire.removes().size)
        assertEquals("remove", wire.bodies().last())
        // Only one notification was ever tracked.
        assertEquals(0, harness.source.status.value.trackedNotifications)
    }

    /** The id map is bounded no matter how many notifications pass through. */
    @Test
    fun `tracked notifications stay bounded`() = runTest {
        val harness = Harness()
        val wire = Wire()
        harness.source.start(producerScope())
        harness.source.attachListener(harness.listener)
        harness.source.onListenerConnected()
        harness.source.attachSession(peer, "peer-device", wire.send)
        harness.source.onInbound(peer, sinkRoles())
        advanceUntilIdle()

        repeat(NotificationLimits.MAX_TRACKED_NOTIFICATIONS + 200) { index ->
            harness.source.onPosted(notification(key = "0|$allowedApp|$index|null|10123"))
            advanceUntilIdle()
        }

        assertTrue(
            harness.source.status.value.trackedNotifications <=
                NotificationLimits.MAX_TRACKED_NOTIFICATIONS,
        )
    }

    // -- lock policy through the pipeline ------------------------------------

    /**
     * NOTIF-SEC-15, from the source end: a locked phone under the default
     * policy sends the app name only, and the title and body are **absent from
     * the encoded bytes** rather than present and flagged.
     */
    @Test
    fun `a locked phone transmits the app label and no content`() = runTest {
        val harness = Harness(locked = true)
        val wire = Wire()
        harness.source.start(producerScope())
        harness.source.attachListener(harness.listener)
        harness.source.onListenerConnected()
        harness.source.attachSession(peer, "peer-device", wire.send)
        harness.source.onInbound(peer, sinkRoles())
        advanceUntilIdle()

        harness.source.onPosted(notification())
        advanceUntilIdle()

        val message = wire.upserts().single()
        assertEquals("", message.upsert.title)
        assertEquals("", message.upsert.body)
        assertTrue(message.upsert.redacted)
        assertEquals("Label for $allowedApp", message.upsert.appLabel)

        // And not merely absent from the fields — absent from the bytes.
        val encoded = message.toByteArray()
        assertFalse(String(encoded, Charsets.ISO_8859_1).contains(fixtureTitle))
        assertFalse(String(encoded, Charsets.ISO_8859_1).contains(fixtureBody))
    }

    @Test
    fun `an unknown lock state behaves as locked`() {
        assertTrue(LockState.ALWAYS_LOCKED.isLocked())
    }

    // -- what is not on the wire ---------------------------------------------

    /**
     * The raw Android key never leaves this device — not truncated, not
     * hashed in place, not alongside — and neither does the uid or the numeric
     * profile id. Asserted against the encoded bytes of a full message.
     */
    @Test
    fun `no encoded message contains a platform key, a uid or a profile number`() = runTest {
        val harness = Harness(
            policy = NotificationPolicy(
                allowedApps = setOf(allowedApp),
                includeWorkProfile = true,
                whenSourceLocked = LockPolicy.FULL,
            ),
        )
        val wire = Wire()
        harness.source.start(producerScope())
        harness.source.attachListener(harness.listener)
        harness.source.onListenerConnected()
        harness.source.attachSession(peer, "peer-device", wire.send)
        harness.source.onInbound(peer, sinkRoles())
        advanceUntilIdle()

        harness.source.onPosted(
            notification(key = "10|$allowedApp|7|fixture-tag|10123", secondaryProfile = true),
        )
        advanceUntilIdle()

        val upsert = wire.upserts().single()
        val bytes = String(upsert.toByteArray(), Charsets.ISO_8859_1)
        assertFalse("the platform key must never be transmitted", bytes.contains("10|$allowedApp|7"))
        assertFalse("the uid must never be transmitted", bytes.contains("10123"))
        assertFalse("the tag must never be transmitted", bytes.contains("fixture-tag"))
        assertFalse("the group key must never be transmitted", bytes.contains("g:fixture"))
        // The profile is a boolean, and that is all: there is no field in
        // the schema that could carry the number, so this asserts the value
        // rather than the absence, which the `anyflow-proto` descriptor test
        // asserts structurally on the other side.
        assertTrue(upsert.upsert.secondaryProfile)
    }
}
