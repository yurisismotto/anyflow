package io.github.yurisismotto.anyflow

import io.github.yurisismotto.anyflow.notifications.NotificationEvent
import io.github.yurisismotto.anyflow.notifications.NotificationMapping
import io.github.yurisismotto.anyflow.notifications.NotificationOutboundQueue
import io.github.yurisismotto.anyflow.notifications.PlatformNotification
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The bounded hand-off between the phone's main thread and the ordered
 * producer.
 *
 * Two properties are being pinned, and they pull against each other:
 *
 *  * **it is bounded** — never `Channel.UNLIMITED`, because a notification
 *    storm on a phone with no connected peer would otherwise grow the heap
 *    until the process died;
 *  * **a removal is never lost** — losing one leaves a mirror on a desktop for
 *    ever, and a stale mirror of a banking alert is exactly the leak this
 *    capability must not create.
 *
 * Coalescing is what lets both be true at once: the flood that would fill the
 * queue is a progress bar updating sixty times a second, and that occupies one
 * slot rather than sixty.
 */
class NotificationQueueTest {

    private fun posted(key: String, postedAt: Long = 0) = NotificationEvent.Posted(
        PlatformNotification(
            platformKey = key,
            packageName = "example.fixture.app",
            secondaryProfile = false,
            postedAtUnixMs = postedAt,
            ongoing = false,
            clearable = true,
            visibility = NotificationMapping.VISIBILITY_PRIVATE,
            androidImportance = NotificationMapping.ANDROID_IMPORTANCE_DEFAULT,
            category = null,
            groupKey = null,
            groupSummary = false,
            title = "FIXTURE TITLE",
            body = "FIXTURE BODY",
            hasProgress = false,
            progressCurrent = 0,
            progressMax = 0,
            progressIndeterminate = false,
        ),
    )

    private fun keysOf(events: List<NotificationEvent>): List<String> = events.map {
        when (it) {
            is NotificationEvent.Posted -> "post:${it.notification.platformKey}"
            is NotificationEvent.Removed -> "remove:${it.platformKey}"
            else -> it.javaClass.simpleName
        }
    }

    // -- ordering ------------------------------------------------------------

    /**
     * The reason there is one queue and one producer: a removal must never be
     * applied before the upsert it refers to, or the sink shows a notification
     * the phone no longer has, permanently.
     */
    @Test
    fun `events drain in the order they were offered`() {
        val queue = NotificationOutboundQueue()
        queue.offer(posted("a"))
        queue.offer(posted("b"))
        queue.offer(NotificationEvent.Removed("a"))
        queue.offer(posted("c"))

        assertEquals(
            listOf("post:b", "remove:a", "post:c"),
            keysOf(queue.drain()),
        )
    }

    @Test
    fun `draining empties the queue`() {
        val queue = NotificationOutboundQueue()
        queue.offer(posted("a"))
        assertEquals(1, queue.size())
        queue.drain()
        assertEquals(0, queue.size())
        assertTrue(queue.drain().isEmpty())
    }

    // -- coalescing ----------------------------------------------------------

    /**
     * A per-identity slot, not a queue of stale states. A download bar
     * updating constantly produces one pending message, always the current
     * one — reading the state after the fact is better than queuing states
     * that are already wrong.
     */
    @Test
    fun `a newer post for the same notification replaces the pending one`() {
        val queue = NotificationOutboundQueue()
        queue.offer(posted("a", postedAt = 1))
        queue.offer(posted("a", postedAt = 2))
        queue.offer(posted("a", postedAt = 3))

        val drained = queue.drain()
        assertEquals(1, drained.size)
        assertEquals(3L, (drained[0] as NotificationEvent.Posted).notification.postedAtUnixMs)
        assertEquals(2, queue.coalesced)
    }

    /** Coalescing keeps the slot's position, so surrounding order survives. */
    @Test
    fun `coalescing does not reorder the queue`() {
        val queue = NotificationOutboundQueue()
        queue.offer(posted("a"))
        queue.offer(posted("b"))
        queue.offer(posted("a", postedAt = 99))

        assertEquals(listOf("post:a", "post:b"), keysOf(queue.drain()))
    }

    /**
     * A removal supersedes a pending post for the same notification: sending a
     * state and then removing it is two messages where none will do, and the
     * sink converges on the same answer.
     */
    @Test
    fun `a removal supersedes a pending post for the same notification`() {
        val queue = NotificationOutboundQueue()
        queue.offer(posted("a"))
        queue.offer(NotificationEvent.Removed("a"))

        assertEquals(listOf("remove:a"), keysOf(queue.drain()))
    }

    @Test
    fun `a removal does not touch another notification's pending post`() {
        val queue = NotificationOutboundQueue()
        queue.offer(posted("a"))
        queue.offer(NotificationEvent.Removed("b"))

        assertEquals(listOf("post:a", "remove:b"), keysOf(queue.drain()))
    }

