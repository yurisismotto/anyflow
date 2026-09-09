package io.github.yurisismotto.anyflow

import io.github.yurisismotto.anyflow.notifications.DropReason
import io.github.yurisismotto.anyflow.notifications.FilterVerdict
import io.github.yurisismotto.anyflow.notifications.LockPolicy
import io.github.yurisismotto.anyflow.notifications.NotificationFilter
import io.github.yurisismotto.anyflow.notifications.NotificationMapping
import io.github.yurisismotto.anyflow.notifications.NotificationPolicy
import io.github.yurisismotto.anyflow.notifications.PlatformNotification
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Which notifications may leave this phone, and for whom.
 *
 * Deny by default is the property this file exists to pin. On the
 * certification hardware Android's own OTP redaction did not fire at all
 * (POC-NOTIF-01), which makes the per-app allow-list the only effective
 * control there is — so every test below that asserts a drop is asserting the
 * thing that stops a 2FA code reaching a screen in an office.
 */
class NotificationFilterTest {

    private val ownPackage = "io.github.yurisismotto.anyflow"

    private fun notification(
        packageName: String = "example.fixture.app",
        visibility: Int = NotificationMapping.VISIBILITY_PRIVATE,
        importance: Int = NotificationMapping.ANDROID_IMPORTANCE_DEFAULT,
        ongoing: Boolean = false,
        secondaryProfile: Boolean = false,
        category: String? = null,
    ) = PlatformNotification(
        platformKey = "0|$packageName|1|null|10123",
        packageName = packageName,
        secondaryProfile = secondaryProfile,
        postedAtUnixMs = 1_700_000_000_000L,
        ongoing = ongoing,
        clearable = !ongoing,
        visibility = visibility,
        androidImportance = importance,
        category = category,
        groupKey = null,
        groupSummary = false,
        title = "FIXTURE TITLE",
        body = "FIXTURE BODY",
        hasProgress = false,
        progressCurrent = 0,
        progressMax = 0,
        progressIndeterminate = false,
    )

    private fun allowing(vararg apps: String) =
        NotificationPolicy(allowedApps = apps.toSet())

    private fun reasonOf(verdict: FilterVerdict): DropReason? =
        (verdict as? FilterVerdict.Drop)?.reason

    // -- the hard rules ------------------------------------------------------

    /**
     * **AnyFlow's own package is never mirrored.** A hard loop-prevention rule
     * and not a preference: it is what stops the ongoing-connection
     * foreground-service notification — which exists on every running install
     * — from being mirrored back to the computer displaying it.
     */
    @Test
    fun `the own package is dropped`() {
        assertEquals(
            DropReason.OWN_PACKAGE,
            reasonOf(NotificationFilter.screen(notification(ownPackage), ownPackage)),
        )
    }

    /**
     * And no setting can override it. Explicitly allowing our own package, on
     * a policy with everything else turned on, still drops it.
     */
    @Test
    fun `the own package cannot be allowed by any policy`() {
        val permissive = NotificationPolicy(
            allowMirror = true,
            allowedApps = setOf(ownPackage, "example.fixture.app"),
            includeWorkProfile = true,
            includeOngoing = true,
            whenSourceLocked = LockPolicy.FULL,
            allowDismissSync = true,
        )
        assertEquals(
            DropReason.OWN_PACKAGE,
            reasonOf(
                NotificationFilter.decide(
                    notification(ownPackage),
                    ownPackage,
                    permissive,
                    sourceLocked = false,
                ),
            ),
        )
    }

    /** The own-package rule is applied before anything else can matter. */
    @Test
    fun `the own package is dropped before every other check`() {
        // Ongoing, secret, importance none, work profile: every other rule
        // would also drop it, and the reason reported is still the first one.
        val worst = PlatformNotification(
            platformKey = "10|$ownPackage|1|null|10123",
            packageName = ownPackage,
            secondaryProfile = true,
            postedAtUnixMs = 0,
            ongoing = true,
            clearable = false,
            visibility = NotificationMapping.VISIBILITY_SECRET,
            androidImportance = NotificationMapping.ANDROID_IMPORTANCE_NONE,
            category = "sys",
            groupKey = null,
            groupSummary = false,
            title = "",
            body = "",
            hasProgress = false,
            progressCurrent = 0,
            progressMax = 0,
            progressIndeterminate = false,
        )
        assertEquals(
            DropReason.OWN_PACKAGE,
            reasonOf(NotificationFilter.screen(worst, ownPackage)),
        )
    }

