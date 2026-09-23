package io.github.yurisismotto.omnibridge

import com.google.protobuf.ByteString
import io.github.yurisismotto.omnibridge.capability.CapabilityRegistry
import io.github.yurisismotto.omnibridge.capability.NotificationsCapability
import io.github.yurisismotto.omnibridge.notifications.LockState
import io.github.yurisismotto.omnibridge.notifications.NotificationPolicy
import io.github.yurisismotto.omnibridge.notifications.NotificationSource
import io.github.yurisismotto.omnibridge.proto.capabilities.DismissRequest
import io.github.yurisismotto.omnibridge.proto.capabilities.NotificationCategory
import io.github.yurisismotto.omnibridge.proto.capabilities.NotificationControl
import io.github.yurisismotto.omnibridge.proto.capabilities.NotificationImportance
import io.github.yurisismotto.omnibridge.proto.capabilities.NotificationOutcome
import io.github.yurisismotto.omnibridge.proto.capabilities.NotificationPrivacy
import io.github.yurisismotto.omnibridge.proto.capabilities.NotificationRemove
import io.github.yurisismotto.omnibridge.proto.capabilities.NotificationResult
import io.github.yurisismotto.omnibridge.proto.capabilities.NotificationRole
import io.github.yurisismotto.omnibridge.proto.capabilities.NotificationRoles
import io.github.yurisismotto.omnibridge.proto.capabilities.NotificationUpsert
import io.github.yurisismotto.omnibridge.proto.capabilities.Progress
import io.github.yurisismotto.omnibridge.proto.capabilities.SyncMarker
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The `notifications.v1` schema, on the JVM.
 *
 * **Nothing in this file exercises a notification listener.** What is tested
 * here is that the Android bindings exist, round-trip, and agree byte-for-byte
 * with the desktop's; the source adapter that uses them is exercised by
 * `NotificationSourceTest` and the suites beside it.
 *
 * Every fixture is obviously synthetic. On the certification hardware the
 * platform's own OTP redaction did not fire at all (POC-NOTIF-01), so
 * notification text is treated as fully sensitive user data everywhere,
 * including in tests: nothing here is logged and nothing reads like a real
 * notification.
 *
 * Must stay in lockstep with `omnibridge_core::notifications` and
 * `desktop/core/tests/notifications_protocol.rs`.
 */
class NotificationsProtocolTest {

    private val syntheticDevice = "0123456789abcdef0123456789abcdef"

    private fun sequential(count: Int): ByteString =
        ByteString.copyFrom(ByteArray(count) { it.toByte() })

    private fun repeated(byte: Int, count: Int): ByteString =
        ByteString.copyFrom(ByteArray(count) { byte.toByte() })

    // -- the shared cross-language vector -------------------------------------

    /**
     * The exact bytes a fully-populated upsert encodes to.
     *
     * Asserted identically by `desktop/core/tests/notifications_protocol.rs`.
     * Both sides compile the same `.proto`; this is what proves they also
     * *agree* about it — a field number, a wire type or an enum value that
     * drifted on one side would fail here rather than on a user's desk.
     */
    private val canonicalUpsertHex =
        "12bd010a10000102030405060708090a0b0c0d0e0f12203031323334353637383961626364" +
            "6566303132333435363738396162636465661a136578616d706c652e666978747572652e" +
            "617070220b46697874757265204170702a0d46495854555245205449544c45320c464958" +
            "5455524520424f44593880d095ffbc314003480250015a0408031007600172080001020304" +
            "0506077801800101880101920120000102030405060708090a0b0c0d0e0f101112131415" +
            "161718191a1b1c1d1e1f"

    private fun canonicalUpsert(): NotificationControl =
        NotificationControl.newBuilder()
            .setUpsert(
                NotificationUpsert.newBuilder()
                    .setNotificationId(sequential(16))
                    .setOriginDeviceId(syntheticDevice)
                    .setAppId("example.fixture.app")
                    .setAppLabel("Fixture App")
                    .setTitle("FIXTURE TITLE")
                    .setBody("FIXTURE BODY")
                    .setPostedAtUnixMs(1_700_000_000_000L)
                    .setImportance(NotificationImportance.NOTIFICATION_IMPORTANCE_HIGH)
                    .setPrivacy(NotificationPrivacy.NOTIFICATION_PRIVACY_PRIVATE)
                    .setCategory(NotificationCategory.NOTIFICATION_CATEGORY_MESSAGE)
                    .setProgress(
                        Progress.newBuilder().setCurrent(3).setMax(7).setIndeterminate(false),
                    )
                    .setOngoing(true)
                    .setDismissible(false)
                    .setGroupId(sequential(8))
                    .setGroupSummary(true)
                    .setSecondaryProfile(true)
                    .setRedacted(true)
                    .setContentHash(sequential(32)),
            )
            .build()

