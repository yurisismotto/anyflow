package io.github.yurisismotto.anyflow.notifications

import com.google.protobuf.ByteString
import io.github.yurisismotto.anyflow.identity.Fingerprint

/**
 * One thing that happened, on its way from a listener callback to the wire.
 *
 * Everything `notifications.v1` sends is produced by draining this queue on a
 * single coroutine, which is what makes the wire order well-defined — see
 * [NotificationOutboundQueue].
 */
sealed interface NotificationEvent {

    /**
     * A notification was posted or updated.
     *
     * Android does not distinguish the two: `onNotificationPosted` fires for
     * both with the same key, which is exactly why the protocol has one
     * idempotent upsert and no separate update message.
     *
     * The only **coalescable** event: a newer one for the same platform key
     * replaces a pending older one, because the older one is a state the sink
     * would immediately overwrite.
     */
    class Posted(val notification: PlatformNotification) : NotificationEvent

    /**
     * A notification is gone.
     *
     * **Terminal**, and terminal events are never coalesced away. Losing a
     * removal leaves a mirror on a screen for ever, which is the one failure
     * mode worse than being slow — and worse still when the mirror is of a
     * banking alert. Android's 23 removal reasons all mean the same thing to a
     * mirror, so none is carried and none is consulted here.
     */
    class Removed(
        val platformKey: String,
        /**
         * Whether the platform attributed this removal to a listener calling
         * `cancelNotification` — Android's `REASON_LISTENER_CANCEL`.
         *
         * **The one bit of Android's removal reason that is ever consulted,
         * and it never leaves this device.** All 23 reasons mean the same
         * thing to a mirror, so none of them is transmitted; this one is used
         * locally, for echo suppression, and for nothing else. Reducing it to
         * a boolean here rather than carrying the number is deliberate: a
         * numeric reason in a portable type is an invitation to branch on
         * `REASON_PACKAGE_BANNED` or `REASON_CLEAR_DATA`, which are facts
         * about the user's device that a mirror has no business knowing.
         */
        val listenerCancelled: Boolean = false,
    ) : NotificationEvent

    /** The system bound the listener: `getActiveNotifications` is now safe. */
    data object ListenerConnected : NotificationEvent

    /**
     * The listener was unbound, or notification access was revoked.
     *
     * Terminal for the `SOURCE` role: it narrows on this event, immediately,
     * without waiting for a reconnect.
     */
    data object ListenerDisconnected : NotificationEvent

    /** A session was established with a peer. Roles and a snapshot follow. */
    class SessionAttached(
        val peer: Fingerprint,
        val peerDeviceId: String,
        val send: suspend (ByteString) -> Unit,
    ) : NotificationEvent

    /** A session ended. Nothing is queued for a peer that is not there. */
    class SessionDetached(val peer: Fingerprint) : NotificationEvent

    /** Something local changed — a grant, a policy — so re-evaluate. */
    data object PolicyChanged : NotificationEvent

    /**
     * A message arrived from a peer, already decoded.
     *
     * Routed through the same queue as everything else so that a reply is
     * ordered with respect to the traffic around it, and so that peer role
     * state is mutated on the one coroutine that owns it.
     */
    class Inbound(
        val peer: Fingerprint,
        val control: io.github.yurisismotto.anyflow.proto.capabilities.NotificationControl,
    ) : NotificationEvent

    /** True for events that must never be dropped to make room. */
    val isTerminal: Boolean
        get() = this is Removed || this is ListenerDisconnected || this is SessionDetached
}

