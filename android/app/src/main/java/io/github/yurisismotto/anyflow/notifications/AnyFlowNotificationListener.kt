package io.github.yurisismotto.anyflow.notifications

import android.app.Notification
import android.app.NotificationManager
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.os.Build
import android.os.Process
import android.provider.Settings
import android.service.notification.NotificationListenerService
import android.service.notification.StatusBarNotification
import android.util.Log
import io.github.yurisismotto.anyflow.AnyFlowApp

/**
 * The platform listener.
 *
 * ## What this class is allowed to do
 *
 * Almost nothing. Its callbacks arrive on the phone's **main thread** — the
 * `NotificationListenerService` javadoc says so from API 24 onward, AOSP
 * VERIFIED — so each one drops AnyFlow's own package, copies a handful of
 * fields into a plain [PlatformNotification], hands it to
 * [NotificationSource] and returns.
 *
 * It performs **no** protobuf encoding, **no** HMAC, **no** package-manager
 * lookup, **no** disk or network I/O, **no** peer enumeration and **no**
 * logging of notification content. Everything that could take a millisecond
 * happens on the capability's own coroutine, because a listener that blocks
 * here stalls the UI of the entire phone.
 *
 * It is deliberately not a god object either: the filter, the identity
 * derivation, the policy, the queue and the wire encoding are separate types
 * with no framework dependency, and they are what the JVM suite tests.
 *
 * ## Holding notification access is not permission to send anything
 *
 * The system may bind this service once the user grants notification access in
 * Settings. That grant alone sends nothing to anyone: with no paired computer
 * there is no session, and with a session but no `notifications.v1` grant the
 * source's authorizer returns [NotificationPolicy.DENIED] and every
 * notification is dropped before it is encoded. Three permissions, checked in
 * three places (ADR-0015 §1, ADR-0017 §6).
 *
 * ## Binding is a function of live peer state
 *
 * `META_DATA_DEFAULT_AUTOBIND` is `false` in the manifest, so the system does
 * not bind this service merely because the app is installed. [NotificationSource]
 * asks for a bind through [NotificationAccess.requestRebind] and releases it
 * through this service's own `requestUnbind`, as granted, mirroring peers come
 * and go — which is what makes "AnyFlow reads your notifications only while a
 * granted computer is connected" structurally true.
 *
 * The two halves are deliberately not the same seam. `requestRebind` is a
 * *static* platform method because it must work when no service instance
 * exists, which is exactly the situation the first bind happens in; a service
 * cannot ask to be created.
 */
class AnyFlowNotificationListener : NotificationListenerService() {

    private val source: NotificationSource?
        get() = (application as? AnyFlowApp)?.notifications

    private val control = object : NotificationSource.ListenerControl {
        override fun requestUnbind() {
            runCatching { this@AnyFlowNotificationListener.requestUnbind() }
        }

        override fun activeNotifications(): List<PlatformNotification>? = runCatching {
            // Active state, never history: the platform defines this as the
            // outstanding notifications visible to the current user, which is
            // what a person sees by picking up the phone.
            activeNotifications?.mapNotNull { extract(it) }
        }.getOrNull()

        override fun activePackages(): List<String>? = runCatching {
            // Names only. Nothing is extracted, so no title or body from the
            // shade is materialised in this process for the picker's sake.
            activeNotifications?.map { it.packageName }?.distinct()
        }.getOrNull()
    }

    override fun onCreate() {
        super.onCreate()
        source?.attachListener(control)
    }

    override fun onDestroy() {
        source?.detachListener()
        super.onDestroy()
    }

    /**
     * The system has bound us. `getActiveNotifications` is now safe to call.
     *
     * This is the resync trigger: the source re-derives its id map over the
     * current shade and sends each connected peer a bracketed snapshot.
     */
    override fun onListenerConnected() {
        source?.attachListener(control)
        source?.onListenerConnected()
    }

