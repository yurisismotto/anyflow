package io.github.yurisismotto.anyflow.notifications

import io.github.yurisismotto.anyflow.proto.capabilities.NotificationRole
import io.github.yurisismotto.anyflow.proto.capabilities.NotificationRoles

/**
 * What this phone claims it can do for `notifications.v1`, on one connection.
 *
 * ADR-0017. Roles live inside the capability rather than in `HELLO` because
 * they **change while the connection is up**: the user can revoke notification
 * access in Settings at any moment, which fires `onListenerDisconnected()`
 * on a session that is otherwise healthy. The phone has to be able to say "I
 * am no longer a source" *now*, without a reconnect.
 *
 * ## A role is not an authorization
 *
 * It is a claim about what this device can physically do, and its only job is
 * to stop a peer waiting for content that will never come. Three separate
 * questions stay separate:
 *
 * | | Answered by |
 * | --- | --- |
 * | May this app read notifications on this phone? | Android, in Settings |
 * | May this computer be sent them? | the local trust store, per peer |
 * | Can this device source at all right now? | this class |
 *
 * The grant is checked independently, per message, and no role state feeds it.
 *
 * ## Which roles this phone announces
 *
 * `SOURCE` and `DISMISS_TARGET` together, which is ADR-0017 §1's v1 assignment
 * for Android, complete as of N4.
 *
 * ### Why they are one set and not two switches
 *
 * They are announced together because they are true together. Both need the
 * same three things — a bound listener, the OS notification-access grant, and
 * a usable notification secret — and *the secret is why*: a `DismissRequest`
 * names a notification by its derived id, and the only way back to a platform
 * key is the [SourceIdMap], whose entries are built by deriving ids with that
 * secret. With no secret the map is empty, so every dismiss would answer
 * `UNKNOWN_NOTIFICATION` for ever, which is a promise not kept rather than a
 * capability.
 *
 * If a future platform can honour dismissals without being able to source —
 * or can source without being able to cancel — this set stops being one thing
 * and the two are announced separately. Nothing in the protocol assumes they
 * travel together; [SOURCING] is a fact about this adapter.
 *
 * ### And why the dismiss-sync *policy* is not an input
 *
 * `allowDismissSync` is off by default and per peer, and it is deliberately
 * **not** consulted here. A role says what this device can physically do; a
 * policy says whether a particular request is allowed, and it is checked on
 * every inbound `DismissRequest` (ADR-0017 §6). Folding the policy into the
 * role would tell a peer "I cannot do this" when the truth is "I will not",
 * and the desktop's own UI needs to tell those apart to say anything useful.
 *
 * ## The epoch
 *
 * Monotonic per connection, from 1, never persisted, never reused. It exists
 * for exactly one failure: a reordered, duplicated or replayed announcement
 * re-widening a set that has already narrowed. Not carried across a reconnect,
 * because each side announces first thing on connect and there is nothing to
 * resynchronise.
 *
 * Not thread-safe: the ordered producer owns one per peer and serialises
 * access, which is also what keeps announcements in order with the upserts
 * they gate.
 */
class SourceRoleState {

    private var lastEpoch: Long = 0
    private var announced: Set<NotificationRole>? = null

    /** The epoch of the last announcement made, or 0 if none has been. */
    fun epoch(): Long = lastEpoch

    /** The set last announced, or null when nothing has been announced yet. */
    fun current(): Set<NotificationRole>? = announced

