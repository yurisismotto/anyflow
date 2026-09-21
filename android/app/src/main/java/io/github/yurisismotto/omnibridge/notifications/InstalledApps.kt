package io.github.yurisismotto.omnibridge.notifications

import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.graphics.drawable.Drawable
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/**
 * The picker's platform half: asking Android which applications exist.
 *
 * ## No `QUERY_ALL_PACKAGES`
 *
 * The manifest declares a `<queries>` element for `MAIN`/`LAUNCHER`, which is
 * the documented and unrestricted way to see the applications a person can
 * open from their home screen. It is what makes this class work, and it is
 * strictly narrower than the restricted permission: measured on the
 * certification hardware, **97 launchable applications against 501 packages
 * installed**, with every one of the 97 resolving a label and an icon.
 *
 * Packages with no launcher entry — the system UI, a weather daemon,
 * `com.android.shell` — are not enumerated here. They reach the picker only
 * through [notifyingPackages], which reads the names of what is *already in
 * the shade* through the listener the user has separately granted, and which
 * therefore adds no visibility this app did not already have.
 *
 * ## Everything here is a binder call
 *
 * `queryIntentActivities`, `getApplicationLabel` and `getApplicationIcon` all
 * cross into `system_server`. Ninety-seven of them is tens of milliseconds,
 * which is a dropped frame on the main thread, so [load] hops to
 * [Dispatchers.IO] and the icon loader is called from a `LaunchedEffect`
 * rather than during composition.
 */
class InstalledApps(context: Context) {

    private val appContext = context.applicationContext
    private val packageManager: PackageManager get() = appContext.packageManager

    /**
     * The list the picker draws, already merged and sorted.
     *
     * [notifying] is passed in rather than read here because only the
     * notification listener can answer it, and it may legitimately be empty —
     * the listener is not bound when no granted peer is connected, and a
     * picker that refused to open in that state would be useless.
     */
    suspend fun load(
        allowed: Set<String>,
        notifying: Collection<String>,
    ): List<NotificationApp> = withContext(Dispatchers.IO) {
        NotificationApps.build(
            launchable = launchablePackages(),
            notifying = notifying,
            allowed = allowed,
            ownPackage = appContext.packageName,
            labels = ::labelFor,
        )
    }

    /**
     * Every package with a launcher entry.
     *
     * A failure answers **empty** rather than throwing: a picker with no rows
     * and a search box is recoverable, and an exception on the way to a
     * consent screen is not. The empty state says what happened.
     */
    fun launchablePackages(): List<String> = runCatching {
        val intent = Intent(Intent.ACTION_MAIN).addCategory(Intent.CATEGORY_LAUNCHER)
        packageManager
            .queryIntentActivities(intent, 0)
            .map { it.activityInfo.packageName }
            .distinct()
    }.getOrDefault(emptyList())

    /**
     * The label the platform resolves, or the package name.
     *
     * Resolved in **this phone's** locale, deliberately: the desktop has no
     * package database and could not re-resolve it, so a phone in Portuguese
     * shows and sends "Definições" and the desktop shows the same. That is
     * consistency, not a translation bug.
     */
    fun labelFor(packageName: String): String = runCatching {
        packageManager
            .getApplicationLabel(packageManager.getApplicationInfo(packageName, 0))
            .toString()
            .trim()
    }.getOrNull()?.takeIf { it.isNotEmpty() } ?: packageName

    /**
     * The application's icon, or null.
     *
     * Loaded locally and never transmitted: `notifications.v1` has no field
     * that could carry an image, and none is added for this. Nothing is
     * written to disk — the platform's own icon cache is behind this call and
     * is the only cache involved.
     */
    suspend fun iconFor(packageName: String): Drawable? = withContext(Dispatchers.IO) {
        runCatching { packageManager.getApplicationIcon(packageName) }.getOrNull()
    }
}