    private fun hex(value: ByteArray): String =
        value.joinToString("") { "%02x".format(it) }

    private fun unhex(value: String): ByteArray =
        ByteArray(value.length / 2) {
            value.substring(it * 2, it * 2 + 2).toInt(16).toByte()
        }

    @Test
    fun `encodes the shared cross-language vector`() {
        assertEquals(canonicalUpsertHex, hex(canonicalUpsert().toByteArray()))
    }

    @Test
    fun `decodes the shared cross-language vector`() {
        val decoded = NotificationControl.parseFrom(unhex(canonicalUpsertHex))
        assertEquals(canonicalUpsert(), decoded)
        assertEquals(
            NotificationControl.BodyCase.UPSERT,
            decoded.bodyCase,
        )
        assertEquals("example.fixture.app", decoded.upsert.appId)
    }

    // -- round trips ---------------------------------------------------------

    @Test
    fun `every body round trips`() {
        val bodies = listOf(
            NotificationControl.newBuilder().setRoles(
                NotificationRoles.newBuilder()
                    .addRoles(NotificationRole.NOTIFICATION_ROLE_SOURCE)
                    .addRoles(NotificationRole.NOTIFICATION_ROLE_DISMISS_TARGET)
                    .setEpoch(1),
            ).build(),
            canonicalUpsert(),
            NotificationControl.newBuilder().setRemove(
                NotificationRemove.newBuilder()
                    .setNotificationId(repeated(0xa1, 16))
                    .setOriginDeviceId(syntheticDevice),
            ).build(),
            NotificationControl.newBuilder().setDismiss(
                DismissRequest.newBuilder()
                    .setNotificationId(repeated(0xa1, 16))
                    .setOriginDeviceId(syntheticDevice),
            ).build(),
            NotificationControl.newBuilder().setResult(
                NotificationResult.newBuilder()
                    .setNotificationId(repeated(0xa1, 16))
                    .setOutcome(NotificationOutcome.NOTIFICATION_OUTCOME_DISPLAYED),
            ).build(),
            NotificationControl.newBuilder().setSync(
                SyncMarker.newBuilder()
                    .setSyncId(repeated(0x5c, 16))
                    .setPhase(SyncMarker.Phase.PHASE_BEGIN),
            ).build(),
        )

        for (original in bodies) {
            val decoded = NotificationControl.parseFrom(original.toByteArray())
            assertEquals(original, decoded)
            // Well under MAX_FRAME_LEN, and under the capability's own 8 KiB
            // ceiling, which is what keeps the headroom the design reserves.
            assertTrue(original.toByteArray().size <= 8 * 1024)
        }
    }

    // -- identity ------------------------------------------------------------

    @Test
    fun `the notification id is exactly sixteen bytes`() {
        assertEquals(16, canonicalUpsert().upsert.notificationId.size())
    }

    /**
     * Posted and Updated are one message: the same identity carrying different
     * content is the whole update mechanism, on every platform OmniBridge targets.
     */
    @Test
    fun `an update keeps the identity and changes the content`() {
        val first = canonicalUpsert().upsert
        val second = first.toBuilder().setTitle("FIXTURE TITLE, EDITED").build()

        assertEquals(first.notificationId, second.notificationId)
        assertNotEquals(first.title, second.title)
    }

    /** Content is never identity: identical text is still two notifications. */
    @Test
    fun `identity is independent of content`() {
        val a = canonicalUpsert().upsert.toBuilder()
            .setNotificationId(repeated(0xd4, 16)).build()
        val b = canonicalUpsert().upsert.toBuilder()
            .setNotificationId(repeated(0xe5, 16)).build()

        assertEquals(a.title, b.title)
        assertEquals(a.body, b.body)
        assertNotEquals(a.notificationId, b.notificationId)
    }

    // -- roles ---------------------------------------------------------------

    /**
     * Absent roles mean no roles. A default-constructed announcement claims
     * nothing and carries epoch 0, which is "unset" and is refused — so a peer
     * cannot acquire a role by sending an empty message.
     */
    @Test
    fun `a default roles message claims nothing`() {
        val roles = NotificationRoles.getDefaultInstance()
        assertEquals(0, roles.rolesCount)
        assertEquals(0, roles.epoch)
    }

