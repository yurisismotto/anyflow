package io.github.yurisismotto.omnibridge.notifications

/**
 * One application, as the picker offers it.
 *
 * Everything here is *metadata about an application*: a package name, the
 * label the platform resolves for it, and how it came to be in the list.
 * **No field can hold a notification**, and nothing on this type is ever sent
 * to a peer — the picker's output is a set of package names in the local
 * policy, and package names are settings the person chose.
 */
data class NotificationApp(
    val packageName: String,
    /**
     * What the platform calls it, in *this phone's* locale.
     *
     * Falls back to [packageName] when the label cannot be resolved:
     * `com.example.chat` is worse than "Chat" and far better than a blank row
     * a person cannot identify.
     */
    val label: String,
    /** It has a launcher entry, so the person sees it on their home screen. */
    val launchable: Boolean,
    /**
     * It is in the notification shade right now.
     *
     * The only source that can name a package with no launcher entry, and it
     * costs nothing: the listener is already bound and already reading these
     * notifications. Only the package name is taken.
     */
    val notifying: Boolean,
    /** It is already in this peer's allow-list. */
    val allowed: Boolean,
)

/**
 * How the picker's list is built, and what may be in it.
 *
 * A pure object with no Android types, so every rule below is decided on the
 * JVM in a test rather than argued about. The platform half — asking the
 * package manager and the listener — is [InstalledApps], which does the I/O
 * and then calls straight into here.
 *
 * ## Where the names come from, and why not `QUERY_ALL_PACKAGES`
 *
 * Three sources, unioned, in this order of preference:
 *
 * | Source | What it contributes | What it costs |
 * | --- | --- | --- |
 * | Launcher entries | every application the person can open from their home screen | a `<queries>` element in the manifest — the documented, unrestricted mechanism |
 * | The notification shade | packages that post notifications but have no launcher entry | nothing: the listener is already bound and already reading them |
 * | The allow-list itself | an application the person already chose | nothing |
 *
 * The third source is not redundant. A package that is chosen and then
 * uninstalled, disabled, or moved out of the launcher would otherwise vanish
 * from the picker while remaining in the stored policy — the person would be
 * sharing something they could no longer see or switch off.
 *
 * ## The one package that is never in the list
 *
 * OmniBridge's own. Not as a default and not as a filter the user can undo:
 * [build] removes it, and the source drops its own package before any filter
 * runs anyway ([NotificationFilter.screen]), so selecting it would be inert
 * even if it could be selected. Two independent mechanisms, because this one
 * is what stops the mirror-of-a-mirror loop.
 */
object NotificationApps {

    /**
     * Builds the picker's list.
     *
     * Sorted by label, case-insensitively and stably, because a picker whose
     * order changes between openings is one a person cannot learn. Sorting by
     * "allowed first" was considered and rejected: the row a person is looking
     * for would move the moment they tapped it.
     */
    fun build(
        launchable: Collection<String>,
        notifying: Collection<String>,
        allowed: Set<String>,
        ownPackage: String,
        labels: (String) -> String,
    ): List<NotificationApp> {
        val launchableSet = launchable.toSet()
        val notifyingSet = notifying.toSet()
        val names = LinkedHashSet<String>().apply {
            addAll(launchableSet)
            addAll(notifyingSet)
            addAll(allowed)
        }
        return names
            .asSequence()
            // Hard rule, and the reason it is applied here rather than in a
            // filter the caller could forget: a list that cannot contain this
            // package cannot offer it.
            .filter { it != ownPackage }
            .filter { it.isNotBlank() }
            .map { name ->
                NotificationApp(
                    packageName = name,
                    label = labels(name).ifBlank { name },
                    launchable = name in launchableSet,
                    notifying = name in notifyingSet,
                    allowed = name in allowed,
                )
            }
            .sortedWith(
                compareBy(String.CASE_INSENSITIVE_ORDER) { it.label },
            )
            .toList()
    }

    /**
     * Narrows the list to a search term.
     *
     * Matches the label *and* the package name, because both are on screen and
     * a person who knows one may not know the other. Case- and
     * accent-insensitive on the label only for the simple case: a locale-aware
     * collator would be better and is not worth a dependency for a list of a
     * hundred rows.
     */
    fun search(apps: List<NotificationApp>, query: String): List<NotificationApp> {
        val needle = query.trim()
        if (needle.isEmpty()) return apps
        return apps.filter {
            it.label.contains(needle, ignoreCase = true) ||
                it.packageName.contains(needle, ignoreCase = true)
        }
    }

    /**
     * Applications that appeared since the person last looked at this picker.
     *
     * Deny-by-default already covers the *security* half: an application
     * installed after the choice is simply not in [NotificationPolicy.allowedApps]
     * and is not shared. This covers the *discoverability* half, which is
     * otherwise a person wondering why their new messaging app is not
     * mirroring.
     *
     * It is passive text and never a notification — a notification about
     * notifications would be the one interruption this feature must not
     * create. It is also empty until a baseline exists ([NotificationPolicy.knownApps]),
     * so a person who has never opened the picker is not told that all ninety-
     * seven of their applications are "new".
     */
    fun newlyInstalled(
        apps: List<NotificationApp>,
        known: Set<String>,
    ): List<NotificationApp> {
        if (known.isEmpty()) return emptyList()
        return apps.filter { it.packageName !in known && !it.allowed }
    }

    /** How many of [apps] are shared. Counts, for the summary line. */
    fun allowedCount(apps: List<NotificationApp>): Int = apps.count { it.allowed }
}
