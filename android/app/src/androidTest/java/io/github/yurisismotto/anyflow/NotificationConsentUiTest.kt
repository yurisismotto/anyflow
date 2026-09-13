package io.github.yurisismotto.anyflow

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.test.SemanticsNodeInteraction
import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertIsNotSelected
import androidx.compose.ui.test.assertIsOff
import androidx.compose.ui.test.assertIsOn
import androidx.compose.ui.test.assertIsSelected
import androidx.compose.ui.test.filterToOne
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isToggleable
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.onSiblings
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.test.ext.junit.runners.AndroidJUnit4
import io.github.yurisismotto.anyflow.NotificationUiFixtures as Fx
import io.github.yurisismotto.anyflow.notifications.LockPolicy
import io.github.yurisismotto.anyflow.notifications.NotificationPolicy
import io.github.yurisismotto.anyflow.ui.NotificationSettingsScreen
import io.github.yurisismotto.anyflow.ui.theme.AnyFlowTheme
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/**
 * The per-device notification consent screen.
 *
 * The properties pinned here are the ones a screenshot cannot pin: that the
 * grant is off on a device nobody has allowed, that the three gates are shown
 * as three things, that no state claims to be mirroring while one of them is
 * shut, and that there is no working dismissal control to imply a feature this
 * release does not have.
 *
 * ## The semantics contract these tests are written against
 *
 * A capability row is a title, a description and a `Switch`. The title and the
 * description are text; the **switch** is the control and the only node that
 * carries a `ToggleableState`, and it carries the row's title as its
 * `contentDescription` so it is never announced as a contextless "switch, on".
 * [switchFor] is therefore how every on/off assertion and every press reaches
 * it — `hasText(title)` reaches the *label*, which has no state and no click.
 *
 * The screen is also **taller than any device it runs on**, and its own scroll
 * container is the only way to the bottom of it: anything below the status
 * card has to be scrolled to before it can be asserted displayed.
 */
@RunWith(AndroidJUnit4::class)
class NotificationConsentUiTest {

    @get:Rule
    val compose = createComposeRule()

    private fun show(
        state: io.github.yurisismotto.anyflow.ui.MainUiState,
        recorder: Fx.Recorder,
        onOpenAppPicker: (String) -> Unit = {},
    ) {
        compose.setContent {
            AnyFlowTheme {
                NotificationSettingsScreen(
                    state = state,
                    actions = recorder.actions(),
                    fingerprintHex = Fx.PEER_HEX,
                    onOpenAppPicker = onOpenAppPicker,
                )
            }
        }
    }

    /** The switch of the capability row titled [title] — the actual control. */
    private fun switchFor(title: String): SemanticsNodeInteraction =
        compose.onNode(
            isToggleable() and hasContentDescription(title),
            useUnmergedTree = true,
        )

    /**
     * The status badge reading [label].
     *
     * A badge publishes its state as a `contentDescription`, which is what
     * separates a *gate's own state* from the plain-text line in **Details**
     * that repeats it for somebody debugging two devices. Both are supposed to
     * be on the screen; only one of them is a gate.
     */
    private fun badge(label: String): SemanticsNodeInteraction =
        compose.onNode(hasContentDescription(label))

    // --- the semantics contract itself -------------------------------------

    @Test
    fun a_capability_row_reads_as_a_row_and_the_switch_stays_targetable() {
        // Pinned so the arrangement cannot regress into "the whole row is the
        // switch": the description under the title is long, and a row-sized
        // touch target would make it far too easy to grant by brushing past.
        val recorder = Fx.Recorder()
        show(Fx.state(Fx.peer(granted = true)), recorder)

        // Title and description are read as text…
        compose.onNodeWithText("Share notifications with this computer").assertIsDisplayed()
        compose.onNodeWithText("Off until you turn it on", substring = true).assertIsDisplayed()
        // …the label itself is not a control…
        compose
            .onAllNodes(isToggleable() and hasText("Share notifications with this computer"))
            .assertCountEquals(0)
        // …and there is exactly one switch, carrying the row's name.
        compose
            .onAllNodes(
                isToggleable() and hasContentDescription("Share notifications with this computer"),
            )
            .assertCountEquals(1)
        switchFor("Share notifications with this computer").assertIsOn().performClick()
        assertEquals(listOf(false), recorder.grants)
    }

