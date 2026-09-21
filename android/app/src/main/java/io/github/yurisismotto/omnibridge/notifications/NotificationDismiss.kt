package io.github.yurisismotto.omnibridge.notifications

import io.github.yurisismotto.omnibridge.proto.capabilities.DismissRequest
import io.github.yurisismotto.omnibridge.proto.capabilities.NotificationOutcome

/**
 * Whether one inbound `DismissRequest` may cause a `cancelNotification`, and
 * what to answer when it may not.
 *
 * ## Why this is a pure function
 *
 * `DismissRequest` is **the only message in `notifications.v1` that travels
 * sink → source and causes an effect on this device**. Everything else this
 * capability receives is a claim, an answer or a marker. So the decision that
 * guards it is written once, here, as a pure function of plain values — no
 * coroutine, no platform type, no listener, no trust store — and the whole
 * decision table is enumerated by a JVM test rather than inspected on a
 * tablet.
 *
 * ## The order is the specification
 *
 * Every check fails closed, and the order they are written in is the order
 * they apply. Two rules shape it:
 *
 * 1. **A malformed identifier is refused and not answered.** A
 *    `NotificationResult` echoes the `notification_id`, so a bad-width one
 *    leaves nothing coherent to correlate a reply with (ADR-0016 §9) — the
 *    same rule `clipboard.v1` applies to a bad-width `event_id`. That check is
 *    therefore first, because it decides whether an answer is possible at all.
 * 2. **Nothing later than a refusal may be evaluated.** In particular, the
 *    [SourceIdMap] is not consulted until the grant, the role and the policy
 *    have all passed — so an ungranted peer cannot use the answer to learn
 *    whether an id exists on this phone. `DismissRequest` is not a lookup
 *    oracle, and the way it stays not one is that a peer which fails an
 *    earlier gate never reaches the lookup.
 *
 * ```text
 *   notification_id is 16 bytes      → refuse, do not answer
 *   origin_device_id is 32 lc hex    → INVALID
 *   the peer has a notifications.v1 grant → NOT_AUTHORIZED
 *   this phone can act at all (listener, access, secret) → REJECTED_ROLE
 *   allowMirror && allowDismissSync  → REJECTED_POLICY
 *   origin_device_id names THIS device → INVALID
 *   ─────────────────────────────────────────────────────
 *   only now: is there such a notification, and may it be cleared?
 * ```
 *
 * The last two lines are deliberately below the fold: they are the only ones
 * whose answer depends on what is actually in this phone's notification shade.
 */
object NotificationDismissRules {

    /** What to do with one `DismissRequest`. */
    sealed interface Verdict {
        /**
         * Refused, and **not answered at all**.
         *
         * Only reachable for a bad-width `notification_id`, because the answer
         * would have nothing coherent to name.
         */
        data object Unanswerable : Verdict

        /** Refused, with an outcome the peer can act on. */
        data class Refuse(val outcome: NotificationOutcome, val reason: DismissRefusal) : Verdict

        /**
         * Every gate this function can answer has passed.
         *
         * What remains — does this id map to a notification this device
         * actually sourced, is it still there, and is it clearable *now* — can
         * only be answered against the live platform, so it is answered by the
         * caller. This value is the statement that the caller is allowed to
         * ask.
         */
        data object Proceed : Verdict
    }

    /**
     * Why a dismissal was refused. A reason class and nothing else.
     *
     * Local diagnostics only: these names never reach a peer, which receives
     * only the [NotificationOutcome] vocabulary the schema defines. Every value
     * here is safe to log — none of them names a notification, an application,
     * a title or a platform key.
     */
    enum class DismissRefusal {
        /** `notification_id` was not exactly 16 bytes. */
        BAD_ID_WIDTH,

        /** `origin_device_id` was not 32 lowercase hex characters. */
        MALFORMED_ORIGIN,

        /** The peer has no `notifications.v1` grant, or is not known at all. */
        NOT_GRANTED,

        /**
         * This phone cannot act on a dismissal right now: the listener is not
         * bound, notification access is absent, or the secret is unusable.
         *
         * The same condition that decides whether `DISMISS_TARGET` is
         * announced, because it is the same question — so a peer receiving
         * this has been told, by the role announcement it also received, that
         * this would happen.
         */
        NOT_A_DISMISS_TARGET,

        /**
         * Granted, and this computer is not allowed to dismiss here.
         *
         * The default. `allowDismissSync` is off until a person turns it on
         * (ADR-0015 §6), and turning mirroring off makes a stale flag inert.
         */
        DISMISS_SYNC_OFF,

