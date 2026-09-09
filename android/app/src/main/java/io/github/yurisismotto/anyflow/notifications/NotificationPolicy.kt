package io.github.yurisismotto.anyflow.notifications

import org.json.JSONArray
import org.json.JSONObject

/**
 * How much of a notification crosses the wire while the source device is
 * locked.
 *
 * The reduction happens **at the source, before encoding**, so content
 * withheld by policy never enters the protobuf, never touches the socket and
 * never exists on the desktop. That is the strong form of a privacy control:
 * there is nothing to leak because there is nothing there.
 *
 * Unlocking is **not retroactive**. Content withheld while locked is not
 * delivered later, because holding it somewhere until the phone unlocks would
 * be a notification history by another name.
 */
enum class LockPolicy {
    /** Everything, as if unlocked. */
    FULL,

    /** `app_label` only. `title` and `body` are empty and `redacted` is set. */
    APP_ONLY,

    /** Nothing is sent at all. */
    SUPPRESS,
    ;

    companion object {
        /** ADR-0015 §7: the default, on both ends. */
        val DEFAULT = APP_ONLY

        /** An unrecognised stored value is the default, never [FULL]. */
        fun fromName(name: String?): LockPolicy =
            entries.firstOrNull { it.name == name } ?: DEFAULT
    }
}

/**
 * What this phone will mirror to one computer.
 *
 * ## Two different questions, again
 *
 * Exactly as `clipboard.v1` (see [io.github.yurisismotto.anyflow.clipboard.ClipboardPolicy]):
 *
 *  * the **grant** — in the trust store, beside `files.v1`'s — answers "may
 *    this computer receive notifications from me at all?", and it is never
 *    automatic and never in `auto_grant` (ADR-0015 §4);
 *  * the **policy** — this type — answers "which of them, and how much of
 *    each?".
 *
 * And there is a third question neither of them answers: whether Android has
 * given this app notification access at all. Collapsing any two of the three
 * would mean a person who wanted a phone-side capability had silently
 * authorised a network destination (ADR-0017 §6).
 *
 * ## A peer can never set its own policy
 *
 * Every field here is decided and stored locally. **There is no protocol
 * message that writes any of them**, and that is enforced by the absence of a
 * field in the schema rather than by a check that a later change could invert.
 *
 * ## Deny by default
 *
 * [allowedApps] starts **empty**, so a freshly granted computer receives
 * nothing until a person names an application. That is the single most
 * important default in this capability: on the certification hardware the
 * platform's own OTP redaction did not fire at all (POC-NOTIF-01), which makes
 * the per-app allow-list the only effective control there is — an argument for
 * deny-by-default rather than against it.
 *
 * An app installed after the grant is therefore *also* denied, without needing
 * a rule of its own: it is simply not in the set. N3 owns the picker that
 * fills it, and the passive "3 new apps are not being shared" affordance.
 *
 * ## What is deliberately not here
 *
 * No OTP heuristic, no keyword list, no banking-app detection, no
 * category-based rule. A guess dressed as a security control is worse than an
 * honest boundary, because the user trusts it and it is wrong in cases neither
 * party can predict (THREAT_MODEL T10, ADR-0015 §5).
 */
