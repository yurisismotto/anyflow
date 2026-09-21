package io.github.yurisismotto.omnibridge

import io.github.yurisismotto.omnibridge.notifications.NotificationIdentity
import io.github.yurisismotto.omnibridge.notifications.NotificationLimits
import io.github.yurisismotto.omnibridge.notifications.SourceIdMap
import javax.crypto.Mac
import javax.crypto.spec.SecretKeySpec
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * ADR-0016 — what a notification is called on the wire.
 *
 * The vectors below are the cross-language contract. `desktop/core/tests/
 * notifications_protocol.rs` computes the same three values from the same
 * inputs and asserts the same bytes, so a typo in a domain string, a
 * length-prefix that stopped being big-endian, or a truncation that moved from
 * 16 bytes to 20 fails a test here or there rather than producing two devices
 * that quietly disagree about which mirror is which.
 *
 * Every fixture is obviously synthetic and none of it reads like a real
 * notification.
 */
class NotificationIdentityTest {

    /**
     * The shape of a real `StatusBarNotification.key`: `userId|pkg|id|tag|uid`.
     * Used as an opaque string — nothing here parses it, and nothing anywhere
     * transmits it.
     */
    private val platformKey = "0|example.fixture.app|1|null|10123"
    private val otherKey = "0|example.other.app|1|null|10124"

    /** 32 bytes, obviously not from a CSPRNG, and therefore obviously a test. */
    private val secret = ByteArray(32) { it.toByte() }
    private val otherSecret = ByteArray(32) { ((it + 1) % 256).toByte() }

    private fun mac(key: ByteArray): Mac =
        Mac.getInstance("HmacSHA256").apply { init(SecretKeySpec(key, "HmacSHA256")) }

    private fun hex(bytes: ByteArray) = bytes.joinToString("") { "%02x".format(it) }

    // -- the pinned vectors --------------------------------------------------

    /**
     * The exact bytes ADR-0016's derivation produces.
     *
     * ```text
     * HMAC-SHA256(secret,
     *   "omnibridge/notifications.v1/id/v1" || len32(key) || key)[0..16]
     * ```
     */
    @Test
    fun `the notification id vector`() {
        assertEquals(
            "3c8effce6feb1582a65e100aeea1a810",
            hex(NotificationIdentity.derive(mac(secret), platformKey)),
        )
    }

    @Test
    fun `the group id vector`() {
        assertEquals(
            "b9f8d940e134bccb",
            hex(NotificationIdentity.groupId("0|example.fixture.app|g:chat")),
        )
    }

    /** Big-endian, four bytes, always — the pairing proof's own convention. */
    @Test
    fun `len32 is a big-endian uint32`() {
        assertEquals("00000000", hex(NotificationIdentity.len32(0)))
        assertEquals("00000001", hex(NotificationIdentity.len32(1)))
        assertEquals("0000ff00", hex(NotificationIdentity.len32(0xff00)))
        assertEquals("12345678", hex(NotificationIdentity.len32(0x12345678)))
    }

    /** The domain strings are part of the contract, not decoration. */
    @Test
    fun `the domain strings are exactly the approved ones`() {
        assertEquals("omnibridge/notifications.v1/id/v1", NotificationIdentity.ID_DOMAIN)
        assertEquals("omnibridge/notifications.v1/group/v1", NotificationIdentity.GROUP_DOMAIN)
        assertEquals("omnibridge/notifications.v1/content/v1", NotificationIdentity.CONTENT_DOMAIN)
    }

    // -- the properties the design depends on --------------------------------

    @Test
    fun `an id is exactly sixteen bytes`() {
        assertEquals(
            NotificationLimits.NOTIFICATION_ID_LENGTH,
            NotificationIdentity.derive(mac(secret), platformKey).size,
        )
        assertEquals(16, NotificationLimits.NOTIFICATION_ID_LENGTH)
    }

    /**
     * The same key under the same secret is the same notification. This is
     * what makes an update replace its predecessor instead of stacking beside
     * it, and what makes a reconnect reconcile instead of duplicating.
     */
    @Test
    fun `the same key and secret derive the same id`() {
        assertArrayEquals(
            NotificationIdentity.derive(mac(secret), platformKey),
            NotificationIdentity.derive(mac(secret), platformKey),
        )
    }

    @Test
    fun `a different key derives a different id`() {
        assertEquals(
            "3561179daf1a26a8a047f3f44758eaf3",
            hex(NotificationIdentity.derive(mac(secret), otherKey)),
        )
        assertNotEquals(
            hex(NotificationIdentity.derive(mac(secret), platformKey)),
            hex(NotificationIdentity.derive(mac(secret), otherKey)),
        )
    }

    /**
     * A new secret is a new id space. That is what makes an identity reset a
     * mirror reset: a peer cannot link ids across a re-pair.
     */
    @Test
    fun `a different secret derives a different id`() {
        assertEquals(
            "e88bcb16b74b498c91090bc76fc3b2d9",
            hex(NotificationIdentity.derive(mac(otherSecret), platformKey)),
        )
        assertNotEquals(
            hex(NotificationIdentity.derive(mac(secret), platformKey)),
            hex(NotificationIdentity.derive(mac(otherSecret), platformKey)),
        )
    }

    /**
     * The length prefix is why two keys cannot be concatenated into a third.
     * Without it `"ab" + "c"` and `"a" + "bc"` would hash identically, and two
     * unrelated notifications would share a name.
     */
    @Test
    fun `the length prefix makes concatenation unambiguous`() {
        assertNotEquals(
            hex(NotificationIdentity.derive(mac(secret), "ab|c")),
            hex(NotificationIdentity.derive(mac(secret), "a|bc")),
        )
    }

