package io.github.yurisismotto.omnibridge.notifications

/**
 * The three gates, as the UI has to explain them.
 *
 * ## Why this is a type and not three booleans on a screen
 *
 * `notifications.v1` has three independent permissions and they **fail
 * separately** (ADR-0017 §6):
 *
 * | Gate | Who holds it | How it is lost |
 * | --- | --- | --- |
 * | Android notification access | the operating system, for the whole device | revoked in Settings |
 * | the peer grant | this phone's trust store, per computer | switched off here |
 * | the announced roles | what each side can physically do right now | the listener unbinds, the computer's notification server dies |
 *
 * N2 observed a real state in which two were satisfied and the third was not:
 * Fedora had granted `notifications.v1` and announced `SINK`, the tablet had
 * not granted it and so announced `roles=0`, and nothing moved. A screen with
 * one switch cannot describe that; it would either claim to be working or
 * claim to be off, and both would be wrong.
 *
 * So the state is computed from the real gates, in one place, and the screen
 * renders it. Every value below is reachable from a real device state, and
 * each has a different fix.
 */
enum class NotificationReadiness {
    /**
     * The person has not allowed this computer to receive notifications.
     *
     * The starting state for every paired computer, because
     * `notifications.v1` is never auto-granted.
     */
    SHARING_OFF,

    /**
     * Sharing is on, but Android has not given OmniBridge notification access.
     *
     * The fix is in Settings, not here, and the screen offers the way there.
     */
    NEEDS_ANDROID_ACCESS,

    /**
     * Access and grant are both in place, but this phone still cannot read
     * notifications — the listener is not bound, or the notification secret
     * is unavailable.
     *
     * Distinct from [NEEDS_ANDROID_ACCESS] because the fix is different and
     * usually is not the person's: it resolves itself when the listener binds.
     */
    UNAVAILABLE,

    /**
     * Everything works, and mirroring has been paused for this computer.
     *
     * Reachable only by switching it off deliberately after granting.
     */
    PAUSED,

    /**
     * Everything works, and no application has been chosen.
     *
     * **Not an error.** It is the deliberate landing state of granting without
     * finishing the picker, and it shares nothing at all — which is the
     * correct, safe outcome and is what the copy says.
     */
    NO_APPS,

    /** Set up correctly, and there is no session to this computer right now. */
    NOT_CONNECTED,

    /**
     * Connected, and the computer has not said it can display notifications.
     *
     * Almost always the mirror image of [SHARING_OFF]: the computer has not
     * granted `notifications.v1` to *this phone* on its own side. It is the
     * state N2 measured, and it is the one a single switch would hide.
     */
    PEER_NOT_RECEIVING,

    /** All three gates are open and a session is up. */
    READY,
    ;

    /** Whether notifications are actually flowing to this computer. */
    val isReady: Boolean get() = this == READY

    /**
     * Whether anything at all could be sent in this state.
     *
     * [NOT_CONNECTED] counts: the configuration is complete and the next
     * session mirrors. It is the difference between "not set up" and "not
     * connected", which are different sentences.
     */
    val isConfigured: Boolean
        get() = this == READY || this == NOT_CONNECTED || this == PEER_NOT_RECEIVING
}

/**
 * What dismissal synchronisation is actually doing for one computer.
 *
 * ## Why this is a second type and not another [NotificationReadiness] value
 *
 * Mirroring and dismissal sync are two features that fail apart, and §16 of
 * the wave brief says so for a reason measured in N2: it is entirely ordinary
 * for mirroring to be `READY` while dismissal sync is unavailable, because the
 * computer is running a build that announces `SINK` but not
 * `DISMISS_REPORTER`. One enum with a single "Ready" would have to lie about
 * whichever half was worse — telling a person their notifications are not
 * being shared when they are, or that their phone will clear when it will not.
 *
 * So it is a second pure function, over the same gates plus two of its own,
 * and every value is reachable from a real device state.
 *
 * ## And why "off" is not a warning
 *
 * [OFF] is the default and the overwhelmingly common state. It is a choice
 * somebody has not made, not a fault, and drawing it as one would train people
 * to ignore the amber badge that means something *is* wrong.
 */
enum class NotificationDismissReadiness {
    /**
     * Android has not given OmniBridge notification access, so this phone could
     * not act on a dismissal even if it were switched on.
     *
     * First, because it is the gate that makes every other one moot, and
     * because the fix is in Settings rather than on this screen.
     */
    NEEDS_ANDROID_ACCESS,

    /** Switched off. The default, and not a fault. */
    OFF,

    /** On, and there is no session to this computer right now. */
    NOT_CONNECTED,