    // --- gate 2: the per-peer grant ---------------------------------------

    @Test
    fun a_paired_computer_starts_with_notifications_off() {
        val recorder = Fx.Recorder()
        show(Fx.state(Fx.peer(granted = false)), recorder)

        compose.onNodeWithText("Share notifications with this computer").assertIsDisplayed()
        switchFor("Share notifications with this computer").assertIsOff()
        compose.onNodeWithText("Off", substring = false).assertIsDisplayed()
        // Nothing was written merely by looking at the screen.
        assertTrue(recorder.grants.isEmpty())
        assertTrue(recorder.policies.isEmpty())
    }

    @Test
    fun an_ungranted_computer_is_offered_no_setting_it_could_not_honour() {
        show(Fx.state(Fx.peer(granted = false)), Fx.Recorder())
        // The lock policy, the pause switch and the app row would all be inert
        // without the grant, so none of them is on screen to be fiddled with.
        compose.onAllNodesWithText("Share the app name only").assertCountEquals(0)
        compose.onAllNodesWithText("Apps").assertCountEquals(0)
    }

    @Test
    fun turning_the_switch_on_writes_the_grant_and_selects_no_application() {
        val recorder = Fx.Recorder()
        show(Fx.state(Fx.peer(granted = false)), recorder)

        switchFor("Share notifications with this computer").assertIsOff().performClick()
        assertEquals(listOf(true), recorder.grants)
        // The critical N3 property: granting is not selecting. Nothing in the
        // grant path writes an application into the policy.
        assertTrue(
            "granting an application-less computer wrote a policy",
            recorder.policies.none { it.allowedApps.isNotEmpty() },
        )
    }

    // --- gate 1: Android's own permission ----------------------------------

    @Test
    fun android_access_is_shown_as_its_own_row_with_its_own_state() {
        show(Fx.state(Fx.peer(granted = true), accessGranted = true), Fx.Recorder())

        // Its own row, with its own heading…
        compose.onNodeWithText("Notification access").performScrollTo().assertIsDisplayed()
        // …and its own state. "Allowed" is on this screen twice on purpose —
        // here, and again in Details — so the assertion is scoped to this
        // gate's status badge rather than to the bare word, which would be
        // asserting the Details line away.
        badge("Allowed").assertIsDisplayed()
        // Independent of the per-computer grant, which has a state of its own
        // and is not what this row is reporting.
        switchFor("Share notifications with this computer").assertIsOn()
        // And the Details line, which repeats the same state for somebody
        // debugging two devices, is deliberately still there: this test is
        // about the gate having a state of its own, not about the word
        // appearing exactly once.
        compose
            .onNodeWithText("Android notification access")
            .performScrollTo()
            .assertIsDisplayed()
    }

    @Test
    fun a_missing_android_permission_is_named_and_not_folded_into_the_grant() {
        show(
            Fx.state(
                Fx.peer(granted = true),
                accessGranted = false,
                status = Fx.idleStatus(accessGranted = false),
            ),
            Fx.Recorder(),
        )
        // The gate's own badge says the permission is missing…
        badge("Not allowed").performScrollTo().assertIsDisplayed()
        // …while the per-computer grant beside it is still on. Two gates, two
        // states, neither folded into the other.
        switchFor("Share notifications with this computer").assertIsOn()
        // The headline names the permission rather than blaming the grant.
        compose.onNodeWithText("Needs Android access").performScrollTo().assertIsDisplayed()
        // And it must never read as though sharing were working.
        compose.onAllNodesWithText("Ready").assertCountEquals(0)
    }