    /**
     * We will receive no further events.
     *
     * Fired when the user revokes notification access in Settings, on a
     * session that is otherwise healthy. The source narrows its `SOURCE` role
     * immediately, with a higher epoch, on the connection that is already up —
     * no reconnect, and no window in which a desktop still believes it is
     * being mirrored to.
     */
    override fun onListenerDisconnected() {
        source?.onListenerDisconnected()
    }

    override fun onNotificationPosted(sbn: StatusBarNotification?, rankingMap: RankingMap?) {
        val notification = sbn ?: return
        // FIRST, before extraction and before anything else touches it. A hard
        // loop-prevention rule with no setting that turns it off: this is what
        // stops the ongoing-connection foreground-service notification, which
        // exists on every running install, from being mirrored back to the
        // computer that is displaying it.
        if (notification.packageName == packageName) return

        val extracted = extract(notification, rankingMap) ?: return
        source?.onPosted(extracted)
    }

    override fun onNotificationRemoved(
        sbn: StatusBarNotification?,
        rankingMap: RankingMap?,
        reason: Int,
    ) {
        val notification = sbn ?: return
        if (notification.packageName == packageName) return
        // The removal reason is deliberately not carried further. All 23 mean
        // the same thing to a mirror — it is gone — and there is no reason for
        // which the correct action is to keep showing it. (N4 will consult it
        // locally, for echo suppression, and will still not transmit it.)
        //
        // The `StatusBarNotification` delivered here is explicitly "light" and
        // may be missing heavyweight fields, so nothing but the key is read.
        source?.onRemoved(notification.key)
    }

    /**
     * Copies one notification out of the framework object.
     *
     * Every read is defensive: `extras` is a `Bundle` an arbitrary application
     * filled in, and a malformed or hostile one must produce a notification
     * that is dropped or reduced, never an exception on the main thread. A
     * failure here returns null, which drops the notification — fail closed.
     */
    private fun extract(
        sbn: StatusBarNotification,
        rankingMap: RankingMap? = null,
    ): PlatformNotification? = runCatching {
        val notification: Notification = sbn.notification
        val extras = notification.extras

        val importance = runCatching {
            val ranking = Ranking()
            val map = rankingMap ?: currentRanking
            if (map != null && map.getRanking(sbn.key, ranking)) {
                ranking.importance
            } else {
                PlatformNotification.IMPORTANCE_UNKNOWN
            }
        }.getOrDefault(PlatformNotification.IMPORTANCE_UNKNOWN)

        val progressMax = runCatching { extras.getInt(Notification.EXTRA_PROGRESS_MAX, 0) }
            .getOrDefault(0)
        val progressCurrent = runCatching { extras.getInt(Notification.EXTRA_PROGRESS, 0) }
            .getOrDefault(0)
        val indeterminate =
            runCatching { extras.getBoolean(Notification.EXTRA_PROGRESS_INDETERMINATE, false) }
                .getOrDefault(false)

        PlatformNotification(
            platformKey = sbn.key,
            packageName = sbn.packageName,
            // A boolean, never the numeric user id: the semantic is portable
            // and the number is a fingerprinting surface.
            secondaryProfile = runCatching { sbn.user != Process.myUserHandle() }
                .getOrDefault(false),
            postedAtUnixMs = sbn.postTime,
            ongoing = sbn.isOngoing,
            clearable = sbn.isClearable,
            visibility = notification.visibility,
            androidImportance = importance,
            category = notification.category,
            groupKey = runCatching { sbn.groupKey }.getOrNull(),
            groupSummary = (notification.flags and Notification.FLAG_GROUP_SUMMARY) != 0,
            // `toString()` on the CharSequence flattens any span to plain
            // text. Nothing styled, nothing serialized, no RemoteViews.
            title = readText(extras, Notification.EXTRA_TITLE),
            body = readText(extras, Notification.EXTRA_TEXT),
            hasProgress = progressMax > 0 || indeterminate,
            progressCurrent = progressCurrent,
            progressMax = progressMax,
            progressIndeterminate = indeterminate,
        )
    }.getOrElse { e ->
        // The exception type only. An exception message from a notification
        // extras parser can contain the notification.
        Log.w(TAG, "could not read a notification: ${e.javaClass.simpleName}")
        null
    }