/**
 * The bounded hand-off between the phone's main thread and the ordered
 * producer.
 *
 * ## Why a queue at all
 *
 * `NotificationListenerService` callbacks run on the **main thread**. They may
 * not block, may not encode protobuf, may not hash and may not touch a socket,
 * so they do one thing: [offer] a plain event and return. A single coroutine
 * drains it and does the rest.
 *
 * ## Why one queue and not one per peer
 *
 * The session's outbound channel guarantees FIFO **per producer**, not across
 * producers ([NOTIFICATIONS.md](../../../../../../../../docs/architecture/NOTIFICATIONS.md)
 * "Ordering"). Two independent senders could let a `NotificationRemove`
 * overtake the `NotificationUpsert` it refers to, and the sink would be left
 * showing a notification the phone no longer has — permanently, because the
 * removal already happened. One queue drained by one coroutine makes that
 * impossible by construction rather than by care, and it is also what keeps a
 * snapshot's `BEGIN`, its items and its `END` from interleaving with live
 * traffic incorrectly.
 *
 * ## Bounds and overflow, stated exactly
 *
 * The queue holds at most [capacity] events. It is **never**
 * `Channel.UNLIMITED`: a notification storm on a phone with no connected peer
 * would otherwise grow the heap until the process died.
 *
 * When it is full, in this order:
 *
 * 1. a [NotificationEvent.Posted] for a key already pending **replaces** it in
 *    place. A progress bar updating sixty times a second occupies one slot,
 *    always holding the current state. This is the per-identity *slot* the
 *    design calls for rather than a queue of stale states, and it means the
 *    common flood never reaches the overflow path at all;
 * 2. otherwise the **oldest non-terminal** event is evicted to make room. The
 *    sink converges anyway: a dropped upsert is a state the next upsert or the
 *    next reconnect snapshot restores;
 * 3. only if there is no non-terminal event to evict — every pending event is
 *    a removal or a session change — is a terminal event dropped, and that
 *    drop is **counted and logged**, never silent. [droppedTerminal] is the
 *    number of times it has happened, and it is expected to stay zero: a
 *    pending removal exists only for a notification that was on the device, so
 *    reaching this branch needs [capacity] simultaneous removals.
 *
 * Nothing here ever blocks, so the phone's UI cannot be stalled by a peer that
 * stopped reading.
 *
 * ## Thread safety
 *
 * [offer] is called from the main thread and [drain] from the producer
 * coroutine, so every method synchronises on the queue itself. The
 * *processing* of a drained event is single-threaded by construction.
 */
class NotificationOutboundQueue(
    private val capacity: Int = NotificationLimits.EVENT_QUEUE_CAPACITY,
) {
    private val events = ArrayDeque<NotificationEvent>()

    /** How many non-terminal events have been dropped for want of room. */
    var droppedNonTerminal: Int = 0
        private set

    /**
     * How many terminal events have been dropped.
     *
     * Expected to be zero for ever. It is a counter rather than an assertion
     * because a crash is not an improvement on a stale mirror, and because a
     * number that a report can print is what makes the claim checkable.
     */
    var droppedTerminal: Int = 0
        private set

    /** How many pending events were replaced by a newer state of the same id. */
    var coalesced: Int = 0
        private set

    /** What [offer] did, so a caller can log a count rather than an event. */
    enum class Result { QUEUED, COALESCED, EVICTED_OLDEST, DROPPED }

    @Synchronized
    fun size(): Int = events.size

    @Synchronized
    fun offer(event: NotificationEvent): Result {
        // A newer state of the same notification supersedes the pending one,
        // wherever it sits in the queue: replacing in place preserves the
        // order the sink sees, which is what a slot means here.
        if (event is NotificationEvent.Posted) {
            val index = events.indexOfFirst {
                it is NotificationEvent.Posted &&
                    it.notification.platformKey == event.notification.platformKey
            }
            if (index >= 0) {
                events[index] = event
                coalesced += 1
                return Result.COALESCED
            }
        }

        // A removal supersedes any pending post for the same notification:
        // sending a state and then removing it is two messages where one will
        // do, and the sink converges on the same answer either way.
        if (event is NotificationEvent.Removed) {
            events.removeAll {
                it is NotificationEvent.Posted &&
                    it.notification.platformKey == event.platformKey
            }
        }

        if (events.size < capacity) {
            events.addLast(event)
            return Result.QUEUED
        }

        val evictable = events.indexOfFirst { !it.isTerminal }
        if (evictable >= 0) {
            events.removeAt(evictable)
            droppedNonTerminal += 1
            events.addLast(event)
            return Result.EVICTED_OLDEST
        }

        if (event.isTerminal) {
            // Every pending event is itself terminal. Drop the oldest so the
            // newest truth still gets out, and count it: this is the only
            // branch in the design that loses a removal, and it must be
            // visible in a report rather than inferred from a stale mirror.
            events.removeFirst()
            droppedTerminal += 1
            events.addLast(event)
            return Result.EVICTED_OLDEST
        }

        droppedNonTerminal += 1
        return Result.DROPPED
    }

    /**
     * Takes everything pending, in order.
     *
     * Drained in a batch rather than one at a time so the producer makes one
     * pass per wakeup, and so a burst that arrived while it was busy is
     * processed in the order it was queued.
     */
    @Synchronized
    fun drain(): List<NotificationEvent> {
        if (events.isEmpty()) return emptyList()
        val taken = events.toList()
        events.clear()
        return taken
    }

    /** Drops everything pending. Used when the source goes inert. */
    @Synchronized
    fun clear() {
        events.clear()
    }

    /** Counts only. */
    override fun toString(): String =
        "NotificationOutboundQueue(pending=${events.size}, coalesced=$coalesced, " +
            "droppedNonTerminal=$droppedNonTerminal, droppedTerminal=$droppedTerminal)"
}
