package io.github.yurisismotto.omnibridge

import io.github.yurisismotto.omnibridge.clipboard.Clock
import io.github.yurisismotto.omnibridge.clipboard.ClipboardText
import io.github.yurisismotto.omnibridge.clipboard.EventCache
import io.github.yurisismotto.omnibridge.clipboard.SuppressionCache
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The two bounded caches, and the loop they exist to prevent.
 *
 * Mirrors `desktop/capabilities/clipboard/src/dedup.rs`. Both ends need the
 * same behaviour: a loop only needs one of them to get it wrong.
 */
class ClipboardCachesTest {

    /** A clock a test can move by hand. Real time would make these slow. */
    private class FakeClock(var now: Long = 0) : Clock {
        override fun nowMillis(): Long = now
    }

    private fun id(seed: Byte) = ByteArray(16) { seed }

    // -----------------------------------------------------------------------
    // EventCache
    // -----------------------------------------------------------------------

    @Test
    fun `a new event is admitted once`() {
        val cache = EventCache(clock = FakeClock())
        assertTrue("first sighting is new", cache.admit(id(1)))
        assertFalse("the same id must not be admitted twice", cache.admit(id(1)))
    }

    @Test
    fun `distinct events with equal content are both admitted`() {
        // The property a content comparison gets wrong: two clipboard events
        // may legitimately carry the same text.
        val cache = EventCache(clock = FakeClock())
        assertTrue(cache.admit(id(1)))
        assertTrue(cache.admit(id(2)))
    }

    @Test
    fun `events expire by age`() {
        val clock = FakeClock()
        val cache = EventCache(capacity = 1024, ttlMillis = 60_000, clock = clock)
        assertTrue(cache.admit(id(1)))
        assertTrue(cache.contains(id(1)))

        clock.now += 61_000

        assertFalse("should have expired", cache.contains(id(1)))
        assertEquals("expired entries must be dropped, not kept", 0, cache.size())
        assertTrue("and the id is free again", cache.admit(id(1)))
    }

    @Test
    fun `the event cache is bounded by count`() {
        val cache = EventCache(capacity = 8, ttlMillis = 3_600_000, clock = FakeClock())
        for (i in 0 until 1000) {
            val distinct = ByteArray(16)
            distinct[0] = (i and 0xff).toByte()
            distinct[1] = ((i shr 8) and 0xff).toByte()
            assertTrue(cache.admit(distinct))
        }
        assertEquals("the cache must not grow without bound", 8, cache.size())
    }

    // -----------------------------------------------------------------------
    // SuppressionCache
    // -----------------------------------------------------------------------

    @Test
    fun `suppression is single use`() {
        val cache = SuppressionCache(clock = FakeClock())
        val hash = ClipboardText.contentHash("shared text")

        cache.arm(hash, "desktop-1")
        assertEquals(
            "the echo of the remote write is suppressed",
            "desktop-1",
            cache.take(hash),
        )
        assertNull(
            "a later copy of the same text is a real local event",
            cache.take(hash),
        )
    }

    @Test
    fun `a suppression entry expires rather than swallowing a later copy`() {
        val clock = FakeClock()
        val cache = SuppressionCache(capacity = 64, ttlMillis = 10_000, clock = clock)
        val hash = ClipboardText.contentHash("re-copied later")
        cache.arm(hash, "phone-1")

        // The local change was never observed — the platform coalesced it.
        clock.now += 11_000

        assertNull("a stale entry must not suppress a genuine copy", cache.take(hash))
        assertEquals(0, cache.size())
    }

    @Test
    fun `the suppression cache is bounded`() {
        val cache = SuppressionCache(capacity = 4, ttlMillis = 3_600_000, clock = FakeClock())
        for (i in 0 until 500) {
            cache.arm(ClipboardText.contentHash("text $i"), "peer")
        }
        assertEquals(4, cache.size())
    }

    @Test
    fun `suppression matches content not order`() {
        val cache = SuppressionCache(clock = FakeClock())
        val a = ClipboardText.contentHash("first")
        val b = ClipboardText.contentHash("second")
        cache.arm(a, "peer-a")
        cache.arm(b, "peer-b")

        // The clipboard settled on the second write; the first entry is still
        // live and must not be consumed by it.
        assertEquals("peer-b", cache.take(b))
        assertEquals("peer-a", cache.take(a))
    }
}
