package io.github.yurisismotto.anyflow.notifications

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
     * Sharing is on, but Android has not given AnyFlow notification access.
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
}