    private fun readText(extras: android.os.Bundle, key: String): String = runCatching {
        extras.getCharSequence(key)?.toString().orEmpty()
    }.getOrDefault("")

    companion object {
        private const val TAG = "AnyFlowListener"
    }
}

/**
 * The two questions the rest of the app asks about notification access.
 *
 * Kept apart from the service so that nothing has to hold a service instance
 * to ask them, and so the component name is written down once.
 */
object NotificationAccess {

    fun component(context: Context): ComponentName =
        ComponentName(context.applicationContext, AnyFlowNotificationListener::class.java)

    /**
     * Whether the user has granted notification access, right now.
     *
     * Read from the platform on every question rather than cached: the user
     * can revoke it in Settings at any moment, and a cached "yes" is exactly
     * the stale answer that would keep a mirror alive after a revocation.
     * Any failure answers **false** — absent, unreadable or ambiguous state is
     * treated as "no permission", never as "permitted" (ADR-0015 §1 clause 6).
     */
    fun isGranted(context: Context): Boolean = runCatching {
        val manager = context.applicationContext
            .getSystemService(NotificationManager::class.java) ?: return false
        manager.isNotificationListenerAccessGranted(component(context))
    }.getOrDefault(false)

    /**
     * Ask the system to bind the listener.
     *
     * `requestRebind` is the only method that is safe to call before
     * `onListenerConnected` or after `onListenerDisconnected`, which is why
     * the bind side of the lifecycle is static and the unbind side is not.
     */
    fun requestRebind(context: Context) {
        runCatching {
            NotificationListenerService.requestRebind(component(context))
        }
    }

    /**
     * Where to send someone who wants to grant or revoke notification access.
     *
     * `ACTION_NOTIFICATION_LISTENER_DETAIL_SETTINGS` plus the component extra
     * lands on **AnyFlow's own switch**, with its own explanation, rather than
     * on a list of every application on the device that a person then has to
     * search. It is API 30; the floor here is 29, so the whole-list action is
     * the documented fallback for that one release.
     *
     * `FLAG_ACTIVITY_NEW_TASK` is not set: this is launched from an Activity
     * the person is looking at, and it must come back to that Activity so the
     * real permission state can be re-read on resume.
     *
     * **AnyFlow never asks for this permission any other way.** There is no
     * dialog that grants it, no accessibility-service workaround, and no
     * `CompanionDeviceManager` association — ADR-0015 §10 records why the last
     * of those is a notification-privacy decision and not a convenience.
     */
    fun settingsIntent(context: Context): Intent {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            val detail = Intent(Settings.ACTION_NOTIFICATION_LISTENER_DETAIL_SETTINGS)
                .putExtra(
                    Settings.EXTRA_NOTIFICATION_LISTENER_COMPONENT_NAME,
                    component(context).flattenToString(),
                )
            if (detail.resolveActivity(context.packageManager) != null) return detail
        }
        return Intent(Settings.ACTION_NOTIFICATION_LISTENER_SETTINGS)
    }

    /**
     * The context-free half of the platform listener, for [NotificationSource].
     *
     * Always available, unlike the service instance: it is what lets the very
     * first bind be requested.
     */
    fun controlFor(context: Context): NotificationSource.AccessControl {
        val appContext = context.applicationContext
        return object : NotificationSource.AccessControl {
            override fun isAccessGranted(): Boolean = isGranted(appContext)
            override fun requestBind() = requestRebind(appContext)
        }
    }
}