    /** The announcement is the complete set, so narrowing is not naming. */
    @Test
    fun `an empty announcement with a real epoch is how revocation is expressed`() {
        val revoked = NotificationRoles.newBuilder().setEpoch(2).build()
        assertEquals(0, revoked.rolesCount)
        assertEquals(2, revoked.epoch)

        val decoded = NotificationRoles.parseFrom(revoked.toByteArray())
        assertEquals(revoked, decoded)
        assertFalse(
            decoded.rolesList.contains(NotificationRole.NOTIFICATION_ROLE_SOURCE),
        )
    }

    /**
     * A future role arrives as `UNRECOGNIZED` and must never be treated as a
     * granted one. The rest of the announcement still applies: one unknown
     * value must not discard a set.
     */
    @Test
    fun `an unknown role is unrecognized and never granted`() {
        val wire = NotificationRoles.newBuilder()
            .addRoles(NotificationRole.NOTIFICATION_ROLE_SINK)
            .setEpoch(1)
            .build()
            .toByteArray()
        // Append field 1 (varint) with an unknown role value 4242.
        val withUnknown = wire + byteArrayOf(0x08, 0x92.toByte(), 0x21)

        val decoded = NotificationRoles.parseFrom(withUnknown)
        assertEquals(2, decoded.rolesCount)
        assertTrue(decoded.rolesList.contains(NotificationRole.NOTIFICATION_ROLE_SINK))
        assertTrue(decoded.rolesList.contains(NotificationRole.UNRECOGNIZED))
        assertEquals(4242, decoded.getRolesValue(1))
    }

    // -- conservative defaults -----------------------------------------------

    /**
     * An unknown enum value must resolve to the most conservative option, not
     * to the most permissive one. A privacy control that fails open is not a
     * control.
     */
    @Test
    fun `an unknown privacy value decodes as unrecognized rather than public`() {
        val wire = NotificationUpsert.newBuilder()
            .setNotificationId(sequential(16))
            .build()
            .toByteArray() + byteArrayOf(0x48, 0x7f) // field 9, varint 127

        val decoded = NotificationUpsert.parseFrom(wire)
        assertEquals(NotificationPrivacy.UNRECOGNIZED, decoded.privacy)
        assertNotEquals(NotificationPrivacy.NOTIFICATION_PRIVACY_PUBLIC, decoded.privacy)
        assertEquals(127, decoded.privacyValue)
    }

    /**
     * Android `IMPORTANCE_NONE` has no wire value: a notification the phone
     * does not show its own owner must not become a desktop banner.
     */
    @Test
    fun `there is no wire value for importance none`() {
        assertEquals(3, NotificationImportance.NOTIFICATION_IMPORTANCE_HIGH.number)
        assertEquals(
            NotificationImportance.UNRECOGNIZED,
            NotificationImportance.forNumber(4) ?: NotificationImportance.UNRECOGNIZED,
        )
    }

    // -- forward compatibility -----------------------------------------------

    /**
     * A message from a hypothetical richer implementation must not fail to
     * decode. Unknown fields are ignored, which is what lets an older peer
     * interoperate with a newer one instead of dropping the session.
     */
    @Test
    fun `an unknown field does not break decoding`() {
        // Field 999, varint 1, appended to a valid dismiss request.
        val dismiss = DismissRequest.newBuilder()
            .setNotificationId(repeated(0x31, 16))
            .setOriginDeviceId(syntheticDevice)
            .build()
        val withUnknown = dismiss.toByteArray() + byteArrayOf(0xB8.toByte(), 0x3E, 0x01)

        val decoded = DismissRequest.parseFrom(withUnknown)
        assertEquals(dismiss.notificationId, decoded.notificationId)
        assertEquals(dismiss.originDeviceId, decoded.originDeviceId)
    }

    /**
     * An unknown `oneof` body decodes to `BODY_NOT_SET` rather than throwing.
     * A future seventh body must be ignorable by a v1 peer.
     */
    @Test
    fun `an unknown control body is ignored not fatal`() {
        // Field 42, length-delimited, empty.
        val decoded = NotificationControl.parseFrom(byteArrayOf(0xD2.toByte(), 0x02, 0x00))
        assertEquals(NotificationControl.BodyCase.BODY_NOT_SET, decoded.bodyCase)
    }

    // -- the prohibited surface ----------------------------------------------