    /**
     * The next announcement, or null when [roles] is already what the peer was
     * told.
     *
     * Returning null rather than a duplicate matters: an epoch is only useful
     * while it strictly increases for a *changed* set, and re-announcing the
     * same set on every event would burn epochs and add wire traffic that says
     * nothing.
     *
     * The first announcement is always made, including the empty one. An
     * empty set with a real epoch is exactly how a phone with no notification
     * access says so, and a peer that is told nothing and a peer that is told
     * "nothing" must reach the same conclusion.
     */
    fun announce(roles: Set<NotificationRole>): NotificationRoles? {
        if (announced == roles) return null
        announced = roles
        lastEpoch += 1
        val builder = NotificationRoles.newBuilder()
        // Sorted by wire number so the encoding is deterministic: two runs of
        // the same state produce the same bytes, which is what makes a
        // cross-language vector possible at all.
        for (role in roles.sortedBy { it.number }) {
            builder.addRoles(role)
        }
        // uint32 on the wire. The epoch is per connection and a session would
        // have to survive four billion permission changes to reach the
        // ceiling; if one somehow did, refusing further updates is the safe
        // failure and the cast below is what produces it.
        builder.epoch = lastEpoch.toInt()
        return builder.build()
    }

    companion object {
        /**
         * What a phone that can currently observe its own notifications says.
         *
         * Both roles, for the reason above: the listener and the secret that
         * make sourcing possible are exactly what make a dismissal possible.
         */
        val SOURCING: Set<NotificationRole> = setOf(
            NotificationRole.NOTIFICATION_ROLE_SOURCE,
            NotificationRole.NOTIFICATION_ROLE_DISMISS_TARGET,
        )

        /**
         * What it says the instant notification access is revoked.
         *
         * Indistinguishable from never having announced, on purpose
         * (ADR-0017 §2).
         */
        val NONE: Set<NotificationRole> = emptySet()
    }
}

/**
 * What a *peer* told us it can do, reduced under the epoch rule.
 *
 * The mirror image of [SourceRoleState], and the Kotlin twin of
 * `anyflow_core::notifications::PeerRoles`. It is recorded and used for
 * exactly two decisions — do not send upserts to a peer that never claimed
 * `SINK`, and report to the UI whether a computer has claimed
 * `DISMISS_REPORTER` so the dismiss-sync row can be honest about it — and for
 * no others.
 *
 * **It is never an authorization input.** In particular, a peer claiming
 * `DISMISS_REPORTER` gains nothing: what decides whether this phone acts on a
 * `DismissRequest` is the pinned identity, the grant and the local policy, and
 * none of them can be reached from anything a peer says about itself.
 */
class PeerRoleState {

    private var epoch: Int = 0
    private val roles = mutableSetOf<NotificationRole>()

    /** Why an announcement was not applied. Local diagnostics only. */
    enum class Rejection {
        /** Epoch 0 is "unset" and is refused. */
        UNSET_EPOCH,

        /**
         * Not strictly greater than the last accepted.
         *
         * *Equal* is refused too: a duplicate of the current epoch carrying a
         * different set must not take effect either, or a replay could
         * re-widen a set that has narrowed.
         */
        STALE_EPOCH,
    }

    fun epoch(): Int = epoch

    fun has(role: NotificationRole): Boolean = role in roles

    fun isEmpty(): Boolean = roles.isEmpty()

    /**
     * Applies an announcement, or refuses it.
     *
     * An unknown role value is dropped and the rest of the set still applies:
     * one unrecognised value must not discard an announcement, and it must not
     * be inferred into existence — a peer that has never heard of a role
     * cannot have implemented it.
     */
    fun apply(announcement: NotificationRoles): Rejection? {
        if (announcement.epoch == 0) return Rejection.UNSET_EPOCH
        // Unsigned comparison: a uint32 past Int.MAX_VALUE arrives negative.
        if (announcement.epoch.toUInt() <= epoch.toUInt()) return Rejection.STALE_EPOCH

        epoch = announcement.epoch
        roles.clear()
        for (role in announcement.rolesList) {
            if (role != null &&
                role != NotificationRole.UNRECOGNIZED &&
                role != NotificationRole.NOTIFICATION_ROLE_UNSPECIFIED
            ) {
                roles += role
            }
        }
        return null
    }

    /** Counts and an epoch, never a peer's content. */
    override fun toString(): String = "PeerRoleState(epoch=$epoch, roles=${roles.size})"
}