    /**
     * `VISIBILITY_SECRET` is never mirrored, under any policy. An app that
     * said "do not show this even on a lock screen" has said something clear
     * enough that forwarding it to another machine cannot be right.
     */
    @Test
    fun `a secret notification is never transmitted`() {
        assertEquals(
            DropReason.SECRET_VISIBILITY,
            reasonOf(
                NotificationFilter.decide(
                    notification(visibility = NotificationMapping.VISIBILITY_SECRET),
                    ownPackage,
                    NotificationPolicy(
                        allowedApps = setOf("example.fixture.app"),
                        whenSourceLocked = LockPolicy.FULL,
                    ),
                    sourceLocked = false,
                ),
            ),
        )
    }

    /** A notification the phone does not show its owner is not a banner. */
    @Test
    fun `importance none is dropped`() {
        assertEquals(
            DropReason.IMPORTANCE_NONE,
            reasonOf(
                NotificationFilter.screen(
                    notification(importance = NotificationMapping.ANDROID_IMPORTANCE_NONE),
                    ownPackage,
                ),
            ),
        )
    }

    // -- deny by default -----------------------------------------------------

    /**
     * The default policy shares nothing. This is the test the whole capability
     * rests on: a computer that was just granted `notifications.v1` receives
     * no notification at all until a person names an application.
     */
    @Test
    fun `the default policy mirrors nothing`() {
        val default = NotificationPolicy()
        assertTrue(default.allowedApps.isEmpty())
        assertEquals(
            DropReason.NOT_ALLOWED_APP,
            reasonOf(
                NotificationFilter.decide(
                    notification(),
                    ownPackage,
                    default,
                    sourceLocked = false,
                ),
            ),
        )
    }

    /** An ungranted or forgotten peer resolves to DENIED and receives nothing. */
    @Test
    fun `an ungranted peer receives nothing`() {
        assertEquals(
            DropReason.NOT_GRANTED,
            reasonOf(
                NotificationFilter.decide(
                    notification(),
                    ownPackage,
                    NotificationPolicy.DENIED,
                    sourceLocked = false,
                ),
            ),
        )
    }

    @Test
    fun `a named application is mirrored`() {
        assertEquals(
            FilterVerdict.Allow,
            NotificationFilter.decide(
                notification(),
                ownPackage,
                allowing("example.fixture.app"),
                sourceLocked = false,
            ),
        )
    }

    /**
     * An app installed after the grant is not shared, and it needs no rule of
     * its own: it is simply not in the set. AnyFlow does not retroactively
     * widen a decision the user made about a different set of apps.
     */
    @Test
    fun `a newly installed app is not shared`() {
        val policy = allowing("example.fixture.app")
        assertEquals(
            DropReason.NOT_ALLOWED_APP,
            reasonOf(
                NotificationFilter.decide(
                    notification("example.newly.installed"),
                    ownPackage,
                    policy,
                    sourceLocked = false,
                ),
            ),
        )
    }

    /**
     * A system package is denied for exactly the reason every other package
     * is: the list starts empty. There is no classification step that could be
     * wrong in either direction.
     */
    @Test
    fun `a system package is denied by default`() {
        assertEquals(
            DropReason.NOT_ALLOWED_APP,
            reasonOf(
                NotificationFilter.decide(
                    notification("com.android.systemui", category = "sys"),
                    ownPackage,
                    NotificationPolicy(),
                    sourceLocked = false,
                ),
            ),
        )
    }

    /**
     * **Category is never an ACL.** An app chooses its own category, so
     * `CATEGORY_SYSTEM` is a claim by an arbitrary application; it grants
     * nothing and denies nothing.
     */
    @Test
    fun `category grants nothing and denies nothing`() {
        for (category in listOf(null, "sys", "msg", "call", "err", "unknown-future")) {
            assertEquals(
                "category $category must not deny an allowed app",
                FilterVerdict.Allow,
                NotificationFilter.decide(
                    notification(category = category),
                    ownPackage,
                    allowing("example.fixture.app"),
                    sourceLocked = false,
                ),
            )
            assertEquals(
                "category $category must not admit an unnamed app",
                DropReason.NOT_ALLOWED_APP,
                reasonOf(
                    NotificationFilter.decide(
                        notification("example.unnamed.app", category = category),
                        ownPackage,
                        allowing("example.fixture.app"),
                        sourceLocked = false,
                    ),
                ),
            )
        }
    }