    @Test
    fun the_permission_copy_never_claims_that_allowing_access_shares_anything() {
        show(Fx.state(Fx.peer(granted = true), accessGranted = false), Fx.Recorder())
        compose
            .onNodeWithText(
                "It does not share them with any computer on its own",
                substring = true,
            )
            .performScrollTo()
            .assertIsDisplayed()
    }

    @Test
    fun the_only_way_to_the_permission_is_androids_own_screen() {
        val recorder = Fx.Recorder()
        show(Fx.state(Fx.peer(granted = true), accessGranted = false), recorder)

        compose.onNodeWithText("Open Android settings").performScrollTo().performClick()
        assertEquals(1, recorder.settingsOpened)
        // Pressing it grants nothing here: the permission is Android's, and
        // the real state is re-read when the person comes back.
        assertTrue(recorder.grants.isEmpty())
    }

    // --- gate 3, and the state it produces ---------------------------------

    @Test
    fun a_granted_computer_with_no_application_chosen_shares_nothing_and_says_so() {
        show(Fx.state(Fx.peer(granted = true)), Fx.Recorder())
        compose.onNodeWithText("No apps chosen").assertIsDisplayed()
        compose
            .onNodeWithText("no app has been chosen yet", substring = true)
            .assertIsDisplayed()
        compose.onNodeWithText("No app chosen").performScrollTo().assertIsDisplayed()
        compose.onAllNodesWithText("Ready").assertCountEquals(0)
    }

    @Test
    fun a_fully_set_up_computer_reads_as_ready() {
        show(
            Fx.state(
                Fx.peer(
                    granted = true,
                    policy = NotificationPolicy(allowedApps = setOf("com.example.chat")),
                ),
            ),
            Fx.Recorder(),
        )
        compose.onNodeWithText("Ready").assertIsDisplayed()
    }

    @Test
    fun a_computer_that_has_not_claimed_it_can_display_is_named_as_such() {
        // The state N2 measured cross-device. It is neither "on" nor "off",
        // and a screen with one switch could not have described it.
        show(
            Fx.state(
                Fx.peer(
                    granted = true,
                    policy = NotificationPolicy(allowedApps = setOf("com.example.chat")),
                ),
                status = Fx.sourcingStatus(peerIsSink = false),
            ),
            Fx.Recorder(),
        )
        compose.onNodeWithText("Computer is not receiving").assertIsDisplayed()
        compose.onAllNodesWithText("Ready").assertCountEquals(0)
    }

    @Test
    fun a_disconnected_computer_is_not_connected_rather_than_off() {
        show(
            Fx.state(
                Fx.peer(
                    granted = true,
                    policy = NotificationPolicy(allowedApps = setOf("com.example.chat")),
                ),
                status = Fx.idleStatus(),
            ),
            Fx.Recorder(),
        )
        compose.onNodeWithText("Not connected").assertIsDisplayed()
        compose.onAllNodesWithText("Off").assertCountEquals(0)
    }

    @Test
    fun revoking_android_access_while_the_screen_is_open_changes_what_it_says() {
        // The permission is Android's and can be taken away at any moment. A
        // screen that had cached the answer would keep claiming to work.
        val recorder = Fx.Recorder()
        val granted = Fx.peer(
            granted = true,
            policy = NotificationPolicy(allowedApps = setOf("com.example.chat")),
        )
        var state by mutableStateOf(Fx.state(granted))
        compose.setContent {
            AnyFlowTheme {
                NotificationSettingsScreen(
                    state = state,
                    actions = recorder.actions(),
                    fingerprintHex = Fx.PEER_HEX,
                    onOpenAppPicker = {},
                )
            }
        }
        compose.onNodeWithText("Ready").assertIsDisplayed()

        state = Fx.state(
            granted,
            accessGranted = false,
            status = Fx.idleStatus(accessGranted = false),
        )
        compose.waitForIdle()
        compose.onNodeWithText("Needs Android access").assertIsDisplayed()
        compose.onAllNodesWithText("Ready").assertCountEquals(0)
    }