    /**
     * On and connected, and the computer has not said it will report a human
     * dismissal.
     *
     * Its notification server may not report why a notification closed, or it
     * may be running a build older than this feature. Either way it will never
     * ask, so nothing will happen — and saying so is better than a switch that
     * looks armed.
     */
    PEER_CANNOT_REPORT,

    /** On, and this computer's dismissals will be honoured here. */
    ACTIVE,
    ;

    /**
     * Whether this state needs somebody to do something.
     *
     * [OFF] and [NOT_CONNECTED] do not: one is a choice and the other is a
     * device that is merely elsewhere.
     */
    val needsAttention: Boolean
        get() = this == NEEDS_ANDROID_ACCESS || this == PEER_CANNOT_REPORT
}

/**
 * What the readiness is computed from.
 *
 * Plain booleans and a count, with no Android type anywhere, so the whole
 * decision table is a JVM test.
 */
data class NotificationGates(
    /** Android's own notification access, read fresh from the platform. */
    val osAccessGranted: Boolean,
    /** `notifications.v1` in this phone's trust store, for this computer. */
    val peerGranted: Boolean,
    /** [NotificationPolicy.allowMirror] for this computer. */
    val allowMirror: Boolean,
    /** How many applications this computer may receive. */
    val allowedAppCount: Int,
    /**
     * Whether this phone can source at all: the listener is bound and the
     * notification secret is usable. The same condition that decides whether
     * `SOURCE` is announced, because it is the same question.
     */
    val sourceActive: Boolean,
    /** A live session to this computer. */
    val peerConnected: Boolean,
    /** The computer has announced it can display notifications. */
    val peerIsSink: Boolean,
    /**
     * [NotificationPolicy.allowDismissSync] for this computer.
     *
     * Combined with [allowMirror] the way the runtime combines them: a policy
     * with mirroring off and dismiss sync on is contradictory and resolves to
     * "off", so the screen cannot promise something the runtime will refuse.
     */
    val allowDismissSync: Boolean = false,
    /** The computer has announced it will report human dismissals. */
    val peerIsDismissReporter: Boolean = false,
) {

    /**
     * The state, resolved.
     *
     * The order is the specification, in two blocks.
     *
     * **What the person can fix, first** — sharing, then Android access, then
     * the pause, then the applications. Sharing comes before access because
     * telling somebody to change an operating-system setting for a computer
     * they have not authorised would be asking for the wrong permission; and
     * "no applications chosen" comes before anything about the network,
     * because it is true and actionable whether or not the computer is on.
     *
     * **Then what the two devices are doing** — connected, able to read, able
     * to display. [UNAVAILABLE] therefore only fires while a session is
     * actually up, which is the only state in which the listener not being
     * bound is an anomaly rather than the designed idle behaviour.
     */
    fun readiness(): NotificationReadiness = when {
        !peerGranted -> NotificationReadiness.SHARING_OFF
        !osAccessGranted -> NotificationReadiness.NEEDS_ANDROID_ACCESS
        !allowMirror -> NotificationReadiness.PAUSED
        allowedAppCount == 0 -> NotificationReadiness.NO_APPS
        !peerConnected -> NotificationReadiness.NOT_CONNECTED
        !sourceActive -> NotificationReadiness.UNAVAILABLE
        !peerIsSink -> NotificationReadiness.PEER_NOT_RECEIVING
        else -> NotificationReadiness.READY
    }

    /**
     * The dismissal state, resolved.
     *
     * The order, and why:
     *
     * 1. **Android access first.** Without it this phone cannot cancel
     *    anything for anybody, so however the switch is set, nothing can
     *    happen — and the fix is somewhere else entirely.
     * 2. **Then off.** When it is off there is nothing to fix and nothing to
     *    warn about, and mentioning a computer's capabilities for a feature
     *    somebody has not enabled is noise.
     * 3. **Then the network, then the far end**, which is the same shape
     *    [readiness] uses.
     *
     * The app allow-list and the mirroring switch are deliberately *not*
     * inputs beyond `allowMirror`'s containment role: a dismissal is about a
     * notification that is already on the computer's screen, so whether a new
     * one would be shared says nothing about whether an old one may be
     * cleared.
     */
    fun dismissReadiness(): NotificationDismissReadiness = when {
        !osAccessGranted -> NotificationDismissReadiness.NEEDS_ANDROID_ACCESS
        !allowMirror || !allowDismissSync -> NotificationDismissReadiness.OFF
        !peerConnected -> NotificationDismissReadiness.NOT_CONNECTED
        !peerIsDismissReporter -> NotificationDismissReadiness.PEER_CANNOT_REPORT
        else -> NotificationDismissReadiness.ACTIVE
    }
}