data class NotificationPolicy(
    /**
     * May this computer receive mirrored notifications at all?
     *
     * Defaults **on**, for the reason `clipboard.v1`'s `allowSend` does:
     * reaching this field already required an explicit, revocable grant, so it
     * is what the person just asked for. It is not a permissive default,
     * because [allowedApps] is empty and nothing is mirrored until it is not.
     */
    val allowMirror: Boolean = true,
    /**
     * The packages this computer may receive. **Empty by default.**
     *
     * AnyFlow's own package is not in this set and could not help if it were:
     * the own-package rule is applied before the filter is consulted and there
     * is no setting that turns it off.
     */
    val allowedApps: Set<String> = emptySet(),
    /**
     * Work-profile notifications, behind their own switch and **off**.
     *
     * A listener in the personal profile receives work-profile notifications
     * unless the administrator blocks it. Mirroring an employer's data onto a
     * personal machine is not a decision AnyFlow makes on someone's behalf,
     * and a user who shares "Slack" from their personal profile has not
     * thereby asked to share work Slack.
     */
    val includeWorkProfile: Boolean = false,
    /**
     * Ongoing and foreground-service notifications. **Off.**
     *
     * "Maps is navigating", "Spotify is playing". They are not events, they
     * update constantly, they are usually not dismissible, and mirroring them
     * fills a desktop with rows that cannot be cleared.
     */
    val includeOngoing: Boolean = false,
    /** ADR-0015 §7: what crosses the wire while this phone is locked. */
    val whenSourceLocked: LockPolicy = LockPolicy.DEFAULT,
    /**
     * Whether a desktop dismissal may cancel the notification here.
     *
     * **Off by default** (ADR-0015 §6), and **not implemented in N1**: this
     * wave contains no path from an inbound message to `cancelNotification`,
     * and Android announces no `DISMISS_TARGET` role. The field is stored so
     * N4 has somewhere to read the answer from, and so the default it must
     * respect is already written down.
     */
    val allowDismissSync: Boolean = false,
) {

    /**
     * Whether [packageName] may be mirrored to this computer.
     *
     * Deny by default: an app that has never been named is not shared, and
     * neither is one installed after the grant.
     */
    fun allowsApp(packageName: String): Boolean =
        allowMirror && packageName in allowedApps

    /** Contains no content and no identity; safe to log. */
    fun describe(): String =
        "mirror=${onOff(allowMirror)} apps=${allowedApps.size} " +
            "work-profile=${onOff(includeWorkProfile)} ongoing=${onOff(includeOngoing)} " +
            "locked=$whenSourceLocked dismiss-sync=${onOff(allowDismissSync)}"

    /**
     * Stores the settings, and only the settings.
     *
     * Package names are settings — the user chose them — and no notification
     * content reaches this object or the file it is written to.
     */
    fun toJson(): JSONObject = JSONObject().apply {
        put(KEY_ALLOW_MIRROR, allowMirror)
        put(KEY_ALLOWED_APPS, JSONArray(allowedApps.sorted()))
        put(KEY_INCLUDE_WORK_PROFILE, includeWorkProfile)
        put(KEY_INCLUDE_ONGOING, includeOngoing)
        put(KEY_WHEN_SOURCE_LOCKED, whenSourceLocked.name)
        put(KEY_ALLOW_DISMISS_SYNC, allowDismissSync)
    }

    companion object {
        /** Nothing is permitted, whatever the stored flags say. */
        val DENIED = NotificationPolicy(
            allowMirror = false,
            allowedApps = emptySet(),
            includeWorkProfile = false,
            includeOngoing = false,
            whenSourceLocked = LockPolicy.SUPPRESS,
            allowDismissSync = false,
        )

        private const val KEY_ALLOW_MIRROR = "allowMirror"
        private const val KEY_ALLOWED_APPS = "allowedApps"
        private const val KEY_INCLUDE_WORK_PROFILE = "includeWorkProfile"
        private const val KEY_INCLUDE_ONGOING = "includeOngoing"
        private const val KEY_WHEN_SOURCE_LOCKED = "whenSourceLocked"
        private const val KEY_ALLOW_DISMISS_SYNC = "allowDismissSync"

        /**
         * Reads a stored policy.
         *
         * A trust store written before this capability existed has no policy
         * object at all, and every field falls back to its documented default
         * rather than to `false` — or, for the app list, to "everything". An
         * upgrade must not silently widen what is shared.
         */
        fun fromJson(json: JSONObject?): NotificationPolicy {
            if (json == null) return NotificationPolicy()
            val apps = json.optJSONArray(KEY_ALLOWED_APPS)
            return NotificationPolicy(
                allowMirror = json.optBoolean(KEY_ALLOW_MIRROR, true),
                allowedApps = buildSet {
                    if (apps != null) {
                        for (index in 0 until apps.length()) {
                            apps.optString(index).takeIf { it.isNotEmpty() }?.let { add(it) }
                        }
                    }
                },
                includeWorkProfile = json.optBoolean(KEY_INCLUDE_WORK_PROFILE, false),
                includeOngoing = json.optBoolean(KEY_INCLUDE_ONGOING, false),
                whenSourceLocked = LockPolicy.fromName(
                    json.optString(KEY_WHEN_SOURCE_LOCKED, LockPolicy.DEFAULT.name),
                ),
                allowDismissSync = json.optBoolean(KEY_ALLOW_DISMISS_SYNC, false),
            )
        }

        private fun onOff(value: Boolean) = if (value) "on" else "off"
    }
}
