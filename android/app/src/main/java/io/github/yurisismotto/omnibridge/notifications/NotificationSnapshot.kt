package io.github.yurisismotto.omnibridge.notifications

import io.github.yurisismotto.omnibridge.proto.capabilities.NotificationCategory
import io.github.yurisismotto.omnibridge.proto.capabilities.NotificationImportance
import io.github.yurisismotto.omnibridge.proto.capabilities.NotificationPrivacy

/**
 * Everything the listener reads off one `StatusBarNotification`, and nothing
 * else.
 *
 * ## Why this type exists
 *
 * `NotificationListenerService` callbacks arrive on the **main thread** — the
 * class javadoc says so from API 24 onward, and it is AOSP VERIFIED in
 * [00 §1.2]. Anything expensive there stalls the UI of the entire phone. So
 * the callback does one thing: copy the fields below out of the framework
 * object into this plain, immutable record, and hand it to the capability's
 * own coroutine. Every later step — package-manager lookups, the HMAC, the
 * protobuf, the socket — happens off that thread.
 *
 * This is also the **testable extraction boundary**. It holds no framework
 * type, so every rule that operates on an extracted notification —
 * own-package, filter, privacy reduction, mapping, identity — is a pure
 * function of this class and is tested on the JVM without a device.
 *
 * ## Not a data class, on purpose
 *
 * A generated `toString()` would put a notification's title and body into
 * every log line, every exception message and every test failure that touched
 * one. That is precisely the leak the logging audit exists to prevent, so the
 * `toString()` below is written by hand and names no content. `ClipboardText`
 * declines to be a data class for the same reason.
 *
 * ## Removal carries no content, and nothing here assumes it does
 *
 * The `StatusBarNotification` delivered to `onNotificationRemoved` is
 * explicitly *"light"* and may be missing heavyweight fields. Removal is
 * handled by platform key alone and never constructs one of these.
 */
class PlatformNotification(
    /**
     * Android's `StatusBarNotification.key`: `userId|pkg|id|tag|uid`.
     *
     * **Never transmitted, never logged.** It is the input to the identity
     * derivation and the argument `cancelNotification` takes, and it leaves
     * this device in neither form: the only handle a peer ever holds is the
     * derived, opaque `notification_id`, and the map back is in memory here.
     */
    val platformKey: String,
    /** `sbn.getPackageName()`. Transmitted as `app_id`. */
    val packageName: String,
    /**
     * The notification belongs to a profile other than this app's own.
     *
     * A boolean, never the numeric user id: the semantic is portable and the
     * number is an Android implementation detail and a fingerprinting surface
     * (ADR-0016 §8).
     */
    val secondaryProfile: Boolean,
    /** `sbn.getPostTime()`. Informational only, never an ordering input. */
    val postedAtUnixMs: Long,
    /** `sbn.isOngoing()` — `FLAG_ONGOING_EVENT`. */
    val ongoing: Boolean,
    /** `sbn.isClearable()`. False means a dismissal would be refused. */
    val clearable: Boolean,
    /** `Notification.visibility`: PUBLIC 1, PRIVATE 0, SECRET -1. */
    val visibility: Int,
    /**
     * `Ranking.getImportance()`, or [IMPORTANCE_UNKNOWN] when the ranking was
     * not available. Android's own scale, mapped in [NotificationMapping].
     */
    val androidImportance: Int,
    /** `Notification.category`, one of Android's 24 strings, or null. */
    val category: String?,
    /** `sbn.getGroupKey()`, or null when ungrouped. Hashed before sending. */
    val groupKey: String?,
    /** `FLAG_GROUP_SUMMARY`. */
    val groupSummary: Boolean,
    /** `EXTRA_TITLE`, already a `String`. Sensitive. */
    val title: String,
    /** `EXTRA_TEXT`, already a `String`. Sensitive. */
    val body: String,
    /** True only when the notification genuinely has a progress bar. */
    val hasProgress: Boolean,
    val progressCurrent: Int,
    val progressMax: Int,
    val progressIndeterminate: Boolean,
) {
    /**
     * Names the notification without describing it.
     *
     * The package is included because the logging policy permits it: it is
     * already transmitted as `app_id`, it is what a person needs to make sense
     * of a filter decision, and it says nothing about the notification's
     * content. Title, body and the platform key are absent and there is no
     * verbose mode that adds them.
     */
    override fun toString(): String =
        "PlatformNotification(pkg=$packageName, profile=${if (secondaryProfile) "secondary" else "primary"}, " +
            "ongoing=$ongoing, clearable=$clearable, visibility=$visibility)"

    companion object {
        /** `Ranking` was unavailable. Mapped conservatively, never dropped. */
        const val IMPORTANCE_UNKNOWN = Int.MIN_VALUE
    }
}

