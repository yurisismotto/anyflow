package io.github.yurisismotto.omnibridge.notifications

/**
 * One pending listener-cancel, so a dismissal does not echo back to the peer
 * that asked for it.
 *
 * ## The loop this exists to break
 *
 * ```text
 *   a human closes the mirror on the desktop
 *         │
 *         ▼  DismissRequest
 *   this phone: cancelNotification(key)
 *         │
 *         ▼  onNotificationRemoved(REASON_LISTENER_CANCEL)
 *   this phone: NotificationRemove … to every granted peer
 *         │
 *         ▼  the desktop receives a removal for a mirror it purged
 *            before it ever sent the request
 * ```
 *
 * On GNOME the last step is a wasted D-Bus call, because closing an unknown id
 * is a silent no-op there (HOST VERIFIED in N2). On a spec-literal server —
 * dunst, mako — it is a D-Bus **error**, on every dismissal, for ever. Neither
 * is a correctness failure, because the desktop converges either way, but a
 * design that produces an error on its own happy path is one nobody will trust
 * the logs of.
 *
 * So the removal is sent to every granted peer **except the one that asked**,
 * and this is what remembers which one that was.
 *
 * ## Why it is not the loop-safety mechanism
 *
 * It is worth being exact, because this class is easy to over-read. The reason
 * the sequence above cannot become a loop is that **the desktop purges its
 * mirror before it sends the request**, so an inbound removal for it finds
 * nothing and answers `UNKNOWN_NOTIFICATION` — an answer, not an error, and
 * not something that produces another message. Echo suppression removes a
 * redundant message; it is not what makes the second lap impossible.
 *
 * That matters because it bounds what a bug here can cost. If an entry expires
 * early, the desktop gets a removal it converges on. If one never arrives, the
 * entry expires and nothing is swallowed. Neither can dismiss anything, loop,
 * or lose a genuine removal — the last of which is the only failure that would
 * strand a notification on a screen for ever.
 *
 * ## Single use, and short lived
 *
 * The same discipline the clipboard's suppression cache follows:
 *
 *  * **single use** — the first matching removal consumes the entry. An app
 *    that re-posts under the same key immediately after a cancel would
 *    otherwise have its *next*, genuine removal swallowed, leaving a mirror on
 *    a desktop that nothing can take off;
 *  * **expiring** — an entry whose callback never arrives is gone in
 *    [TTL_MS] rather than lingering to swallow something later;
 *  * **bounded** — [CAPACITY] entries, oldest evicted, so a device cancelling
 *    in a storm cannot grow the heap;
 *  * **monotonic** — `SystemClock.elapsedRealtime` is injected as a plain
 *    `() -> Long`, so a clock change cannot steer expiry and the whole class
 *    is a JVM test.
 *
 * ## What it holds
 *
 * A derived notification id's hex, a peer fingerprint's hex and a timestamp.
 * **No platform key, no title, no body**, and no field that could carry one.
 * It is memory only and dies with the process.
 *
 * Not thread-safe: the ordered producer owns one and serialises access, which
 * is also what guarantees an entry is armed strictly before the cancel that
 * would fire the callback it is waiting for.
 */
class EchoSuppression(
    private val capacity: Int = CAPACITY,
    private val ttlMs: Long = TTL_MS,
    private val now: () -> Long,
) {

    private class Entry(val peerHex: String, val armedAtMs: Long)

    /** Keyed by the id's hex, because a `ByteArray` hashes by reference. */
    private val pending = LinkedHashMap<String, Entry>()

    /**
     * Records that [peerHex] asked for this notification to be cancelled.
     *
     * **Call this before `cancelNotification`, never after.** The platform
     * callback can arrive on the main thread while the cancel call is still
     * returning, and an entry armed afterwards would lose the race it exists
     * to win.
     */
    fun arm(idHex: String, peerHex: String) {
        expire()
        // Re-arming refreshes the position, so the oldest evicted is the least
        // recently armed rather than an arbitrary one.
        pending.remove(idHex)
        pending[idHex] = Entry(peerHex, now())
        while (pending.size > capacity) {
            pending.remove(pending.keys.first())
        }
    }

    /**
     * The peer whose dismissal caused this removal, consuming the entry.
     *
     * `null` when nothing is pending for this id — the ordinary case, because
     * almost every removal is a person swiping a notification away on the
     * phone itself, and that one must reach every peer.
     */
    fun consume(idHex: String): String? {
        expire()
        return pending.remove(idHex)?.peerHex
    }

    /**
     * Releases an entry without consuming it.
     *
     * Called when the cancel itself failed: no callback is coming, so holding
     * the entry could only swallow a later, genuine removal.
     */
    fun release(idHex: String) {
        pending.remove(idHex)
    }

    /** Everything pending goes. A lifecycle change invalidates all of it. */
    fun clear() {
        pending.clear()
    }

    fun size(): Int {
        expire()
        return pending.size
    }

    private fun expire() {
        val deadline = now() - ttlMs
        val iterator = pending.entries.iterator()
        while (iterator.hasNext()) {
            // Insertion-ordered, so the first entry still inside the window
            // ends the sweep.
            if (iterator.next().value.armedAtMs > deadline) break
            iterator.remove()
        }
    }

    /** Counts only. Never renders an id or a fingerprint. */
    override fun toString(): String = "EchoSuppression(pending=${pending.size})"

    companion object {
        /**
         * How many cancels may be in flight at once.
         *
         * A person clearing a desktop's notification list dismisses at most a
         * screenful; sixty-four is well past that and small enough that the
         * bound is obviously not a memory concern.
         */
        const val CAPACITY = 64

        /**
         * How long an entry waits for the callback it was armed for.
         *
         * Ten seconds: long enough that a busy main thread cannot miss the
         * window, short enough that a cancel whose callback never arrives
         * cannot swallow a genuine removal minutes later.
         */
        const val TTL_MS = 10_000L
    }
}