    /**
     * Nothing in the filter looks at text. Asserted by giving two
     * notifications from the same app radically different content and getting
     * the same verdict — there is no OTP regex, no keyword list and no
     * "looks like a bank" guess to find.
     */
    @Test
    fun `no rule inspects the notification text`() {
        val policy = allowing("example.fixture.app")
        val plain = notification()
        val otpShaped = PlatformNotification(
            platformKey = plain.platformKey,
            packageName = plain.packageName,
            secondaryProfile = false,
            postedAtUnixMs = plain.postedAtUnixMs,
            ongoing = false,
            clearable = true,
            visibility = plain.visibility,
            androidImportance = plain.androidImportance,
            category = plain.category,
            groupKey = null,
            groupSummary = false,
            title = "FIXTURE BANK",
            body = "FIXTURE CODE 000000",
            hasProgress = false,
            progressCurrent = 0,
            progressMax = 0,
            progressIndeterminate = false,
        )
        assertEquals(
            NotificationFilter.decide(plain, ownPackage, policy, false),
            NotificationFilter.decide(otpShaped, ownPackage, policy, false),
        )
    }

    // -- the separate switches -----------------------------------------------

    /**
     * A work-profile notification needs its own switch, independent of the app
     * list: a person who shares "Slack" from their personal profile has not
     * thereby asked to share work Slack.
     */
    @Test
    fun `a secondary profile notification is denied by default`() {
        assertEquals(
            DropReason.SECONDARY_PROFILE,
            reasonOf(
                NotificationFilter.decide(
                    notification(secondaryProfile = true),
                    ownPackage,
                    allowing("example.fixture.app"),
                    sourceLocked = false,
                ),
            ),
        )
    }

    @Test
    fun `a secondary profile notification passes only with the switch on`() {
        assertEquals(
            FilterVerdict.Allow,
            NotificationFilter.decide(
                notification(secondaryProfile = true),
                ownPackage,
                allowing("example.fixture.app").copy(includeWorkProfile = true),
                sourceLocked = false,
            ),
        )
    }

    /** And the switch alone does not admit an app the list does not name. */
    @Test
    fun `the work profile switch does not bypass the app list`() {
        assertEquals(
            DropReason.NOT_ALLOWED_APP,
            reasonOf(
                NotificationFilter.decide(
                    notification("example.work.app", secondaryProfile = true),
                    ownPackage,
                    allowing("example.fixture.app").copy(includeWorkProfile = true),
                    sourceLocked = false,
                ),
            ),
        )
    }

    @Test
    fun `an ongoing notification is denied by default`() {
        assertEquals(
            DropReason.ONGOING,
            reasonOf(
                NotificationFilter.decide(
                    notification(ongoing = true),
                    ownPackage,
                    allowing("example.fixture.app"),
                    sourceLocked = false,
                ),
            ),
        )
    }

    @Test
    fun `mirroring switched off denies an otherwise allowed app`() {
        assertEquals(
            DropReason.MIRRORING_OFF,
            reasonOf(
                NotificationFilter.decide(
                    notification(),
                    ownPackage,
                    allowing("example.fixture.app").copy(allowMirror = false),
                    sourceLocked = false,
                ),
            ),
        )
    }

    // -- lock policy ---------------------------------------------------------

    @Test
    fun `a locked source with the suppress policy sends nothing`() {
        assertEquals(
            DropReason.LOCK_SUPPRESSED,
            reasonOf(
                NotificationFilter.decide(
                    notification(),
                    ownPackage,
                    allowing("example.fixture.app").copy(whenSourceLocked = LockPolicy.SUPPRESS),
                    sourceLocked = true,
                ),
            ),
        )
    }

    @Test
    fun `an unlocked source ignores the lock policy`() {
        assertEquals(
            FilterVerdict.Allow,
            NotificationFilter.decide(
                notification(),
                ownPackage,
                allowing("example.fixture.app").copy(whenSourceLocked = LockPolicy.SUPPRESS),
                sourceLocked = false,
            ),
        )
    }

    // -- policy helpers ------------------------------------------------------

    @Test
    fun `allowsApp is false for everything under the DENIED policy`() {
        assertFalse(NotificationPolicy.DENIED.allowsApp("example.fixture.app"))
        assertFalse(NotificationPolicy.DENIED.allowsApp(ownPackage))
    }

    /** The description a log line uses names counts, never package contents. */
    @Test
    fun `the policy description carries no notification content`() {
        val described = allowing("example.fixture.app", "example.other.app").describe()
        assertTrue(described.contains("apps=2"))
        assertTrue(described.contains("mirror=on"))
    }
}