    // --- the remaining policy ----------------------------------------------

    @Test
    fun the_source_lock_policy_defaults_to_the_application_name_only() {
        show(Fx.state(Fx.peer(granted = true)), Fx.Recorder())
        compose.onNodeWithText("Share the app name only").performScrollTo().assertIsDisplayed()
        compose.onNode(hasText("Share the app name only")).assertIsSelected()
        compose.onNode(hasText("Share everything")).assertIsNotSelected()
    }

    @Test
    fun choosing_a_lock_policy_writes_exactly_that_policy() {
        val recorder = Fx.Recorder()
        show(Fx.state(Fx.peer(granted = true)), recorder)

        compose.onNodeWithText("Share nothing").performScrollTo().performClick()
        assertEquals(LockPolicy.SUPPRESS, recorder.policies.last().whenSourceLocked)
        // And it changed nothing else.
        assertTrue(recorder.policies.last().allowedApps.isEmpty())
        assertEquals(false, recorder.policies.last().allowDismissSync)
    }

    @Test
    fun the_lock_policy_says_that_unlocking_restores_nothing() {
        show(Fx.state(Fx.peer(granted = true)), Fx.Recorder())
        compose
            .onNodeWithText("Unlocking does not send what was held back", substring = true)
            .performScrollTo()
            .assertIsDisplayed()
    }

    @Test
    fun ongoing_notifications_are_off_and_do_not_bypass_the_app_list() {
        show(Fx.state(Fx.peer(granted = true)), Fx.Recorder())
        compose.onNodeWithText("Ongoing notifications").performScrollTo().assertIsDisplayed()
        switchFor("Ongoing notifications").assertIsOff()
        compose
            .onNodeWithText("The app still has to be chosen", substring = true)
            .assertIsDisplayed()
    }

    @Test
    fun a_device_with_no_work_profile_is_told_so_rather_than_offered_an_inert_switch() {
        show(Fx.state(Fx.peer(granted = true), hasWorkProfile = false), Fx.Recorder())
        compose
            .onNodeWithText("This device has no work profile", substring = true)
            .performScrollTo()
            .assertIsDisplayed()
        // And there is no switch to reach for.
        compose
            .onAllNodes(isToggleable() and hasContentDescription("Work profile notifications"))
            .assertCountEquals(0)
    }

    @Test
    fun a_device_with_a_work_profile_is_offered_the_switch_and_it_is_off() {
        show(Fx.state(Fx.peer(granted = true), hasWorkProfile = true), Fx.Recorder())
        compose.onNodeWithText("Work profile notifications").performScrollTo().assertIsDisplayed()
        switchFor("Work profile notifications").assertIsOff()
    }

    // --- what must not be here ---------------------------------------------

    // --- dismissal: the one control that acts on this device ---------------

    private val DISMISS = "Allow this computer to dismiss notifications"

    /**
     * The default, and the one that would matter most if it were wrong.
     *
     * A switch that shipped on would mean everybody who ever granted
     * `notifications.v1` had silently authorised a computer to act on their
     * phone — and it is a default that cannot be walked back for existing
     * installs once it ships.
     */
    @Test
    fun the_dismissal_switch_is_off_by_default() {
        show(Fx.state(Fx.peer(granted = true)), Fx.Recorder())
        compose.onNodeWithText(DISMISS).performScrollTo().assertIsDisplayed()
        switchFor(DISMISS).assertIsOff()
    }