        /**
         * `origin_device_id` named a device that is not this one.
         *
         * A peer cannot dismiss a third device's notification through us. It
         * is refused as malformed rather than as unknown, because it is a
         * statement about the *message* — the sender addressed the wrong
         * device — and not about any notification this phone may or may not
         * hold.
         */
        NOT_THE_ORIGIN,
    }

    /**
     * The gates that do not need the platform.
     *
     * @param localDeviceId this device's own id, as it stamps into every
     *   outbound message. The comparison is exact: an identifier is never
     *   normalised to fit, because normalising an identifier is how two
     *   different things come to compare equal.
     * @param canDismiss whether this phone can act on a dismissal at all — the
     *   same condition that decides the `DISMISS_TARGET` role.
     */
    fun screen(
        request: DismissRequest,
        localDeviceId: String,
        policy: NotificationPolicy,
        canDismiss: Boolean,
    ): Verdict {
        if (request.notificationId.size() != NotificationLimits.NOTIFICATION_ID_LENGTH) {
            return Verdict.Unanswerable
        }
        if (!isDeviceId(request.originDeviceId)) {
            return refuse(
                NotificationOutcome.NOTIFICATION_OUTCOME_INVALID,
                DismissRefusal.MALFORMED_ORIGIN,
            )
        }
        if (policy == NotificationPolicy.DENIED) {
            return refuse(
                NotificationOutcome.NOTIFICATION_OUTCOME_NOT_AUTHORIZED,
                DismissRefusal.NOT_GRANTED,
            )
        }
        if (!canDismiss) {
            return refuse(
                NotificationOutcome.NOTIFICATION_OUTCOME_REJECTED_ROLE,
                DismissRefusal.NOT_A_DISMISS_TARGET,
            )
        }
        // `allowMirror` as well as `allowDismissSync`: a policy with mirroring
        // off and dismiss sync on is contradictory and resolves to "no", the
        // same containment rule the desktop's `may_sync_dismissals` applies.
        if (!policy.allowMirror || !policy.allowDismissSync) {
            return refuse(
                NotificationOutcome.NOTIFICATION_OUTCOME_REJECTED_POLICY,
                DismissRefusal.DISMISS_SYNC_OFF,
            )
        }
        if (request.originDeviceId != localDeviceId) {
            return refuse(
                NotificationOutcome.NOTIFICATION_OUTCOME_INVALID,
                DismissRefusal.NOT_THE_ORIGIN,
            )
        }
        return Verdict.Proceed
    }

    /**
     * Whether a notification the platform still holds may be cancelled.
     *
     * Read from the **live** notification rather than from the `dismissible`
     * flag the desktop was sent: that value was true when the upsert left this
     * device, and an app can make a notification ongoing between the mirror
     * appearing and the dismissal arriving. The source is the only place that
     * knows, and it knows it now rather than then.
     *
     * `null` means the platform no longer has it, which is
     * `UNKNOWN_NOTIFICATION` — an answer, not an error. Both ends converging on
     * "it is gone" is the correct outcome and is what makes dismissal
     * idempotent.
     */
    fun decideClearable(current: PlatformNotification?): NotificationOutcome = when {
        current == null -> NotificationOutcome.NOTIFICATION_OUTCOME_UNKNOWN_NOTIFICATION
        // Both, and not just `clearable`. `isClearable()` is already false for
        // an ongoing notification on every Android OmniBridge supports, but the
        // two are separate platform concepts and a rule that relies on one
        // implying the other is a rule that breaks on the release where it
        // stops.
        !current.clearable || current.ongoing ->
            NotificationOutcome.NOTIFICATION_OUTCOME_NOT_DISMISSIBLE
        else -> NotificationOutcome.NOTIFICATION_OUTCOME_REMOVED
    }

    private fun refuse(outcome: NotificationOutcome, reason: DismissRefusal) =
        Verdict.Refuse(outcome, reason)

    /**
     * Exactly 32 lowercase hex characters, the same rule
     * `omnibridge_core::notifications::check_device_id` applies on the other end.
     *
     * Uppercase is refused rather than folded: the two implementations must
     * agree on what is valid, and a receiver that repairs input is a receiver
     * whose limits are advisory.
     */
    private fun isDeviceId(value: String): Boolean =
        value.length == NotificationLimits.DEVICE_ID_HEX_LENGTH &&
            value.all { it in '0'..'9' || it in 'a'..'f' }
}