    /**
     * The dismissal primitive carries exactly two fields: which notification,
     * and whose. There is no action index, no intent, no payload and no reply
     * text — and no field that could be widened into one.
     *
     * The structural version of this check lives in `omnibridge-proto`'s
     * `notifications_schema` test, which asserts against the compiled
     * descriptors. The `lite` runtime used here has no descriptors, so this
     * is the behavioural half: a re-encode of a decoded message is
     * byte-identical, so nothing was carried in a field this build knows
     * nothing about.
     */
    @Test
    fun `the dismiss primitive has no remote execution path`() {
        val dismiss = DismissRequest.newBuilder()
            .setNotificationId(repeated(0x31, 16))
            .setOriginDeviceId(syntheticDevice)
            .build()

        val encoded = dismiss.toByteArray()
        val decoded = DismissRequest.parseFrom(encoded)
        assertArrayEquals(encoded, decoded.toByteArray())
        assertEquals(16, decoded.notificationId.size())
        assertEquals(syntheticDevice, decoded.originDeviceId)
        assertEquals(
            "a DismissRequest is two fields and no more",
            2 + 16 + 2 + syntheticDevice.length,
            encoded.size,
        )
    }

    /**
     * The capability envelope has six bodies and no seventh. A generic escape
     * hatch here would be the place a serialized platform notification, an
     * action or a reply would eventually arrive.
     */
    @Test
    fun `the control envelope has exactly six bodies`() {
        val cases = NotificationControl.BodyCase.values().map { it.name }.toSet()
        assertEquals(
            setOf("ROLES", "UPSERT", "REMOVE", "DISMISS", "RESULT", "SYNC", "BODY_NOT_SET"),
            cases,
        )
    }

    // -- snapshot framing ----------------------------------------------------

    @Test
    fun `a sync marker brackets a snapshot`() {
        val syncId = repeated(0x5c, 16)
        val begin = SyncMarker.newBuilder()
            .setSyncId(syncId).setPhase(SyncMarker.Phase.PHASE_BEGIN).build()
        val end = SyncMarker.newBuilder()
            .setSyncId(syncId).setPhase(SyncMarker.Phase.PHASE_END).build()

        assertEquals(16, begin.syncId.size())
        assertEquals(begin.syncId, end.syncId)
        assertNotEquals(begin.phase, end.phase)
        assertEquals(begin, SyncMarker.parseFrom(begin.toByteArray()))
    }

    // -- capability id -------------------------------------------------------

    /**
     * N0 defined the id and registered nobody; N1 registers the Android source
     * adapter, so `notifications.v1` now appears in `HELLO` and can be
     * negotiated.
     *
     * That is deliberately not conditional on the current Android permission
     * state: roles exist precisely so a capability can be supported while
     * being unable to do anything right now, and making the handshake depend
     * on a permission the user can toggle at 14:32 would mean a reconnect were
     * needed to pick up a grant made in Settings (ADR-0017).
     *
     * **Support is not permission.** Negotiating the capability sends nothing:
     * the peer grant, the OS notification access and the announced roles are
     * three further, independent gates.
     */
    @Test
    fun `notifications v1 is advertised once the source adapter is registered`() {
        val registry = CapabilityRegistry(listOf(NotificationsCapability(source())))

        assertTrue(registry.supports("notifications.v1"))
        assertTrue(registry.advertised().contains("notifications.v1"))
        assertEquals(
            listOf("notifications.v1"),
            registry.negotiate(listOf("battery.v1", "notifications.v1")),
        )
    }

    /**
     * And a peer that does not implement it never negotiates it, so a desktop
     * without the sink — every desktop until N2 — is untouched.
     */
    @Test
    fun `a peer that does not implement it never negotiates it`() {
        val registry = CapabilityRegistry(listOf(NotificationsCapability(source())))
        assertTrue(
            registry.negotiate(listOf("battery.v1", "clipboard.v1", "files.v1")).isEmpty(),
        )
    }

    /** A registry without the adapter still claims nothing. */
    @Test
    fun `an empty registry advertises nothing`() {
        val registry = CapabilityRegistry(emptyList())
        assertFalse(registry.supports("notifications.v1"))
        assertFalse(registry.advertised().contains("notifications.v1"))
        assertTrue(registry.negotiate(listOf("notifications.v1")).isEmpty())
    }

    /**
     * A source with every platform seam stubbed out. Enough to register: this
     * file is about the schema and the capability id, not about behaviour.
     */
    private fun source() = NotificationSource(
        ownPackage = "io.github.yurisismotto.omnibridge",
        localDeviceId = syntheticDevice,
        authorizer = { NotificationPolicy.DENIED },
        appLabels = { it },
        secretProvider = { null },
        lockState = LockState.ALWAYS_LOCKED,
        access = object : NotificationSource.AccessControl {
            override fun isAccessGranted() = false
            override fun requestBind() = Unit
        },
    )

    /** Existing capability negotiation is untouched by the new id. */
    @Test
    fun `existing capability negotiation is unchanged`() {
        val registry = CapabilityRegistry(emptyList())
        assertTrue(
            registry.negotiate(
                listOf("battery.v1", "clipboard.v1", "files.v1", "notifications.v1"),
            ).isEmpty(),
        )
    }
}