    /**
     * And it says what it does to this phone, and what it does not.
     *
     * The copy is a security control, not marketing: "allow this computer to
     * dismiss notifications" is exactly the phrase somebody could read as
     * "allow this computer to control my notifications", so the row says which
     * of the two it is.
     */
    @Test
    fun the_dismissal_copy_names_the_effect_and_bounds_it() {
        show(Fx.state(Fx.peer(granted = true)), Fx.Recorder())
        compose
            .onNodeWithText("dismisses the original here", substring = true)
            .performScrollTo()
            .assertIsDisplayed()
        for (bound in listOf("buttons", "reply", "open an app", "clear everything at once")) {
            compose
                .onNodeWithText(bound, substring = true)
                .assertIsDisplayed()
        }
        // And it must not imply anything about ongoing notifications either.
        compose
            .onNodeWithText("Ongoing notifications are never dismissed", substring = true)
            .assertIsDisplayed()
    }

    /** Turning it on writes exactly one field, and no other. */
    @Test
    fun the_dismissal_switch_changes_only_the_dismiss_policy() {
        val recorder = Fx.Recorder()
        val before = NotificationPolicy(
            allowedApps = setOf("com.example.chat"),
            includeOngoing = true,
            whenSourceLocked = LockPolicy.FULL,
        )
        show(Fx.state(Fx.peer(granted = true, policy = before)), recorder)

        switchFor(DISMISS).performScrollTo().performClick()

        val written = recorder.policies.single()
        assertTrue(written.allowDismissSync)
        assertEquals(before.allowMirror, written.allowMirror)
        assertEquals(before.allowedApps, written.allowedApps)
        assertEquals(before.knownApps, written.knownApps)
        assertEquals(before.includeOngoing, written.includeOngoing)
        assertEquals(before.includeWorkProfile, written.includeWorkProfile)
        assertEquals(before.whenSourceLocked, written.whenSourceLocked)
        // And it is not a grant change: the capability grant has its own row.
        assertTrue(recorder.grants.isEmpty())
    }

    /** A stored `true` is drawn as on. */
    @Test
    fun the_dismissal_switch_shows_what_is_stored() {
        show(
            Fx.state(
                Fx.peer(
                    granted = true,
                    policy = NotificationPolicy(
                        allowedApps = setOf("com.example.chat"),
                        allowDismissSync = true,
                    ),
                ),
            ),
            Fx.Recorder(),
        )
        switchFor(DISMISS).performScrollTo().assertIsOn()
        // The ACTIVE state line, not the description. Both contain the phrase
        // "dismisses the original here" — the description says what the switch
        // would do, and this says that it is doing it — so the assertion has to
        // name the half that only appears when the state is actually active.
        compose
            .onNodeWithText("On. Dismissing a mirrored notification", substring = true)
            .assertIsDisplayed()
    }

    /**
     * On, and the computer cannot report a dismissal. The switch still shows
     * the stored choice — it is the person's — and the line underneath says
     * plainly that nothing will happen, and what would change that.
     */
    @Test
    fun an_unusable_dismissal_state_is_truthful_rather_than_silent() {
        show(
            Fx.state(
                Fx.peer(
                    granted = true,
                    policy = NotificationPolicy(
                        allowedApps = setOf("com.example.chat"),
                        allowDismissSync = true,
                    ),
                ),
                status = Fx.sourcingStatus(peerIsDismissReporter = false),
            ),
            Fx.Recorder(),
        )
        switchFor(DISMISS).performScrollTo().assertIsOn()
        compose
            .onNodeWithText("has not said it can report", substring = true)
            .assertIsDisplayed()
        compose
            .onNodeWithText("may need to reconnect", substring = true)
            .assertIsDisplayed()
    }

    /**
     * Without the OS grant the row names the real gate rather than blaming the
     * computer, because that is where the fix is.
     */
    @Test
    fun without_android_access_the_dismissal_row_names_the_real_gate() {
        show(
            Fx.state(
                Fx.peer(
                    granted = true,
                    policy = NotificationPolicy(
                        allowedApps = setOf("com.example.chat"),
                        allowDismissSync = true,
                    ),
                ),
                accessGranted = false,
                status = Fx.idleStatus(accessGranted = false),
            ),
            Fx.Recorder(),
        )
        compose
            .onNodeWithText("Android has not given AnyFlow notification access", substring = true)
            .performScrollTo()
            .assertIsDisplayed()
    }

