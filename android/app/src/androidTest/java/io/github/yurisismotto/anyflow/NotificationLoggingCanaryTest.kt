package io.github.yurisismotto.anyflow

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import com.google.protobuf.ByteString
import io.github.yurisismotto.anyflow.identity.Fingerprint
import io.github.yurisismotto.anyflow.notifications.LockPolicy
import io.github.yurisismotto.anyflow.notifications.LockState
import io.github.yurisismotto.anyflow.notifications.NotificationMapping
import io.github.yurisismotto.anyflow.notifications.NotificationPolicy
import io.github.yurisismotto.anyflow.notifications.NotificationSecret
import io.github.yurisismotto.anyflow.notifications.NotificationSource
import io.github.yurisismotto.anyflow.notifications.PlatformNotification
import io.github.yurisismotto.anyflow.proto.capabilities.NotificationControl
import io.github.yurisismotto.anyflow.proto.capabilities.NotificationRole
import io.github.yurisismotto.anyflow.proto.capabilities.NotificationRoles
import io.github.yurisismotto.anyflow.proto.capabilities.NotificationUpsert
import java.io.FileInputStream
import javax.crypto.SecretKey
import javax.crypto.spec.SecretKeySpec
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.delay
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith

/**
 * NOTIF-SEC-25, the Android half.
 *
 * N1 left this to N3 and N2 built the desktop twin (`tests/logging.rs`). This
 * is the same idea against the real platform logger: drive real flows —
 * including the ones that fail — with values no notification would ever
 * contain, capture **everything AnyFlow wrote to logcat**, and assert none of
 * it came out.
 *
 * ## Why it is an instrumented test
 *
 * `android.util.Log` is a stub on the JVM unit-test classpath: it returns a
 * default and writes nothing, so a canary suite there would pass by writing to
 * nothing at all. Here the log is the device's own, and it is read back
 * through the instrumentation's shell — which holds `READ_LOGS` — rather than
 * by the app, which does not and must not.
 *
 * ## It cannot pass vacuously
 *
 * Every test asserts the capture is **non-empty** before asserting what is not
 * in it. A suite that silently captured nothing would otherwise report a clean
 * bill of health for a build that logged everything.
 *
 * ## The error paths are the point
 *
 * A content leak does not usually happen on the happy path — it happens in an
 * exception message, a failed parse, or a diagnostic somebody added while
 * debugging. So the flows below include an authorizer that throws with the
 * canaries in its message, a notification whose every field is hostile, and a
 * refusal for a peer that was never granted anything.
 */
@RunWith(AndroidJUnit4::class)
class NotificationLoggingCanaryTest {

    // Distinctive enough that a substring match cannot be a coincidence, and
    // obviously synthetic so nothing here reads like somebody's message.
    private companion object {
        const val TITLE = "ANYFLOW-N3-CANARY-TITLE-Q7X"
        const val BODY = "ANYFLOW-N3-CANARY-BODY-Q7X"
        const val PACKAGE = "example.canary.q7x.app"
        const val DENIED_PACKAGE = "example.canary.q7x.denied"
        const val TAG = "anyflow-n3-canary-tag-q7x"
        const val KEY = "0|example.canary.q7x.app|4711|anyflow-n3-canary-tag-q7x|10123"
        const val LABEL = "Canary Q7X Label"

        /** Every AnyFlow tag that can write a line during these flows. */
        val TAGS = listOf(
            "NotificationSource",
            "AnyFlowListener",
            "AnyFlowApp",
            "NotificationQueue",
            "ClipboardSync",
        )
    }

    private val ownPackage = "io.github.yurisismotto.anyflow"
    private val peer = Fingerprint(ByteArray(32) { 0x31 })
    private val ungranted = Fingerprint(ByteArray(32) { 0x32 })
    private val scopes = mutableListOf<CoroutineScope>()
    private val sent = mutableListOf<NotificationControl>()

    @Before
    fun clearTheLog() {
        shell("logcat -c")
    }

    @After
    fun stopProducers() {
        scopes.forEach { it.cancel() }
        scopes.clear()
    }

    // -----------------------------------------------------------------------
    // The flows
    // -----------------------------------------------------------------------

    @Test
    fun a_normal_mirroring_flow_writes_no_notification_content() {
        val captured = runFlow {
            val source = source(policy = NotificationPolicy(allowedApps = setOf(PACKAGE)))
            val wire = attach(source)
            source.onPosted(notification())
            source.onPosted(notification(title = TITLE + "-2"))
            source.onRemoved(KEY)
            settle()
            assertTrue(
                "the flow produced no wire traffic, so it proves nothing",
                wire.isNotEmpty(),
            )
        }
        assertClean(captured)
    }

