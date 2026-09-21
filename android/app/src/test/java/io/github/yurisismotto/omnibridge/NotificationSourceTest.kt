package io.github.yurisismotto.omnibridge

import com.google.protobuf.ByteString
import io.github.yurisismotto.omnibridge.identity.Fingerprint
import io.github.yurisismotto.omnibridge.notifications.LockPolicy
import io.github.yurisismotto.omnibridge.notifications.LockState
import io.github.yurisismotto.omnibridge.notifications.NotificationLimits
import io.github.yurisismotto.omnibridge.notifications.NotificationMapping
import io.github.yurisismotto.omnibridge.notifications.NotificationPolicy
import io.github.yurisismotto.omnibridge.notifications.NotificationOutboundQueue
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

    private val ownPackage = "io.github.yurisismotto.omnibridge"
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

        /**
         * Every key `cancel` was called with, in order.
         *
         * The raw platform key is exactly what must never leave the device, so
         * this list is also what the leak assertions read: a key recorded here
         * and found nowhere on the wire is the property N4 has to hold.
         */
        val cancelled = mutableListOf<String>()

        /** Makes the next cancel fail, as a platform refusal would. */
        var cancelSucceeds = true

        override fun requestUnbind() { unbinds += 1 }
        override fun activeNotifications(): List<PlatformNotification>? = active

        // The keyed lookup the dismissal path uses to re-check clearability
        // against the live platform rather than a stale wire value.
        override fun activeNotification(platformKey: String): PlatformNotification? =
            active?.firstOrNull { it.platformKey == platformKey }

        override fun cancel(platformKey: String): Boolean {
            cancelled += platformKey
            if (!cancelSucceeds) return false
            // The platform removes it and then reports the removal; the fake
            // does the first half and the test drives the second, so the two
            // can be ordered and raced deliberately.
            active = active?.filterNot { it.platformKey == platformKey }
            return true
        }

        // Names only, exactly as the real service does: the picker asks this
        // and never the one above, so no title or body is materialised for it.
        override fun activePackages(): List<String>? =
            active?.map { it.packageName }?.distinct()
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
            ownPackage = "io.github.yurisismotto.omnibridge",
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

        assertEquals("roles:2@1", wire.bodies().first())
        assertEquals(
            // ADR-0017 §1's v1 assignment for Android, in wire-number order.
            listOf(
                NotificationRole.NOTIFICATION_ROLE_SOURCE,
                NotificationRole.NOTIFICATION_ROLE_DISMISS_TARGET,
            ),
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
                allowedApps = setOf(allowedApp, "io.github.yurisismotto.omnibridge"),
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
        assertEquals(2, roles[0].roles.rolesCount)
        assertEquals(2, roles[1].roles.epoch)
        assertEquals(
            "both roles go together: without a listener there is nothing to " +
                "observe and nothing to cancel",
            0,
            roles[1].roles.rolesCount,
        )
        // Nothing further was emitted, and no reconnect was needed to say so.
        assertEquals(1, wire.upserts().size)
    }

    // -- inbound -------------------------------------------------------------

    // -- N4: dismissal ------------------------------------------------------
    //
    // `DismissRequest` is the only message in the capability that travels
    // sink -> source and causes an effect here, so these are the tests that
    // decide whether a paired computer can reach into this phone. Most of them
    // are negative, and the negative ones are the valuable ones: a mistake in
    // this section does not look like a failure on a laptop, it looks like
    // somebody's notifications clearing themselves.

    /** A phone with everything switched on, one notification, one peer. */
    private suspend fun TestScope.dismissable(
        policy: NotificationPolicy = NotificationPolicy(
            allowedApps = setOf("example.fixture.app"),
            allowDismissSync = true,
        ),
        notification: PlatformNotification = notification(),
    ): Triple<Harness, Wire, PlatformNotification> {
        val harness = Harness(policy = policy)
        harness.listener.active = listOf(notification)
        val wire = Wire()
        harness.source.start(producerScope())
        harness.source.attachListener(harness.listener)
        harness.source.onListenerConnected()
        harness.source.attachSession(peer, "peer-device", wire.send)
        harness.source.onInbound(peer, sinkRoles())
        advanceUntilIdle()
        harness.source.onPosted(notification)
        advanceUntilIdle()
        return Triple(harness, wire, notification)
    }

    private fun dismissFor(
        notificationId: ByteString,
        originDeviceId: String = "0123456789abcdef0123456789abcdef",
    ): NotificationControl =
        NotificationControl.newBuilder()
            .setDismiss(
                DismissRequest.newBuilder()
                    .setNotificationId(notificationId)
                    .setOriginDeviceId(originDeviceId),
            )
            .build()

    private fun Wire.lastOutcome(): NotificationOutcome = messages.last().result.outcome

    /**
     * The whole point of the wave: a computer's dismissal clears the original.
     *
     * `cancelNotification` is called **exactly once**, with the raw platform
     * key the source looked up itself, and the peer is told `REMOVED`.
     */
    @Test
    fun `a permitted dismiss request cancels the notification exactly once`() = runTest {
        val (harness, wire, fixture) = dismissable()
        val id = wire.upserts().single().upsert.notificationId

        harness.source.onInbound(peer, dismissFor(id))
        advanceUntilIdle()

        assertEquals(listOf(fixture.platformKey), harness.listener.cancelled)
        assertEquals(NotificationOutcome.NOTIFICATION_OUTCOME_REMOVED, wire.lastOutcome())
    }

    /**
     * The default, and the one that matters most: `allowDismissSync` is off
     * until a person turns it on (ADR-0015 §6), and an off policy cancels
     * nothing.
     *
     * Every other gate here is deliberately open — the peer is granted, the
     * listener is bound, the id resolves, the notification is clearable — so
     * the policy is the only thing holding it.
     */
    @Test
    fun `dismiss sync is off by default and cancels nothing`() = runTest {
        assertFalse(NotificationPolicy().allowDismissSync)

        val (harness, wire, _) = dismissable(
            policy = NotificationPolicy(allowedApps = setOf("example.fixture.app")),
        )
        val id = wire.upserts().single().upsert.notificationId

        harness.source.onInbound(peer, dismissFor(id))
        advanceUntilIdle()

        assertTrue(harness.listener.cancelled.isEmpty())
        assertEquals(
            NotificationOutcome.NOTIFICATION_OUTCOME_REJECTED_POLICY,
            wire.lastOutcome(),
        )
    }

    /**
     * Turning mirroring off makes a stale dismiss flag inert, the same
     * containment rule the desktop's `may_sync_dismissals` applies.
     */
    @Test
    fun `mirroring off defeats a stale dismiss sync flag`() = runTest {
        val (harness, wire, _) = dismissable()
        val id = wire.upserts().single().upsert.notificationId

        harness.policy = NotificationPolicy(
            allowMirror = false,
            allowedApps = setOf("example.fixture.app"),
            allowDismissSync = true,
        )
        harness.source.onInbound(peer, dismissFor(id))
        advanceUntilIdle()

        assertTrue(harness.listener.cancelled.isEmpty())
        assertEquals(
            NotificationOutcome.NOTIFICATION_OUTCOME_REJECTED_POLICY,
            wire.lastOutcome(),
        )
    }

    /** An ungranted peer is refused before anything else is consulted. */
    @Test
    fun `an ungranted peer cannot dismiss anything`() = runTest {
        val (harness, wire, _) = dismissable()
        val id = wire.upserts().single().upsert.notificationId

        harness.policy = NotificationPolicy.DENIED
        harness.source.onInbound(peer, dismissFor(id))
        advanceUntilIdle()

        assertTrue(harness.listener.cancelled.isEmpty())
        assertEquals(
            NotificationOutcome.NOTIFICATION_OUTCOME_NOT_AUTHORIZED,
            wire.lastOutcome(),
        )
    }

    /**
     * Revoking the grant mid-session takes effect on the next message.
     *
     * The policy is re-read per operation rather than captured when the
     * session came up, which is what makes a revocation bite on a connection
     * that is already established.
     */
    @Test
    fun `a grant revoked after the mirror was sent stops the dismissal`() = runTest {
        val (harness, wire, _) = dismissable()
        val id = wire.upserts().single().upsert.notificationId

        harness.policyOverride = { NotificationPolicy.DENIED }
        harness.source.onInbound(peer, dismissFor(id))
        advanceUntilIdle()

        assertTrue(harness.listener.cancelled.isEmpty())
    }

    /**
     * One peer cannot dismiss through another's grant.
     *
     * The policy is looked up by the **pinned fingerprint** the session was
     * built on, so a second computer with dismiss sync off is refused even
     * while the first has it on for the same notification.
     */
    @Test
    fun `enabling dismiss sync for one computer does not enable it for another`() = runTest {
        val harness = Harness()
        val fixture = notification()
        harness.listener.active = listOf(fixture)
        val allowed = Wire()
        val denied = Wire()
        harness.policyOverride = { fingerprint ->
            if (fingerprint == peer) {
                NotificationPolicy(
                    allowedApps = setOf("example.fixture.app"),
                    allowDismissSync = true,
                )
            } else {
                NotificationPolicy(allowedApps = setOf("example.fixture.app"))
            }
        }
        harness.source.start(producerScope())
        harness.source.attachListener(harness.listener)
        harness.source.onListenerConnected()
        harness.source.attachSession(peer, "peer-device", allowed.send)
        harness.source.attachSession(otherPeer, "other-device", denied.send)
        harness.source.onInbound(peer, sinkRoles())
        harness.source.onInbound(otherPeer, sinkRoles())
        advanceUntilIdle()
        harness.source.onPosted(fixture)
        advanceUntilIdle()

        val id = allowed.upserts().single().upsert.notificationId

        // The computer that was not allowed asks first, and is refused.
        harness.source.onInbound(otherPeer, dismissFor(id))
        advanceUntilIdle()
        assertTrue(harness.listener.cancelled.isEmpty())
        assertEquals(
            NotificationOutcome.NOTIFICATION_OUTCOME_REJECTED_POLICY,
            denied.lastOutcome(),
        )

        // The one that was, is not.
        harness.source.onInbound(peer, dismissFor(id))
        advanceUntilIdle()
        assertEquals(listOf(fixture.platformKey), harness.listener.cancelled)
    }

    /**
     * With no listener bound this phone cannot act on a dismissal, and it has
     * said so: the same condition that withholds `DISMISS_TARGET` answers
     * `REJECTED_ROLE` here.
     */
    @Test
    fun `a phone that is not a dismiss target refuses with rejected role`() = runTest {
        val (harness, wire, _) = dismissable()
        val id = wire.upserts().single().upsert.notificationId

        harness.source.onListenerDisconnected()
        advanceUntilIdle()
        harness.source.onInbound(peer, dismissFor(id))
        advanceUntilIdle()

        assertTrue(harness.listener.cancelled.isEmpty())
        assertEquals(
            NotificationOutcome.NOTIFICATION_OUTCOME_REJECTED_ROLE,
            wire.lastOutcome(),
        )
        // And the peer was told, before it asked, that this would happen.
        assertEquals(0, wire.roles().last().roles.rolesCount)
    }

    /**
     * A `DismissRequest` naming a different device is refused as malformed.
     *
     * A peer cannot dismiss a third device's notification through us, and the
     * refusal is a statement about the *message* rather than about any
     * notification this phone may or may not hold.
     */
    @Test
    fun `a dismiss request for another device's origin is refused`() = runTest {
        val (harness, wire, _) = dismissable()
        val id = wire.upserts().single().upsert.notificationId

        harness.source.onInbound(
            peer,
            dismissFor(id, originDeviceId = "ffffffffffffffffffffffffffffffff"),
        )
        advanceUntilIdle()

        assertTrue(harness.listener.cancelled.isEmpty())
        assertEquals(NotificationOutcome.NOTIFICATION_OUTCOME_INVALID, wire.lastOutcome())
    }

    /** And one whose origin is not an origin at all. */
    @Test
    fun `a malformed origin device id is refused`() = runTest {
        val (harness, wire, _) = dismissable()
        val id = wire.upserts().single().upsert.notificationId

        for (bad in listOf("", "short", "0123456789ABCDEF0123456789ABCDEF", "g".repeat(32))) {
            harness.source.onInbound(peer, dismissFor(id, originDeviceId = bad))
            advanceUntilIdle()
            assertEquals(
                "'$bad' must be refused as malformed",
                NotificationOutcome.NOTIFICATION_OUTCOME_INVALID,
                wire.lastOutcome(),
            )
        }
        assertTrue(harness.listener.cancelled.isEmpty())
    }

    /**
     * A bad-width identifier is refused **and not answered**: a
     * `NotificationResult` echoes the id, so a malformed one leaves nothing
     * coherent to correlate a reply with (ADR-0016 §9).
     */
    @Test
    fun `a bad width notification id in a dismiss is refused and not answered`() = runTest {
        val (harness, wire, _) = dismissable()
        val before = wire.messages.size

        for (width in listOf(0, 8, 15, 17, 32)) {
            harness.source.onInbound(
                peer,
                dismissFor(ByteString.copyFrom(ByteArray(width))),
            )
        }
        advanceUntilIdle()

        assertEquals("nothing may be answered", before, wire.messages.size)
        assertTrue(harness.listener.cancelled.isEmpty())
    }

    /**
     * An id this phone never issued converges as unknown.
     *
     * Not an error — and deliberately indistinguishable from an id that was
     * dismissed on the phone a moment ago, so a dismiss cannot be used to ask
     * whether a notification exists.
     */
    @Test
    fun `an unknown notification id converges rather than failing`() = runTest {
        val (harness, wire, _) = dismissable()

        harness.source.onInbound(
            peer,
            dismissFor(ByteString.copyFrom(ByteArray(16) { 0x5A })),
        )
        advanceUntilIdle()

        assertTrue(harness.listener.cancelled.isEmpty())
        assertEquals(
            NotificationOutcome.NOTIFICATION_OUTCOME_UNKNOWN_NOTIFICATION,
            wire.lastOutcome(),
        )
    }

    /** Dismissing twice: the first removes, the second converges. */
    @Test
    fun `a duplicate dismiss request is idempotent`() = runTest {
        val (harness, wire, fixture) = dismissable()
        val id = wire.upserts().single().upsert.notificationId

        harness.source.onInbound(peer, dismissFor(id))
        advanceUntilIdle()
        assertEquals(NotificationOutcome.NOTIFICATION_OUTCOME_REMOVED, wire.lastOutcome())

        // The platform reports the removal, exactly as it would.
        harness.source.onRemoved(fixture.platformKey, listenerCancelled = true)
        advanceUntilIdle()

        harness.source.onInbound(peer, dismissFor(id))
        advanceUntilIdle()
        assertEquals(
            NotificationOutcome.NOTIFICATION_OUTCOME_UNKNOWN_NOTIFICATION,
            wire.lastOutcome(),
        )
        assertEquals(
            "the platform must be asked exactly once",
            listOf(fixture.platformKey),
            harness.listener.cancelled,
        )
    }

    /**
     * A notification removed on the phone between the mirror appearing and the
     * dismissal arriving converges as unknown, and cancels nothing.
     */
    @Test
    fun `a notification removed before the request arrives converges`() = runTest {
        val (harness, wire, fixture) = dismissable()
        val id = wire.upserts().single().upsert.notificationId

        harness.listener.active = emptyList()
        harness.source.onRemoved(fixture.platformKey)
        advanceUntilIdle()

        harness.source.onInbound(peer, dismissFor(id))
        advanceUntilIdle()

        assertTrue(harness.listener.cancelled.isEmpty())
        assertEquals(
            NotificationOutcome.NOTIFICATION_OUTCOME_UNKNOWN_NOTIFICATION,
            wire.lastOutcome(),
        )
    }

    /**
     * A non-clearable notification is refused, and the refusal is read from
     * the **live** platform rather than from the flag the desktop holds.
     *
     * This is the case the re-check exists for: the mirror went out saying
     * `dismissible = true`, and the app has made the notification ongoing
     * since. The desktop's copy is stale and the phone does not take its word.
     */
    @Test
    fun `a notification that became ongoing after it was mirrored is refused`() = runTest {
        val clearable = notification()
        val (harness, wire, _) = dismissable(notification = clearable)
        val id = wire.upserts().single().upsert.notificationId
        assertTrue(
            "the desktop was told it could be dismissed",
            wire.upserts().single().upsert.dismissible,
        )

        // The app re-posts it as an ongoing, non-clearable notification.
        harness.listener.active = listOf(
            notification(key = clearable.platformKey, ongoing = true),
        )
        harness.source.onInbound(peer, dismissFor(id))
        advanceUntilIdle()

        assertTrue(harness.listener.cancelled.isEmpty())
        assertEquals(
            NotificationOutcome.NOTIFICATION_OUTCOME_NOT_DISMISSIBLE,
            wire.lastOutcome(),
        )
    }

    /** And one that was never dismissible in the first place. */
    @Test
    fun `an ongoing notification is never force cancelled`() = runTest {
        val ongoing = notification(ongoing = true)
        val harness = Harness(
            policy = NotificationPolicy(
                allowedApps = setOf("example.fixture.app"),
                includeOngoing = true,
                allowDismissSync = true,
            ),
        )
        harness.listener.active = listOf(ongoing)
        val wire = Wire()
        harness.source.start(producerScope())
        harness.source.attachListener(harness.listener)
        harness.source.onListenerConnected()
        harness.source.attachSession(peer, "peer-device", wire.send)
        harness.source.onInbound(peer, sinkRoles())
        advanceUntilIdle()
        harness.source.onPosted(ongoing)
        advanceUntilIdle()

        val upsert = wire.upserts().single().upsert
        assertFalse("the desktop is told it cannot be dismissed", upsert.dismissible)

        harness.source.onInbound(peer, dismissFor(upsert.notificationId))
        advanceUntilIdle()

        assertTrue(harness.listener.cancelled.isEmpty())
        assertEquals(
            NotificationOutcome.NOTIFICATION_OUTCOME_NOT_DISMISSIBLE,
            wire.lastOutcome(),
        )
    }

    /**
     * A platform that refuses the cancel is reported honestly, and releases
     * the echo-suppression entry so a later genuine removal is not swallowed.
     */
    @Test
    fun `a cancel the platform refuses is reported as failed`() = runTest {
        val (harness, wire, fixture) = dismissable()
        val id = wire.upserts().single().upsert.notificationId
        harness.listener.cancelSucceeds = false

        harness.source.onInbound(peer, dismissFor(id))
        advanceUntilIdle()

        assertEquals(NotificationOutcome.NOTIFICATION_OUTCOME_FAILED, wire.lastOutcome())

        // The notification is still there, and a genuine removal still reaches
        // the peer: the released entry swallowed nothing.
        harness.listener.active = emptyList()
        harness.source.onRemoved(fixture.platformKey, listenerCancelled = false)
        advanceUntilIdle()
        assertEquals(1, wire.removes().size)
    }

    // -- echo suppression ---------------------------------------------------

    /**
     * The critical sequence, whole.
     *
     * A computer dismisses, the phone cancels, the platform reports its own
     * cancellation — and the computer that asked is **not** sent a removal for
     * a mirror it purged before it ever sent the request.
     */
    @Test
    fun `a dismissal does not echo a removal back to the peer that asked`() = runTest {
        val (harness, wire, fixture) = dismissable()
        val id = wire.upserts().single().upsert.notificationId

        harness.source.onInbound(peer, dismissFor(id))
        advanceUntilIdle()
        harness.source.onRemoved(fixture.platformKey, listenerCancelled = true)
        advanceUntilIdle()

        assertTrue(
            "the requesting peer must not be told about its own dismissal",
            wire.removes().isEmpty(),
        )
        // And nothing looped: one result, one cancel, and no second message.
        assertEquals(listOf(fixture.platformKey), harness.listener.cancelled)
        assertEquals(NotificationOutcome.NOTIFICATION_OUTCOME_REMOVED, wire.lastOutcome())
    }

    /**
     * Every *other* granted computer is still told, because for them the
     * notification really did just disappear.
     */
    @Test
    fun `other peers are still told about a dismissal they did not ask for`() = runTest {
        val harness = Harness(
            policy = NotificationPolicy(
                allowedApps = setOf("example.fixture.app"),
                allowDismissSync = true,
            ),
        )
        val fixture = notification()
        harness.listener.active = listOf(fixture)
        val asking = Wire()
        val other = Wire()
        harness.source.start(producerScope())
        harness.source.attachListener(harness.listener)
        harness.source.onListenerConnected()
        harness.source.attachSession(peer, "peer-device", asking.send)
        harness.source.attachSession(otherPeer, "other-device", other.send)
        harness.source.onInbound(peer, sinkRoles())
        harness.source.onInbound(otherPeer, sinkRoles())
        advanceUntilIdle()
        harness.source.onPosted(fixture)
        advanceUntilIdle()

        harness.source.onInbound(peer, dismissFor(asking.upserts().single().upsert.notificationId))
        advanceUntilIdle()
        harness.source.onRemoved(fixture.platformKey, listenerCancelled = true)
        advanceUntilIdle()

        assertTrue(asking.removes().isEmpty())
        assertEquals(
            "the other computer's mirror is still on its screen and must come off",
            1,
            other.removes().size,
        )
    }

    /**
     * A person swiping the notification away on the phone is **not** a
     * listener cancel, and reaches every peer — including one that happens to
     * have a dismissal pending for the same identity.
     */
    @Test
    fun `a removal the user performed on the phone is never suppressed`() = runTest {
        val (harness, wire, fixture) = dismissable()

        harness.listener.active = emptyList()
        harness.source.onRemoved(fixture.platformKey, listenerCancelled = false)
        advanceUntilIdle()

        assertEquals(1, wire.removes().size)
    }

    /**
     * Single use: a re-post under the same key after a cancel must not have
     * its *next*, genuine removal swallowed. That would leave a mirror on a
     * computer that nothing could ever take off.
     */
    @Test
    fun `echo suppression is consumed once and never swallows a later removal`() = runTest {
        val (harness, wire, fixture) = dismissable()
        val id = wire.upserts().single().upsert.notificationId

        harness.source.onInbound(peer, dismissFor(id))
        advanceUntilIdle()
        harness.source.onRemoved(fixture.platformKey, listenerCancelled = true)
        advanceUntilIdle()
        assertTrue(wire.removes().isEmpty())

        // The app posts again under the same key, and it is genuinely removed.
        harness.listener.active = listOf(fixture)
        harness.source.onPosted(fixture)
        advanceUntilIdle()
        harness.listener.active = emptyList()
        harness.source.onRemoved(fixture.platformKey, listenerCancelled = true)
        advanceUntilIdle()

        assertEquals(
            "the entry was single use; the second removal is genuine",
            1,
            wire.removes().size,
        )
    }

    // -- what must never leave the device -----------------------------------

    /**
     * NOTIF-SEC: the raw Android key is never transmitted, in any message, on
     * any path — including the one whose whole job is to consume it.
     *
     * The fixture key contains a canary, so a key that reached the wire in any
     * encoding this test can see would be found. It is asserted against the
     * *encoded bytes*, not against a field, because a future field added in
     * the wrong place would still be caught.
     */
    @Test
    fun `the raw platform key never reaches the wire on the dismissal path`() = runTest {
        val keyed = notification(key = "0|example.fixture.app|1|RAWKEY-CANARY|10123")
        val (harness, wire, _) = dismissable(notification = keyed)
        val id = wire.upserts().single().upsert.notificationId

        harness.source.onInbound(peer, dismissFor(id))
        advanceUntilIdle()
        harness.source.onRemoved(keyed.platformKey, listenerCancelled = true)
        advanceUntilIdle()

        assertEquals(listOf(keyed.platformKey), harness.listener.cancelled)
        for (message in wire.messages) {
            val encoded = String(message.toByteArray(), Charsets.ISO_8859_1)
            assertFalse(
                "the raw platform key reached the wire in ${message.bodyCase}",
                encoded.contains("RAWKEY-CANARY"),
            )
            assertFalse(encoded.contains("10123"))
        }
    }

    /**
     * And neither does any notification content, on the dismissal path.
     *
     * A `DismissRequest` is answered with an identity and an outcome enum, and
     * this asserts it against the bytes rather than against the fields.
     */
    @Test
    fun `a dismissal answer carries no notification content`() = runTest {
        val (harness, wire, _) = dismissable()
        val id = wire.upserts().single().upsert.notificationId
        val before = wire.messages.size

        harness.source.onInbound(peer, dismissFor(id))
        advanceUntilIdle()

        val answer = wire.messages.drop(before).single()
        assertEquals(NotificationControl.BodyCase.RESULT, answer.bodyCase)
        val encoded = String(answer.toByteArray(), Charsets.ISO_8859_1)
        for (canary in listOf(fixtureTitle, fixtureBody, "example.fixture.app")) {
            assertFalse("$canary reached a dismissal answer", encoded.contains(canary))
        }
        // Two fields: a 16-byte identity and an enum.
        assertTrue("an answer is tiny", answer.toByteArray().size < 32)
    }

    /**
     * A peer cannot name an Android notification by package, id or tag.
     *
     * There is no field in `DismissRequest` that could carry one — this test
     * asserts the consequence: the only handle that works is the derived id
     * this phone issued, and something that merely *looks* like a platform key
     * resolves to nothing.
     */
    @Test
    fun `no remote field can identify an android notification directly`() = runTest {
        val (harness, wire, fixture) = dismissable()

        // The real platform key, padded to the right width. It is 16 bytes of
        // something, and it maps to nothing, because the map is keyed on HMAC
        // output and not on anything a peer can construct.
        val spoofed = fixture.platformKey.toByteArray(Charsets.UTF_8)
            .copyOf(16)
        harness.source.onInbound(peer, dismissFor(ByteString.copyFrom(spoofed)))
        advanceUntilIdle()

        assertTrue(harness.listener.cancelled.isEmpty())
        assertEquals(
            NotificationOutcome.NOTIFICATION_OUTCOME_UNKNOWN_NOTIFICATION,
            wire.lastOutcome(),
        )
    }

    /**
     * A notification this phone never mirrored to anybody cannot be targeted.
     *
     * The id map holds only notifications that passed the hard screen, so a
     * peer guessing an id reaches nothing — and an id for a notification from
     * a denied app was never issued in the first place.
     */
    @Test
    fun `a notification that was never mirrored cannot be dismissed remotely`() = runTest {
        val denied = notification(
            key = "0|example.denied.app|9|null|10999",
            packageName = deniedApp,
        )
        val (harness, wire, _) = dismissable()
        harness.listener.active = harness.listener.active.orEmpty() + denied
        harness.source.onPosted(denied)
        advanceUntilIdle()

        // Nothing about it left the device, so the desktop has no id for it.
        assertEquals(1, wire.upserts().size)

        // And guessing the id it *would* have had reaches nothing either: the
        // derivation needs this device's secret.
        harness.source.onInbound(
            peer,
            dismissFor(ByteString.copyFrom(ByteArray(16) { 0x77 })),
        )
        advanceUntilIdle()
        assertTrue(harness.listener.cancelled.isEmpty())
    }

    /**
     * The role announcement and the counters a UI reads.
     *
     * `DISMISS_TARGET` is announced from the moment the phone can act, the
     * epoch is monotonic, and the counters are counts with nothing in them
     * that could name a notification.
     */
    @Test
    fun `the status reports both roles and content free dismissal counters`() = runTest {
        val (harness, wire, _) = dismissable()
        val id = wire.upserts().single().upsert.notificationId

        harness.source.onInbound(peer, dismissFor(id))
        advanceUntilIdle()

        val status = harness.source.status.value.peers.getValue(peer.toHex())
        assertTrue(status.localIsSource)
        assertTrue(status.localIsDismissTarget)
        assertEquals(1, status.dismissRequests)
        assertEquals(1, status.dismissesPerformed)
        assertFalse("the computer announced SINK only", status.peerIsDismissReporter)
        assertFalse(status.toString().contains(fixtureTitle))
        assertFalse(status.toString().contains(fixtureBody))
    }

    /** A computer that says it will report dismissals is recorded as such. */
    @Test
    fun `a peer claiming dismiss reporter is recorded and grants itself nothing`() = runTest {
        val (harness, wire, _) = dismissable()

        harness.source.onInbound(
            peer,
            NotificationControl.newBuilder()
                .setRoles(
                    NotificationRoles.newBuilder()
                        .addRoles(NotificationRole.NOTIFICATION_ROLE_SINK)
                        .addRoles(NotificationRole.NOTIFICATION_ROLE_DISMISS_REPORTER)
                        .setEpoch(2),
                )
                .build(),
        )
        advanceUntilIdle()

        assertTrue(
            harness.source.status.value.peers.getValue(peer.toHex()).peerIsDismissReporter,
        )

        // And it changes nothing about authorization: with the policy off, the
        // claim buys the peer no dismissal at all.
        harness.policy = NotificationPolicy(allowedApps = setOf("example.fixture.app"))
        harness.source.onInbound(peer, dismissFor(wire.upserts().first().upsert.notificationId))
        advanceUntilIdle()
        assertTrue(harness.listener.cancelled.isEmpty())
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
                    io.github.yurisismotto.omnibridge.proto.capabilities.DismissRequest.newBuilder()
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
                    io.github.yurisismotto.omnibridge.proto.capabilities.DismissRequest.newBuilder()
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
                io.github.yurisismotto.omnibridge.proto.capabilities.NotificationUpsert.newBuilder()
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
     * The claim "OmniBridge reads your notifications only while a granted
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
        // rather than the absence, which the `omnibridge-proto` descriptor test
        // asserts structurally on the other side.
        assertTrue(upsert.upsert.secondaryProfile)
    }
}