    /**
     * **Content is never identity.** The derivation takes the platform key and
     * nothing else, so an app that edits a message in place keeps one mirror
     * and two distinct alerts that read the same stay two notifications.
     *
     * Asserted structurally: the only inputs the function has are a `Mac` and
     * the key, so there is no parameter through which a title could reach it.
     */
    @Test
    fun `the derivation cannot depend on title or body`() {
        val method = NotificationIdentity::class.java.methods.first { it.name == "derive" }
        assertEquals(2, method.parameterCount)
        assertEquals(Mac::class.java, method.parameterTypes[0])
        assertEquals(String::class.java, method.parameterTypes[1])
    }

    // -- content hash --------------------------------------------------------

    private fun hashOf(
        title: String = "FIXTURE TITLE",
        body: String = "FIXTURE BODY",
        redacted: Boolean = false,
    ) = NotificationIdentity.contentHash(
        appId = "example.fixture.app",
        appLabel = "Fixture App",
        title = title,
        body = body,
        importance = 2,
        privacy = 2,
        category = 1,
        flags = NotificationIdentity.flags(
            ongoing = false,
            dismissible = true,
            groupSummary = false,
            secondaryProfile = false,
            redacted = redacted,
        ),
        progress = ByteArray(0),
        groupId = ByteArray(0),
    )

    @Test
    fun `a content hash is exactly thirty-two bytes and is deterministic`() {
        assertEquals(NotificationLimits.CONTENT_HASH_LENGTH, hashOf().size)
        assertArrayEquals(hashOf(), hashOf())
    }

    @Test
    fun `changing any semantic field changes the content hash`() {
        val base = hex(hashOf())
        assertNotEquals(base, hex(hashOf(title = "FIXTURE TITLE, EDITED")))
        assertNotEquals(base, hex(hashOf(body = "FIXTURE BODY, EDITED")))
        // A reduction is a semantic change: the same app name with and without
        // its body must not collapse to one digest, or a phone that locked
        // mid-conversation would look like a duplicate and never be sent.
        assertNotEquals(hex(hashOf(title = "", body = "")), hex(hashOf(title = "", body = "", redacted = true)))
    }

    /**
     * The flags word packs one bit per boolean, so no two combinations share a
     * value.
     */
    @Test
    fun `every flag combination is distinct`() {
        val seen = mutableSetOf<Int>()
        for (bits in 0 until 32) {
            seen += NotificationIdentity.flags(
                ongoing = bits and 1 != 0,
                dismissible = bits and 2 != 0,
                groupSummary = bits and 4 != 0,
                secondaryProfile = bits and 8 != 0,
                redacted = bits and 16 != 0,
            )
        }
        assertEquals(32, seen.size)
    }

    // -- the source-side map -------------------------------------------------

    @Test
    fun `the id map maps both ways and forgets on removal`() {
        val map = SourceIdMap()
        val id = NotificationIdentity.derive(mac(secret), platformKey)

        map.remember(id, platformKey)
        assertEquals(platformKey, map.platformKey(id))
        assertArrayEquals(id, map.notificationId(platformKey))

        map.forgetKey(platformKey)
        assertNull(map.platformKey(id))
        assertNull(map.notificationId(platformKey))
        assertEquals(0, map.size())
    }

    /** A re-post keeps one entry: the identity did not change. */
    @Test
    fun `remembering the same notification twice keeps one entry`() {
        val map = SourceIdMap()
        val id = NotificationIdentity.derive(mac(secret), platformKey)
        map.remember(id, platformKey)
        map.remember(id, platformKey)
        assertEquals(1, map.size())
    }

    @Test
    fun `the id map is bounded and evicts the oldest`() {
        val map = SourceIdMap(capacity = 4)
        val keys = (0 until 10).map { "0|example.fixture.app|$it|null|10123" }
        for (key in keys) {
            map.remember(NotificationIdentity.derive(mac(secret), key), key)
        }
        assertEquals(4, map.size())
        // The first six are gone; the last four survive.
        assertNull(map.notificationId(keys[0]))
        assertTrue(map.notificationId(keys[9]) != null)
    }

    @Test
    fun `retainOnly drops what the platform no longer lists`() {
        val map = SourceIdMap()
        val keys = (0 until 5).map { "0|example.fixture.app|$it|null|10123" }
        for (key in keys) {
            map.remember(NotificationIdentity.derive(mac(secret), key), key)
        }
        map.retainOnly(listOf(keys[1], keys[3]))
        assertEquals(2, map.size())
        assertNull(map.notificationId(keys[0]))
        assertTrue(map.notificationId(keys[1]) != null)
    }

    /** An identity reset clears it: the old ids can never be mapped again. */
    @Test
    fun `clear empties the map`() {
        val map = SourceIdMap()
        map.remember(NotificationIdentity.derive(mac(secret), platformKey), platformKey)
        map.clear()
        assertEquals(0, map.size())
    }

    /**
     * The map holds identities, never content. Asserted on the rendering the
     * logging audit actually depends on.
     */
    @Test
    fun `the id map never renders a key`() {
        val map = SourceIdMap()
        map.remember(NotificationIdentity.derive(mac(secret), platformKey), platformKey)
        val rendered = map.toString()
        assertTrue(rendered.contains("entries=1"))
        assertTrue("the map must not render a platform key", !rendered.contains("example"))
    }
}