    @Test
    fun a_filtered_notification_writes_no_notification_content() {
        // Deny by default: the app was never named, so it is dropped. The drop
        // is logged, and the log line must be a reason code and nothing more.
        val captured = runFlow {
            val source = source(policy = NotificationPolicy(allowedApps = setOf(PACKAGE)))
            attach(source)
            source.onPosted(notification(packageName = DENIED_PACKAGE))
            settle()
        }
        assertClean(captured)
        assertFalse(
            "a drop line named the package it dropped",
            captured.contains(DENIED_PACKAGE),
        )
    }

    @Test
    fun a_denied_peer_writes_no_notification_content() {
        val captured = runFlow {
            val source = source(policy = NotificationPolicy.DENIED)
            attach(source)
            source.onPosted(notification())
            // And an inbound message from a peer that holds no grant, which is
            // refused and answered — a path that carries an identifier.
            source.onInbound(
                peer,
                NotificationControl.newBuilder()
                    .setUpsert(
                        NotificationUpsert.newBuilder()
                            .setNotificationId(ByteString.copyFrom(ByteArray(16) { 0x5a }))
                            .setTitle(TITLE)
                            .setBody(BODY)
                            .setAppId(PACKAGE)
                            .setAppLabel(LABEL),
                    )
                    .build(),
            )
            settle()
        }
        assertClean(captured)
    }

    @Test
    fun a_lock_reduced_flow_writes_no_notification_content() {
        val captured = runFlow {
            val source = source(
                policy = NotificationPolicy(
                    allowedApps = setOf(PACKAGE),
                    whenSourceLocked = LockPolicy.APP_ONLY,
                ),
                locked = true,
            )
            attach(source)
            source.onPosted(notification())
            settle()
        }
        assertClean(captured)
        // The reduction keeps the application's label on the wire, which is
        // correct — and it must still not reach the log.
        assertFalse("the reduced path logged the app label", captured.contains(LABEL))
    }

    @Test
    fun a_suppressed_flow_writes_no_notification_content() {
        val captured = runFlow {
            val source = source(
                policy = NotificationPolicy(
                    allowedApps = setOf(PACKAGE),
                    whenSourceLocked = LockPolicy.SUPPRESS,
                ),
                locked = true,
            )
            attach(source)
            source.onPosted(notification())
            settle()
        }
        assertClean(captured)
    }

    @Test
    fun a_hostile_notification_writes_no_notification_content() {
        // Every field is something an application chose, including the ones the
        // adapter reads defensively. A parser that reported what it could not
        // understand by quoting it would fail here.
        val captured = runFlow {
            val source = source(policy = NotificationPolicy(allowedApps = setOf(PACKAGE)))
            attach(source)
            source.onPosted(
                notification(
                    title = TITLE + " \n[31m%s%n{}",
                    body = BODY + " 💣 <b>x</b> " + "A".repeat(4096),
                    key = KEY + "| " + TITLE,
                ),
            )
            settle()
        }
        assertClean(captured)
    }

    @Test
    fun an_error_path_carrying_the_canaries_does_not_render_them() {
        // The path a leak actually hides in. The authorizer throws, and its
        // message contains everything that must never be logged; the source
        // catches it and fails closed, and what it writes must be a type name.
        val captured = runFlow {
            val source = source(
                policy = NotificationPolicy(allowedApps = setOf(PACKAGE)),
                throwing = true,
            )
            attach(source)
            source.onPosted(notification())
            source.onRemoved(KEY)
            settle()
        }
        assertClean(captured)
    }

    @Test
    fun a_snapshot_over_the_whole_shade_writes_no_notification_content() {
        val captured = runFlow {
            val listener = FakeListener(
                listOf(
                    notification(),
                    notification(
                        key = KEY + "-2",
                        title = TITLE + "-2",
                        body = BODY + "-2",
                    ),
                    notification(packageName = DENIED_PACKAGE, key = KEY + "-3"),
                ),
            )
            val source = source(
                policy = NotificationPolicy(allowedApps = setOf(PACKAGE)),
                listener = listener,
            )
            attach(source, listener)
            settle()
        }
        assertClean(captured)
    }

    // -----------------------------------------------------------------------
    // The assertion
    // -----------------------------------------------------------------------

    /**
     * Everything a leak would look like, in one place.
     *
     * The non-empty check is first and is not a formality: it is what stops
     * this whole file from passing because the capture failed.
     */
    private fun assertClean(captured: String) {
        assertTrue(
            "AnyFlow wrote nothing at all, so this test proves nothing",
            captured.isNotBlank(),
        )
        for (canary in listOf(TITLE, BODY, TAG, KEY, LABEL, PACKAGE)) {
            assertFalse(
                "AnyFlow's log contained $canary:\n$captured",
                captured.contains(canary),
            )
        }
        // The raw platform key, in another shape it could be written.
        assertFalse(
            "a raw StatusBarNotification key was logged",
            captured.contains("4711|"),
        )
    }

