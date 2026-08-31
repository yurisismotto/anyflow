package io.github.yurisismotto.anyflow.clipboard

/**
 * The two bounded caches that make clipboard sync converge instead of loop.
 *
 * Neither stores clipboard content. They hold ids, hashes and monotonic
 * timestamps, which is the whole reason "no history, nothing persisted" and
 * "no sync storms" are compatible requirements.
 *
 * Kotlin twin of `desktop/capabilities/clipboard/src/dedup.rs`; the two must
 * behave the same, because a loop only needs one end to get it wrong.
 *
 * ## The clock
 *
 * Both use [android.os.SystemClock.elapsedRealtime] via [Clock], never
 * `System.currentTimeMillis`. Expiry must not be steerable by a clock change
 * or by the user setting the date, and a monotonic clock is also the only one
 * that keeps counting while the device sleeps.
 */
fun interface Clock {
    fun nowMillis(): Long
}

/**
 * Remembers which clipboard events have already been handled.
 *
 * Bounded twice over — by entry count and by age — so neither a quiet session
 * nor a flooding peer can grow it without limit.
 *
 * Not thread-safe by itself: [ClipboardSync] owns one and serialises access.
 */
class EventCache(
    private val capacity: Int = ClipboardLimits.EVENT_CACHE_ENTRIES,
    private val ttlMillis: Long = ClipboardLimits.EVENT_CACHE_TTL_MS,
    private val clock: Clock,
) {
    private data class Entry(val id: ByteArray, val at: Long) {
        override fun equals(other: Any?): Boolean =
            other is Entry && id.contentEquals(other.id) && at == other.at

        override fun hashCode(): Int = id.contentHashCode() * 31 + at.hashCode()
    }

    private val entries = ArrayDeque<Entry>()

    /**
     * Records [eventId] as handled.
     *
     * Returns true if it is new, false if it was already known — in which
     * case the caller must treat the update as a duplicate and do nothing
     * else with it.
     */
    fun admit(eventId: ByteArray): Boolean {
        expire()
        if (entries.any { it.id.contentEquals(eventId) }) return false
        entries.addLast(Entry(eventId.copyOf(), clock.nowMillis()))
        while (entries.size > capacity) entries.removeFirst()
        return true
    }

    fun contains(eventId: ByteArray): Boolean {
        val now = clock.nowMillis()
        return entries.any { it.id.contentEquals(eventId) && now - it.at < ttlMillis }
    }

    fun size(): Int {
        expire()
        return entries.size
    }

    private fun expire() {
        val now = clock.nowMillis()
        // Entries are appended in time order, so the first live one means
        // every later one is live too.
        while (entries.isNotEmpty() && now - entries.first().at >= ttlMillis) {
            entries.removeFirst()
        }
    }
}

/**
 * Marks content this device wrote to its own clipboard *because a computer
 * sent it*, so the resulting local change is not echoed back.
 *
 * ## Single-use, on purpose
 *
 * [take] removes the entry it matches. The consequences are what make this
 * different from a naive `newText == oldText` comparison:
 *
 *  * a remote clip applied locally suppresses **exactly one** subsequent
 *    observation — the one it caused;
 *  * copying that same text again by hand afterwards is a new local event and
 *    is sent normally;
 *  * an entry whose local change is never observed expires on its own rather
 *    than sitting there swallowing a later copy.
 */
class SuppressionCache(
    private val capacity: Int = ClipboardLimits.SUPPRESSION_ENTRIES,
    private val ttlMillis: Long = ClipboardLimits.SUPPRESSION_TTL_MS,
    private val clock: Clock,
) {
    private class Entry(val hash: ByteArray, val at: Long, val originDeviceId: String)

    private val entries = ArrayDeque<Entry>()

    /**
     * Records that [hash] is about to be written locally on behalf of
     * [originDeviceId], and must not be sent back out.
     */
    fun arm(hash: ByteArray, originDeviceId: String) {
        expire()
        entries.addLast(Entry(hash.copyOf(), clock.nowMillis(), originDeviceId))
        while (entries.size > capacity) entries.removeFirst()
    }

    /**
     * Consumes a suppression entry for [hash], if one is live.
     *
     * A non-null result means this local clipboard change was caused by that
     * computer and must not produce an outbound update. Null means it is a
     * genuine local copy.
     */
    fun take(hash: ByteArray): String? {
        expire()
        val index = entries.indexOfFirst { it.hash.contentEquals(hash) }
        if (index < 0) return null
        val entry = entries.removeAt(index)
        return entry.originDeviceId
    }

    fun size(): Int {
        expire()
        return entries.size
    }

    private fun expire() {
        val now = clock.nowMillis()
        while (entries.isNotEmpty() && now - entries.first().at >= ttlMillis) {
            entries.removeFirst()
        }
    }
}
