package io.github.yurisismotto.anyflow.notifications

/**
 * Why a notification was not mirrored.
 *
 * A reason code and nothing else: every value here is safe to log and safe to
 * count, and none of them names a notification, a title or a body. These are
 * **local** diagnostics — they are never sent to a peer, which receives only
 * the `NotificationOutcome` vocabulary the schema defines.
 */
enum class DropReason {
    /**
     * AnyFlow's own package. A hard rule, checked first, with no setting that
     * turns it off. See [NotificationFilter].
     */
    OWN_PACKAGE,

    /**
     * `VISIBILITY_SECRET`. Never mirrored under any policy: an app that said
     * "do not show this even on a lock screen" has said something clear enough
     * that forwarding it to another machine cannot be right.
     */
    SECRET_VISIBILITY,

    /**
     * Android `IMPORTANCE_NONE`. A notification the phone does not show its
     * own owner must not become a desktop banner.
     */
    IMPORTANCE_NONE,

    /** The peer has no `notifications.v1` grant, or is not known at all. */
    NOT_GRANTED,

    /** Granted, but this computer's mirroring is switched off. */
    MIRRORING_OFF,

    /** A secondary (work) profile with `includeWorkProfile` off. */
    SECONDARY_PROFILE,

    /** Ongoing or foreground-service, with `includeOngoing` off. */
    ONGOING,

    /**
     * The application is not on the list shared with this computer.
     *
     * The deny-by-default outcome, and the one that fires for every app until
     * a person names it — including every newly installed one.
     */
    NOT_ALLOWED_APP,

    /** The phone is locked and this computer's lock policy is `Suppress`. */
    LOCK_SUPPRESSED,

    /** No `SOURCE` role: no notification access, or no usable secret. */
    NOT_A_SOURCE,
}

/** What the filter decided about one notification, for one peer. */
sealed interface FilterVerdict {
    /** Send it — subject to the privacy reduction, which is a separate step. */
    data object Allow : FilterVerdict

    /** Do not send it, and why. */
    data class Drop(val reason: DropReason) : FilterVerdict
}

/**
 * Whether one notification may be mirrored to one computer.
 *
 * A pure function of an extracted [PlatformNotification], a
 * [NotificationPolicy] and the current lock state. No I/O, no platform types,
 * no coroutines: every rule below is decided on the JVM in a test.
 *
 * ## The order is the specification
 *
 * The checks are applied in the order they are written, and the order is not
 * an implementation detail:
 *
 * 1. **own package**, before anything else — [02 §9.4] requires the drop to
 *    happen before the filter, before policy, before the caches, before the
 *    hash and before any encoding. It is what stops the ongoing-connection
 *    foreground-service notification — which exists on every running install —
 *    from being mirrored, and it stops the mirror-of-a-mirror class of loop
 *    before it can start. It is a **hard rule and not a user preference**:
 *    there is no policy field that could enable it, and no allow-list entry
 *    that could override it, which is asserted by test;
 * 2. **`VISIBILITY_SECRET`**, unconditionally, at the source;
 * 3. **`IMPORTANCE_NONE`**;
 * 4. the peer's grant and mirroring switch;
 * 5. the work-profile and ongoing switches, each independent of the app list;
 * 6. the **per-app allow-list**, deny by default;
 * 7. the lock policy, when it says suppress.
 *
 * Rules 1–3 are properties of the notification and are checked before any
 * per-peer state is consulted, so a notification that must never leave the
 * device is dropped once rather than once per peer.
 *
 * ## What is not a rule here
 *
 * Category is never an ACL. `CATEGORY_SYSTEM` is a claim by whichever app
 * posted the notification, and it neither grants nor denies anything — the
 * only thing that admits an application is a person naming it. There is no OTP
 * detection, no keyword list and no banking heuristic, and the design forbids
 * adding one.
 *
 * System applications are denied by default for exactly the same reason every
 * other application is: the allow-list starts empty and a system package is
 * not in it. AnyFlow does not classify packages as "system" and then trust the
 * classification — a check that could be wrong in either direction is worse
 * than the deny-by-default that needs no check at all.
 */
object NotificationFilter {

    /**
     * The hard, peer-independent rules.
     *
     * Applied once, before any peer is considered. [ownPackage] is this app's
     * own package name, passed in rather than read from a context so the rule
     * is testable and so it cannot be quietly disabled by a context that
     * failed to resolve.
     */
    fun screen(
        notification: PlatformNotification,
        ownPackage: String,
    ): FilterVerdict {
        // FIRST. Before the filter, before policy, before the HMAC, before
        // protobuf, before the queue. Loop prevention, not a preference.
        if (notification.packageName == ownPackage) {
            return FilterVerdict.Drop(DropReason.OWN_PACKAGE)
        }
        if (notification.visibility == NotificationMapping.VISIBILITY_SECRET) {
            return FilterVerdict.Drop(DropReason.SECRET_VISIBILITY)
        }
        if (notification.androidImportance == NotificationMapping.ANDROID_IMPORTANCE_NONE) {
            return FilterVerdict.Drop(DropReason.IMPORTANCE_NONE)
        }
        return FilterVerdict.Allow
    }

    /**
     * The per-peer rules, applied after [screen] has passed.
     *
     * [policy] must already be the *effective* policy — the one the trust
     * store returns having checked the grant, so a caller cannot ask about the
     * policy and forget to ask about the grant. [NotificationPolicy.DENIED] is
     * what an unknown, forgotten or ungranted peer resolves to.
     */
    fun decideForPeer(
        notification: PlatformNotification,
        policy: NotificationPolicy,
        sourceLocked: Boolean,
    ): FilterVerdict {
        if (!policy.allowMirror) {
            return FilterVerdict.Drop(
                if (policy == NotificationPolicy.DENIED) {
                    DropReason.NOT_GRANTED
                } else {
                    DropReason.MIRRORING_OFF
                },
            )
        }
        if (notification.secondaryProfile && !policy.includeWorkProfile) {
            return FilterVerdict.Drop(DropReason.SECONDARY_PROFILE)
        }
        if (notification.ongoing && !policy.includeOngoing) {
            return FilterVerdict.Drop(DropReason.ONGOING)
        }
        if (!policy.allowsApp(notification.packageName)) {
            return FilterVerdict.Drop(DropReason.NOT_ALLOWED_APP)
        }
        if (sourceLocked && policy.whenSourceLocked == LockPolicy.SUPPRESS) {
            return FilterVerdict.Drop(DropReason.LOCK_SUPPRESSED)
        }
        return FilterVerdict.Allow
    }

    /**
     * Both halves, for the common case.
     *
     * Kept as a convenience over the two functions rather than as the only
     * entry point, because the ordered producer screens once and then decides
     * per peer, and collapsing the two would make it screen once per peer.
     */
    fun decide(
        notification: PlatformNotification,
        ownPackage: String,
        policy: NotificationPolicy,
        sourceLocked: Boolean,
    ): FilterVerdict = when (val screened = screen(notification, ownPackage)) {
        is FilterVerdict.Drop -> screened
        FilterVerdict.Allow -> decideForPeer(notification, policy, sourceLocked)
    }
}
