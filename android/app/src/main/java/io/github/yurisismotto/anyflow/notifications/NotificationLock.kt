package io.github.yurisismotto.anyflow.notifications

import android.app.KeyguardManager
import android.content.Context

/**
 * Whether this phone is locked, right now.
 *
 * ## Unknown is locked
 *
 * The contract is that any answer other than a confident "unlocked" is
 * `true`. A privacy control that fails open is not a control, and the failure
 * this rule prevents is concrete: an exception, a missing system service or an
 * OEM that answers oddly would otherwise send full notification bodies from a
 * locked phone to a screen in an office.
 *
 * ## Why `isDeviceLocked` and not `isKeyguardLocked`
 *
 * `isKeyguardLocked` is true whenever the keyguard is showing, including the
 * insecure swipe-to-dismiss one on a device with no lock set. `isDeviceLocked`
 * asks the question the policy is actually about — is the device secured
 * behind a credential — which means a phone with no lock configured is not
 * reported as locked forever, and a phone that does have one is reported the
 * instant it locks.
 */
fun interface LockState {
    fun isLocked(): Boolean

    companion object {
        /** For tests and for a source with no context. Fails closed. */
        val ALWAYS_LOCKED = LockState { true }
    }
}

/**
 * The platform implementation.
 *
 * Reads the answer at the moment it is asked rather than tracking it, because
 * the only thing that ever asks is the encoding step and a stale answer there
 * would be exactly the fail-open window this exists to close. No polling, no
 * broadcast receiver and no background work: this is one binder call on the
 * capability's own coroutine.
 */
class KeyguardLockState(context: Context) : LockState {
    private val appContext = context.applicationContext

    override fun isLocked(): Boolean = runCatching {
        val keyguard = appContext.getSystemService(KeyguardManager::class.java)
            ?: return true
        keyguard.isDeviceLocked
    }.getOrDefault(true)
}

/**
 * What is left of a notification after the source's privacy policy has been
 * applied.
 *
 * The point of this type is that the reduction happens **here, before
 * encoding**, and produces a value that simply does not contain the withheld
 * text. There is no "redact on the way out" step further down that could be
 * skipped, and nothing downstream is ever handed a full body plus a flag
 * saying not to use it.
 */
class ReducedNotification(
    val title: String,
    val body: String,
    /**
     * The source reduced this before sending it.
     *
     * Set for a policy reduction, **not** for a length truncation: a
     * truncation is a limit, not a privacy decision, and conflating them would
     * make the sink show "reduced" for a long chat message.
     */
    val redacted: Boolean,
) {
    /** Lengths, never text. */
    override fun toString(): String =
        "ReducedNotification(titleBytes=${title.toByteArray(Charsets.UTF_8).size}, " +
            "bodyBytes=${body.toByteArray(Charsets.UTF_8).size}, redacted=$redacted)"
}

/**
 * Applies the lock policy and the length limits, in that order.
 *
 * Order matters: policy first, so a withheld body is never truncated into
 * existence, and so `redacted` describes a privacy decision rather than a
 * ceiling.
 *
 * Returns null when the notification must not be sent at all, which is the
 * `Suppress` case. The filter normally catches that first; this is the second
 * of the two places that agree, because a privacy rule with one enforcement
 * point is a privacy rule one refactor away from being gone.
 */
object NotificationReduction {

    fun reduce(
        notification: PlatformNotification,
        policy: NotificationPolicy,
        sourceLocked: Boolean,
    ): ReducedNotification? {
        val effective = if (sourceLocked) policy.whenSourceLocked else LockPolicy.FULL
        return when (effective) {
            LockPolicy.SUPPRESS -> null
            LockPolicy.APP_ONLY -> ReducedNotification(title = "", body = "", redacted = true)
            LockPolicy.FULL -> ReducedNotification(
                title = NotificationText.reduce(
                    notification.title,
                    NotificationLimits.MAX_TITLE_BYTES,
                ),
                body = NotificationText.reduce(
                    notification.body,
                    NotificationLimits.MAX_BODY_BYTES,
                ),
                redacted = false,
            )
        }
    }
}