    // -- bounds --------------------------------------------------------------

    @Test
    fun `the queue is bounded`() {
        val queue = NotificationOutboundQueue(capacity = 4)
        for (index in 0 until 100) {
            queue.offer(posted("key-$index"))
        }
        assertEquals(4, queue.size())
        assertTrue(queue.droppedNonTerminal > 0)
    }

    /** Under pressure it is the oldest state that goes, not the newest truth. */
    @Test
    fun `overflow evicts the oldest non-terminal event`() {
        val queue = NotificationOutboundQueue(capacity = 3)
        queue.offer(posted("a"))
        queue.offer(posted("b"))
        queue.offer(posted("c"))
        queue.offer(posted("d"))

        assertEquals(listOf("post:b", "post:c", "post:d"), keysOf(queue.drain()))
    }

    /**
     * **A removal is never dropped to make room for a post.** The queue is
     * full of posts, a removal arrives, and it is a post that goes.
     */
    @Test
    fun `a removal survives a full queue of posts`() {
        val queue = NotificationOutboundQueue(capacity = 3)
        queue.offer(posted("a"))
        queue.offer(posted("b"))
        queue.offer(posted("c"))
        queue.offer(NotificationEvent.Removed("z"))

        val drained = keysOf(queue.drain())
        assertTrue("remove:z" in drained)
        assertEquals(0, queue.droppedTerminal)
        assertEquals(1, queue.droppedNonTerminal)
    }

    /**
     * And every removal survives a flood of posts, as long as the removals
     * themselves fit: the queue gives up states, never terminal events.
     *
     * 200 posts and 20 removals into a queue of 32, drained once at the end.
     * The posts are decimated; not one removal is.
     */
    @Test
    fun `no removal is dropped under a flood of posts`() {
        val queue = NotificationOutboundQueue(capacity = 32)
        val removals = mutableListOf<String>()
        for (index in 0 until 200) {
            queue.offer(posted("post-$index"))
            if (index % 10 == 0) {
                val key = "gone-$index"
                removals += "remove:$key"
                queue.offer(NotificationEvent.Removed(key))
            }
        }

        val drained = keysOf(queue.drain())
        assertEquals(0, queue.droppedTerminal)
        assertTrue(queue.droppedNonTerminal > 0)
        for (removal in removals) {
            assertTrue("$removal must not be dropped", removal in drained)
        }
    }

    /**
     * The one branch that can lose a removal, and it is counted rather than
     * silent. Reaching it needs a full queue of nothing but removals, which
     * means as many simultaneous removals as the capacity — but the counter is
     * what makes the claim checkable in a report instead of inferred from a
     * stale mirror.
     */
    @Test
    fun `dropping a terminal event is counted, never silent`() {
        val queue = NotificationOutboundQueue(capacity = 2)
        queue.offer(NotificationEvent.Removed("a"))
        queue.offer(NotificationEvent.Removed("b"))
        assertEquals(0, queue.droppedTerminal)

        queue.offer(NotificationEvent.Removed("c"))
        assertEquals(1, queue.droppedTerminal)
        // The newest truth got through; the oldest is what was lost.
        assertEquals(listOf("remove:b", "remove:c"), keysOf(queue.drain()))
    }

    /** A non-terminal event that cannot find room is dropped and counted. */
    @Test
    fun `a post is dropped when every pending event is terminal`() {
        val queue = NotificationOutboundQueue(capacity = 2)
        queue.offer(NotificationEvent.Removed("a"))
        queue.offer(NotificationEvent.Removed("b"))

        assertEquals(
            NotificationOutboundQueue.Result.DROPPED,
            queue.offer(posted("c")),
        )
        assertEquals(0, queue.droppedTerminal)
        assertEquals(listOf("remove:a", "remove:b"), keysOf(queue.drain()))
    }

    // -- what counts as terminal ---------------------------------------------

    @Test
    fun `terminal events are exactly the ones that must not be lost`() {
        assertTrue(NotificationEvent.Removed("a").isTerminal)
        assertTrue(NotificationEvent.ListenerDisconnected.isTerminal)
        assertFalse(posted("a").isTerminal)
        assertFalse(NotificationEvent.ListenerConnected.isTerminal)
        assertFalse(NotificationEvent.PolicyChanged.isTerminal)
    }

    // -- the rendering rule --------------------------------------------------

    @Test
    fun `the queue never renders a notification`() {
        val queue = NotificationOutboundQueue()
        queue.offer(posted("0|example.fixture.app|1|null|10123"))
        val rendered = queue.toString()
        assertFalse(rendered.contains("FIXTURE"))
        assertFalse(rendered.contains("example.fixture.app"))
        assertTrue(rendered.contains("pending=1"))
    }
}
