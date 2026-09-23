package io.github.yurisismotto.omnibridge

import io.github.yurisismotto.omnibridge.notifications.NotificationApps
import io.github.yurisismotto.omnibridge.notifications.NotificationPolicy
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The app picker's list, and what can and cannot be in it.
 *
 * The picker is the largest usability gap N2 left and the largest new attack
 * surface N3 adds, so the rules that bound it are pinned here rather than
 * inspected in a screenshot: OmniBridge's own package can never appear, nothing
 * is selected without being named, and an application that was chosen and
 * then disappeared from the launcher is still visible so it can be switched
 * off.
 */
class NotificationAppsTest {

    private val own = "io.github.yurisismotto.omnibridge"

    private val labels: (String) -> String = { pkg ->
        when (pkg) {
            "com.example.chat" -> "Chat"
            "com.example.bank" -> "Banco"
            "com.example.ongoing" -> "Weather"
            else -> ""
        }
    }

    @Test
    fun `the own package is never in the list`() {
        val apps = NotificationApps.build(
            launchable = listOf(own, "com.example.chat"),
            notifying = listOf(own),
            allowed = setOf(own),
            ownPackage = own,
            labels = labels,
        )
        assertEquals(listOf("com.example.chat"), apps.map { it.packageName })
    }

    @Test
    fun `the own package cannot be smuggled in through a stored allow-list`() {
        // A trust store edited by hand, or written by a future bug. The list
        // is where the rule is applied, so there is no row to tick.
        val apps = NotificationApps.build(
            launchable = emptyList(),
            notifying = emptyList(),
            allowed = setOf(own),
            ownPackage = own,
            labels = labels,
        )
        assertTrue(apps.isEmpty())
    }

    @Test
    fun `nothing is selected unless it is in the allow-list`() {
        val apps = NotificationApps.build(
            launchable = listOf("com.example.chat", "com.example.bank"),
            notifying = listOf("com.example.chat"),
            allowed = emptySet(),
            ownPackage = own,
            labels = labels,
        )
        assertEquals(2, apps.size)
        assertTrue(apps.none { it.allowed })
        assertEquals(0, NotificationApps.allowedCount(apps))
    }

    @Test
    fun `a package with no launcher entry appears only while it is notifying`() {
        // `com.android.shell` and the system UI are the real cases. They are
        // not enumerated by the manifest's `<queries>` element, so the shade
        // is the only thing that can name them — and only while they are in it.
        val idle = NotificationApps.build(
            launchable = listOf("com.example.chat"),
            notifying = emptyList(),
            allowed = emptySet(),
            ownPackage = own,
            labels = labels,
        )
        assertFalse(idle.any { it.packageName == "com.android.shell" })

        val active = NotificationApps.build(
            launchable = listOf("com.example.chat"),
            notifying = listOf("com.android.shell"),
            allowed = emptySet(),
            ownPackage = own,
            labels = labels,
        )
        val shell = active.first { it.packageName == "com.android.shell" }
        assertTrue(shell.notifying)
        assertFalse(shell.launchable)
    }

    @Test
    fun `an already chosen app stays visible after it leaves the launcher`() {
        // Otherwise a person would be sharing something they could no longer
        // see, and could not switch off without clearing everything.
        val apps = NotificationApps.build(
            launchable = listOf("com.example.chat"),
            notifying = emptyList(),
            allowed = setOf("com.example.gone"),
            ownPackage = own,
            labels = labels,
        )
        val gone = apps.first { it.packageName == "com.example.gone" }
        assertTrue(gone.allowed)
        assertFalse(gone.launchable)
        // No label resolves for an app that is not there; the package name is
        // shown rather than a blank row.
        assertEquals("com.example.gone", gone.label)
    }

    @Test
    fun `an unresolvable label falls back to the package name`() {
        val apps = NotificationApps.build(
            launchable = listOf("com.example.unknown"),
            notifying = emptyList(),
            allowed = emptySet(),
            ownPackage = own,
            labels = { "" },
        )
        assertEquals("com.example.unknown", apps.single().label)
    }

    @Test
    fun `the order is by label and does not move when something is ticked`() {
        // A picker whose rows reorder as they are tapped is one nobody can
        // work through. "Allowed first" was considered and rejected for
        // exactly that reason.
        val before = NotificationApps.build(
            launchable = listOf("com.example.chat", "com.example.bank"),
            notifying = emptyList(),
            allowed = emptySet(),
            ownPackage = own,
            labels = labels,
        )
        val after = NotificationApps.build(
            launchable = listOf("com.example.chat", "com.example.bank"),
            notifying = emptyList(),
            allowed = setOf("com.example.chat"),
            ownPackage = own,
            labels = labels,
        )
        assertEquals(listOf("Banco", "Chat"), before.map { it.label })
        assertEquals(before.map { it.packageName }, after.map { it.packageName })
    }

    @Test
    fun `search matches the label and the package name`() {
        val apps = NotificationApps.build(
            launchable = listOf("com.example.chat", "com.example.bank"),
            notifying = emptyList(),
            allowed = emptySet(),
            ownPackage = own,
            labels = labels,
        )
        assertEquals(listOf("Chat"), NotificationApps.search(apps, "cha").map { it.label })
        // Two applications can carry the same label; the package name is how
        // a person tells them apart, so it has to be searchable too.
        assertEquals(
            listOf("Banco"),
            NotificationApps.search(apps, "example.bank").map { it.label },
        )
        assertEquals(apps, NotificationApps.search(apps, "   "))
        assertTrue(NotificationApps.search(apps, "zzz").isEmpty())
    }

    @Test
    fun `no application is new until a baseline exists`() {
        // A fresh install announcing that all ninety-seven of its apps are new
        // would be noise, and would train people to ignore the line that
        // matters later.
        val apps = NotificationApps.build(
            launchable = listOf("com.example.chat", "com.example.bank"),
            notifying = emptyList(),
            allowed = emptySet(),
            ownPackage = own,
            labels = labels,
        )
        assertTrue(NotificationApps.newlyInstalled(apps, known = emptySet()).isEmpty())
    }

    @Test
    fun `an application installed after the last look is reported as new`() {
        val apps = NotificationApps.build(
            launchable = listOf("com.example.chat", "com.example.bank"),
            notifying = emptyList(),
            allowed = emptySet(),
            ownPackage = own,
            labels = labels,
        )
        val fresh = NotificationApps.newlyInstalled(apps, known = setOf("com.example.chat"))
        assertEquals(listOf("com.example.bank"), fresh.map { it.packageName })
    }

    @Test
    fun `being reported as new never shares anything`() {
        // The line is discoverability, not policy. Deny-by-default is what
        // actually stops the notification, and it is unchanged.
        val policy = NotificationPolicy(allowedApps = setOf("com.example.chat"))
        assertFalse(policy.allowsApp("com.example.bank"))
    }

    @Test
    fun `a newly chosen application stops being counted as new`() {
        val apps = NotificationApps.build(
            launchable = listOf("com.example.chat", "com.example.bank"),
            notifying = emptyList(),
            allowed = setOf("com.example.bank"),
            ownPackage = own,
            labels = labels,
        )
        assertTrue(
            NotificationApps.newlyInstalled(apps, known = setOf("com.example.chat")).isEmpty(),
        )
    }

    @Test
    fun `a blank package name is never a row`() {
        val apps = NotificationApps.build(
            launchable = listOf("", "  "),
            notifying = listOf(""),
            allowed = setOf(""),
            ownPackage = own,
            labels = labels,
        )
        assertTrue(apps.isEmpty())
    }
}