    /**
     * A control that could not be honoured is not offered at all: with no
     * `notifications.v1` grant, the whole section below it is absent.
     */
    @Test
    fun an_ungranted_computer_is_offered_no_dismissal_control() {
        show(Fx.state(Fx.peer(granted = false)), Fx.Recorder())
        compose.onAllNodesWithText(DISMISS).assertCountEquals(0)
        compose.onAllNodes(isToggleable() and hasContentDescription(DISMISS)).assertCountEquals(0)
    }

    // --- what must not be here ---------------------------------------------

    /**
     * The dismissal switch is the whole of it. There is no control here for a
     * notification's actions, a reply, opening an app or clearing everything —
     * because `DismissRequest` has no field that could carry any of them.
     */
    @Test
    fun no_control_here_goes_further_than_a_dismissal() {
        show(
            Fx.state(
                Fx.peer(
                    granted = true,
                    policy = NotificationPolicy(
                        allowedApps = setOf("com.example.chat"),
                        allowDismissSync = true,
                    ),
                ),
            ),
            Fx.Recorder(),
        )
        for (forbidden in listOf(
            "Reply from",
            "Allow replies",
            "notification actions",
            "Clear all",
            "Dismiss all",
            "Snooze",
            "Open app",
        )) {
            compose.onAllNodesWithText(forbidden, substring = true, ignoreCase = true)
                .assertCountEquals(0)
        }
    }

    @Test
    fun the_privacy_copy_never_offers_androids_own_redaction_as_reassurance() {
        show(Fx.state(Fx.peer(granted = true)), Fx.Recorder())
        for (forbidden in listOf(
            "verification code",
            "one-time",
            "OTP",
            "automatically hides",
            "detects sensitive",
        )) {
            compose.onAllNodesWithText(forbidden, substring = true, ignoreCase = true)
                .assertCountEquals(0)
        }
        compose
            .onNodeWithText("does not read, classify or hide", substring = true)
            .performScrollTo()
            .assertIsDisplayed()
    }

    @Test
    fun no_screen_here_lists_a_notification() {
        show(
            Fx.state(
                Fx.peer(
                    granted = true,
                    policy = NotificationPolicy(allowedApps = setOf("com.example.chat")),
                ),
            ),
            Fx.Recorder(),
        )
        compose
            .onNodeWithText("keeps no history", substring = true)
            .performScrollTo()
            .assertIsDisplayed()
    }

    // --- the diagnostic section, which is where protocol words live ---------

    @Test
    fun protocol_vocabulary_is_confined_to_the_details_section() {
        show(
            Fx.state(
                Fx.peer(
                    granted = true,
                    policy = NotificationPolicy(allowedApps = setOf("com.example.chat")),
                ),
            ),
            Fx.Recorder(),
        )
        // The primary status line, which is what the screen opens on, is in
        // words a person can use.
        compose.onNodeWithText("Ready").assertIsDisplayed()

        // The protocol vocabulary is below the fold, in Details, and has to be
        // scrolled to through the screen's own container rather than assumed
        // visible. Present, because somebody debugging two devices needs it…
        compose.onNodeWithText("Details").performScrollTo().assertIsDisplayed()
        compose.onNodeWithText("SOURCE", substring = true).performScrollTo().assertIsDisplayed()
        compose.onNodeWithText("SINK", substring = true).performScrollTo().assertIsDisplayed()

        // …and confined to it: each term occurs once, on a Details row, and
        // never in a gate title, a status badge or an explainer.
        compose.onAllNodesWithText("SOURCE", substring = true).assertCountEquals(1)
        compose.onAllNodesWithText("SINK", substring = true).assertCountEquals(1)
        compose.onAllNodesWithText("epoch", substring = true).assertCountEquals(2)
        compose
            .onNodeWithText("SOURCE", substring = true)
            .onSiblings()
            .filterToOne(hasText("This device announces"))
            .assertIsDisplayed()
    }
}