/**
 * Android's vocabulary, projected onto the portable one.
 *
 * Every mapping here is lossy on purpose and every one fails towards the
 * conservative answer. None of them is a security boundary: an app chooses its
 * own category and its own visibility, so both are claims by an arbitrary
 * application and the per-app allow-list is what actually decides.
 */
object NotificationMapping {

    // Android's Notification.visibility values, named rather than inlined.
    const val VISIBILITY_SECRET = -1
    const val VISIBILITY_PRIVATE = 0
    const val VISIBILITY_PUBLIC = 1

    // Android's NotificationManager.IMPORTANCE_* values.
    const val ANDROID_IMPORTANCE_NONE = 0
    const val ANDROID_IMPORTANCE_MIN = 1
    const val ANDROID_IMPORTANCE_LOW = 2
    const val ANDROID_IMPORTANCE_DEFAULT = 3
    const val ANDROID_IMPORTANCE_HIGH = 4
    const val ANDROID_IMPORTANCE_MAX = 5

    /**
     * MIN and LOW collapse to LOW, DEFAULT to NORMAL, HIGH and MAX to HIGH.
     *
     * `IMPORTANCE_NONE` has no wire value: a notification the phone does not
     * show its own owner must not become a desktop banner, so it is dropped by
     * the filter rather than mapped. An unknown or unavailable value maps to
     * `NORMAL`, which is what [02 §11.3] specifies — for importance the
     * conservative direction is the middle, because guessing LOW would hide a
     * message and guessing HIGH would make the mirror louder than the phone.
     */
    fun importance(android: Int): NotificationImportance = when (android) {
        ANDROID_IMPORTANCE_MIN, ANDROID_IMPORTANCE_LOW ->
            NotificationImportance.NOTIFICATION_IMPORTANCE_LOW
        ANDROID_IMPORTANCE_DEFAULT ->
            NotificationImportance.NOTIFICATION_IMPORTANCE_NORMAL
        ANDROID_IMPORTANCE_HIGH, ANDROID_IMPORTANCE_MAX ->
            NotificationImportance.NOTIFICATION_IMPORTANCE_HIGH
        else -> NotificationImportance.NOTIFICATION_IMPORTANCE_NORMAL
    }

    /**
     * An unset or unrecognised visibility is **PRIVATE**, never PUBLIC.
     *
     * An unknown value must not decay to the most permissive one. `SECRET` is
     * mapped so the filter can name it, and is then never transmitted under
     * any policy.
     */
    fun privacy(visibility: Int): NotificationPrivacy = when (visibility) {
        VISIBILITY_PUBLIC -> NotificationPrivacy.NOTIFICATION_PRIVACY_PUBLIC
        VISIBILITY_PRIVATE -> NotificationPrivacy.NOTIFICATION_PRIVACY_PRIVATE
        VISIBILITY_SECRET -> NotificationPrivacy.NOTIFICATION_PRIVACY_SECRET
        else -> NotificationPrivacy.NOTIFICATION_PRIVACY_PRIVATE
    }

    /**
     * Android's 24 categories, projected onto seven.
     *
     * Presentation and coalescing only. `CATEGORY_SYSTEM` arriving here is a
     * claim by whichever app posted the notification and grants nothing:
     * category is never an ACL.
     */
    fun category(category: String?): NotificationCategory = when (category) {
        "msg", "email", "social" -> NotificationCategory.NOTIFICATION_CATEGORY_MESSAGE
        "call", "missed_call", "voicemail" -> NotificationCategory.NOTIFICATION_CATEGORY_CALL
        "alarm", "reminder", "event" -> NotificationCategory.NOTIFICATION_CATEGORY_ALARM
        "progress" -> NotificationCategory.NOTIFICATION_CATEGORY_PROGRESS
        "transport" -> NotificationCategory.NOTIFICATION_CATEGORY_TRANSPORT
        "sys", "service", "err" -> NotificationCategory.NOTIFICATION_CATEGORY_SYSTEM
        null -> NotificationCategory.NOTIFICATION_CATEGORY_UNSPECIFIED
        else -> NotificationCategory.NOTIFICATION_CATEGORY_OTHER
    }
}