    // -----------------------------------------------------------------------
    // Harness
    // -----------------------------------------------------------------------

    private class FakeListener(
        val active: List<PlatformNotification> = emptyList(),
    ) : NotificationSource.ListenerControl {
        override fun requestUnbind() = Unit
        override fun activeNotifications(): List<PlatformNotification> = active
        override fun activePackages(): List<String> = active.map { it.packageName }.distinct()
    }

    private class FakeAccess : NotificationSource.AccessControl {
        override fun isAccessGranted(): Boolean = true
        override fun requestBind() = Unit
    }

    private class FakeStore : NotificationSecret.Store {
        private var stored: SecretKey? =
            SecretKeySpec(ByteArray(32) { it.toByte() }, "HmacSHA256")

        override fun load(): SecretKey? = stored
        override fun create(): SecretKey =
            SecretKeySpec(ByteArray(32) { (it + 7).toByte() }, "HmacSHA256").also { stored = it }

        override fun destroy() {
            stored = null
        }
    }

    private class FakeMetadata(override var notificationSecretGeneration: Int = 1) :
        NotificationSecret.Metadata

    private var listener: FakeListener = FakeListener()

    private fun source(
        policy: NotificationPolicy,
        locked: Boolean = false,
        throwing: Boolean = false,
        listener: FakeListener = FakeListener(),
    ): NotificationSource {
        this.listener = listener
        val metadata = FakeMetadata()
        val store = FakeStore()
        return NotificationSource(
            ownPackage = ownPackage,
            localDeviceId = "0123456789abcdef0123456789abcdef",
            authorizer = {
                if (throwing) {
                    error("authorizer failed for $TITLE / $BODY / $KEY / $PACKAGE")
                }
                policy
            },
            appLabels = { LABEL },
            secretProvider = { NotificationSecret.loadOrCreate(store, metadata) },
            lockState = LockState { locked },
            access = FakeAccess(),
        )
    }

    /** Starts the producer, binds the listener and brings a sink peer up. */
    private suspend fun attach(
        source: NotificationSource,
        listener: FakeListener = this.listener,
    ): List<NotificationControl> {
        val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default).also { scopes += it }
        source.start(scope)
        source.attachListener(listener)
        source.onListenerConnected()
        source.attachSession(peer, "peer-device") { payload ->
            synchronized(sent) { sent += NotificationControl.parseFrom(payload) }
        }
        // A second peer that was never granted anything, so the refusal path
        // runs in every flow rather than only in the one that tests it.
        source.attachSession(ungranted, "other-device") { }
        source.onInbound(
            peer,
            NotificationControl.newBuilder()
                .setRoles(
                    NotificationRoles.newBuilder()
                        .addRoles(NotificationRole.NOTIFICATION_ROLE_SINK)
                        .setEpoch(1),
                )
                .build(),
        )
        settle()
        return synchronized(sent) { sent.toList() }
    }

    /** The producer is a real coroutine on a real dispatcher; let it drain. */
    private suspend fun settle() = delay(250)

    private fun runFlow(body: suspend () -> Unit): String {
        synchronized(sent) { sent.clear() }
        runBlocking { body() }
        // Logcat is asynchronous; give the buffer a moment before reading it.
        Thread.sleep(300)
        return shell("logcat -d " + TAGS.joinToString(" ") { "$it:V" } + " *:S")
    }

    /**
     * Runs a shell command through the instrumentation.
     *
     * `logcat` needs `READ_LOGS`, which the *shell* holds and the application
     * does not — and must not: AnyFlow declares no `READ_LOGS`, and reading its
     * own log at runtime is not something it should be able to do. The test
     * harness borrowing the shell's permission is exactly the supported way to
     * audit what an app wrote.
     */
    private fun shell(command: String): String {
        val descriptor = InstrumentationRegistry.getInstrumentation()
            .uiAutomation
            .executeShellCommand(command)
        return FileInputStream(descriptor.fileDescriptor).use { it.readBytes().decodeToString() }
    }

    private fun notification(
        key: String = KEY,
        packageName: String = PACKAGE,
        title: String = TITLE,
        body: String = BODY,
    ) = PlatformNotification(
        platformKey = key,
        packageName = packageName,
        secondaryProfile = false,
        postedAtUnixMs = 1_700_000_000_000L,
        ongoing = false,
        clearable = true,
        visibility = NotificationMapping.VISIBILITY_PRIVATE,
        androidImportance = NotificationMapping.ANDROID_IMPORTANCE_DEFAULT,
        category = "msg",
        groupKey = "0|$packageName|g:$TAG",
        groupSummary = false,
        title = title,
        body = body,
        hasProgress = false,
        progressCurrent = 0,
        progressMax = 0,
        progressIndeterminate = false,
    )
}
