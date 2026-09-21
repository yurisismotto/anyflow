package io.github.yurisismotto.omnibridge

import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.test.SemanticsNodeInteraction
import androidx.compose.ui.test.assert
import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertIsOff
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.assertIsOn
import androidx.compose.ui.test.hasAnyAncestor
import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.hasStateDescription
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isDialog
import androidx.compose.ui.test.isToggleable
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performSemanticsAction
import androidx.compose.ui.test.performTextInput
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.github.yurisismotto.omnibridge.NotificationUiFixtures as Fx
import io.github.yurisismotto.omnibridge.notifications.NotificationApps
import io.github.yurisismotto.omnibridge.notifications.NotificationPolicy
import io.github.yurisismotto.omnibridge.ui.AppPickerScreen
import io.github.yurisismotto.omnibridge.ui.theme.OmniBridgeTheme
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/**
 * The deny-by-default application picker.
 *
 * This screen is where the largest usability gap N2 left is closed, and it is
 * also where the largest mistake could be made: a picker that pre-selected
 * anything, or that turned an empty selection into "everything", would undo
 * the one control that actually stops a 2FA code reaching a screen in an
 * office. The tests below pin that it does neither.
 *
 * ## The semantics contract these tests are written against
 *
 * A row is **one merged node** — the application's label, its package name and
 * the word *Shared* or *Not shared* are read out as a single item, which is
 * what a person using a screen reader needs and what a bare tick box could not
 * give them. The tick itself is a `Checkbox` **child** of that row, and it is
 * both the real touch target and the only node that carries a
 * `ToggleableState`; the row deliberately is not clickable.
 *
 * So the assertions below split in two, and the split is deliberate:
 *
 *  * what a person *perceives* — the merged row's `StateDescription`;
 *  * what a person *operates* — the `Checkbox`, reached in the unmerged tree
 *    by [checkboxFor], asserted with `assertIsOn`/`assertIsOff` and clicked.
 *
 * [a_row_is_an_accessible_summary_and_the_checkbox_is_the_control] pins that
 * arrangement on its own, so it stays a decision rather than an accident.
 */
@RunWith(AndroidJUnit4::class)
class AppPickerUiTest {

    @get:Rule
    val compose = createComposeRule()

    private val catalogue = listOf(
        Fx.app("com.example.bank", "Banco"),
        Fx.app("com.example.chat", "Chat"),
        Fx.app("com.example.maps", "Maps"),
    )

    private fun show(
        policy: NotificationPolicy,
        recorder: Fx.Recorder,
        apps: List<io.github.yurisismotto.omnibridge.notifications.NotificationApp> = catalogue,
        granted: Boolean = true,
    ) {
        recorder.apps = apps.map { it.copy(allowed = it.packageName in policy.allowedApps) }
        val peer = Fx.peer(granted = granted, policy = policy)
        compose.setContent {
            OmniBridgeTheme {
                AppPickerScreen(
                    state = Fx.state(peer),
                    actions = recorder.actions(),
                    fingerprintHex = Fx.PEER_HEX,
                )
            }
        }
        compose.waitForIdle()
    }

    // --- reaching the right node -------------------------------------------

    /** The merged row: what is on screen, and what a screen reader announces. */
    private fun rowFor(label: String): SemanticsNodeInteraction =
        compose.onNode(hasText(label))

    /**
     * The tick: the actual toggleable control inside that row.
     *
     * It carries the application's label as its `contentDescription`, which is
     * what makes it addressable without depending on the row's text hierarchy
     * or on a test tag.
     */
    private fun checkboxFor(label: String): SemanticsNodeInteraction =
        compose.onNode(
            isToggleable() and hasContentDescription(label),
            useUnmergedTree = true,
        )

    /**
     * What it means for the picker to be *offering* a package.
     *
     * Not "the string appears somewhere": a row, with a sharing state on it.
     * Nothing else on this screen carries one.
     */
    private fun pickerRowFor(packageName: String) =
        hasText(packageName) and
            (hasStateDescription("Shared") or hasStateDescription("Not shared"))

    /**
     * A button inside the confirmation dialog.
     *
     * Scoped to the dialog window, because the confirming button carries the
     * same words as the button that opened it, and an index into
     * `onAllNodesWithText` is not a contract — it is a guess about ordering.
     */
    private fun dialogButton(text: String): SemanticsNodeInteraction =
        compose.onNode(hasAnyAncestor(isDialog()) and hasClickAction() and hasText(text))

    /**
     * Presses a dialog button through its semantics action.
     *
     * Touch injection into the dialog's own window fails on the certification
     * hardware; the click *action* is the same one the platform's accessibility
     * services invoke, and it is what the button actually promises.
     */
    private fun pressInDialog(text: String) {
        dialogButton(text).performSemanticsAction(SemanticsActions.OnClick)
        compose.waitForIdle()
    }

    /**
     * The dismiss button's words, resolved the way the screen resolves them.
     *
     * It is `android.R.string.cancel`, so it is the platform's word and not
     * ours: on the certification hardware the vendor's framework does not
     * spell it "Cancel". Hard-coding it would be asserting against a device
     * skin rather than against the product.
     */
    private fun cancelLabel(): String =
        InstrumentationRegistry.getInstrumentation()
            .targetContext
            .getString(android.R.string.cancel)

    // --- the semantics contract itself -------------------------------------

    @Test
    fun a_row_is_an_accessible_summary_and_the_checkbox_is_the_control() {
        // Pinned so the arrangement below cannot regress into "the row is the
        // toggle" — which would be a real accessibility loss, because the row
        // is what carries the label, the package and the state as words.
        val recorder = Fx.Recorder()
        show(NotificationPolicy(allowedApps = setOf("com.example.bank")), recorder)

        // Merged: one node per application, and it says what it is.
        val row = compose.onNode(hasText("Banco") and hasText("com.example.bank"))
        row.assertIsDisplayed()
        row.assert(hasStateDescription("Shared"))
        rowFor("Chat").assert(hasStateDescription("Not shared"))

        // Unmerged: exactly one toggleable control for that application…
        compose
            .onAllNodes(isToggleable() and hasContentDescription("Banco"), useUnmergedTree = true)
            .assertCountEquals(1)
        // …the row itself is not one…
        compose.onAllNodes(pickerRowFor("com.example.bank") and isToggleable()).assertCountEquals(0)
        // …and the control is what writes, for exactly that package.
        checkboxFor("Banco").assertIsOn().performClick()
        assertEquals(emptySet<String>(), recorder.policies.last().allowedApps)
    }

    // --- deny by default ---------------------------------------------------

    @Test
    fun a_fresh_picker_has_nothing_ticked() {
        val recorder = Fx.Recorder()
        show(NotificationPolicy(), recorder)

        compose.onNodeWithText("Banco").assertIsDisplayed()
        compose.onNodeWithText("Chat").assertIsDisplayed()
        compose.onNodeWithText("Maps").assertIsDisplayed()
        for (label in listOf("Banco", "Chat", "Maps")) {
            checkboxFor(label).assertIsOff()
        }
        compose.onNodeWithText("No app chosen").assertIsDisplayed()
        // Opening the picker records a baseline for "N new apps", and that is
        // the ONLY policy write it may make on its own.
        assertTrue(
            "opening the picker selected an application",
            recorder.policies.all { it.allowedApps.isEmpty() },
        )
    }

    @Test
    fun opening_the_picker_records_a_baseline_and_selects_nothing() {
        val recorder = Fx.Recorder()
        show(NotificationPolicy(), recorder)

        val written = recorder.policies.last()
        assertEquals(
            setOf("com.example.bank", "com.example.chat", "com.example.maps"),
            written.knownApps,
        )
        assertEquals(emptySet<String>(), written.allowedApps)
    }

    @Test
    fun ticking_an_application_adds_exactly_that_one() {
        val recorder = Fx.Recorder()
        show(NotificationPolicy(), recorder)

        checkboxFor("Chat").performClick()
        assertEquals(setOf("com.example.chat"), recorder.policies.last().allowedApps)
    }

    @Test
    fun unticking_an_application_removes_exactly_that_one() {
        val recorder = Fx.Recorder()
        show(
            NotificationPolicy(allowedApps = setOf("com.example.chat", "com.example.maps")),
            recorder,
        )

        checkboxFor("Chat").assertIsOn().performClick()
        assertEquals(setOf("com.example.maps"), recorder.policies.last().allowedApps)
    }

    @Test
    fun the_stored_selection_is_what_the_rows_show() {
        // What the rows *show* is the accessible presentation, so this is the
        // one assertion that belongs on the merged node rather than the tick.
        show(NotificationPolicy(allowedApps = setOf("com.example.bank")), Fx.Recorder())
        rowFor("Banco").assert(hasStateDescription("Shared"))
        rowFor("Chat").assert(hasStateDescription("Not shared"))
        rowFor("Maps").assert(hasStateDescription("Not shared"))
        compose.onNodeWithText("1 of 3 apps").assertIsDisplayed()
    }

    // --- select all, which is never automatic ------------------------------

    @Test
    fun select_all_asks_before_it_shares_a_hundred_applications() {
        val recorder = Fx.Recorder()
        show(NotificationPolicy(), recorder)

        compose.onNodeWithText("Select all").performClick()
        // The dialog is up and nothing has been written yet.
        compose
            .onNodeWithText("Share notifications from all 3 apps", substring = true)
            .assertIsDisplayed()
        assertTrue(recorder.policies.all { it.allowedApps.isEmpty() })
    }

    @Test
    fun select_all_shares_everything_only_after_it_is_confirmed() {
        val recorder = Fx.Recorder()
        show(NotificationPolicy(), recorder)

        compose.onNodeWithText("Select all").performClick()
        // The confirming button carries the same words as the one that opened
        // the dialog; it is told apart by living inside the dialog, not by
        // where it happens to fall in the node list.
        pressInDialog("Select all")
        assertEquals(
            setOf("com.example.bank", "com.example.chat", "com.example.maps"),
            recorder.policies.last().allowedApps,
        )
    }

    @Test
    fun cancelling_select_all_shares_nothing() {
        val recorder = Fx.Recorder()
        show(NotificationPolicy(), recorder)

        compose.onNodeWithText("Select all").performClick()
        val before = recorder.policies.toList()
        pressInDialog(cancelLabel())
        // The dialog is gone…
        compose.onAllNodes(isDialog()).assertCountEquals(0)
        // …and the selection is exactly what it was, not merely still empty.
        assertEquals(before, recorder.policies.toList())
        assertTrue(
            "cancelling the confirmation still shared applications",
            recorder.policies.all { it.allowedApps.isEmpty() },
        )
    }

    @Test
    fun clear_all_empties_the_selection() {
        val recorder = Fx.Recorder()
        show(
            NotificationPolicy(allowedApps = setOf("com.example.chat", "com.example.maps")),
            recorder,
        )

        compose.onNodeWithText("Clear all").performClick()
        assertEquals(emptySet<String>(), recorder.policies.last().allowedApps)
    }

    // --- finding an application --------------------------------------------

    @Test
    fun search_narrows_the_list_by_label() {
        show(NotificationPolicy(), Fx.Recorder())
        compose.onNodeWithText("Search apps").performTextInput("cha")
        compose.waitForIdle()
        compose.onNodeWithText("Chat").assertIsDisplayed()
        compose.onAllNodesWithText("Banco").assertCountEquals(0)
    }

    @Test
    fun search_also_matches_the_package_name() {
        // Two applications can carry the same label, and the package name is
        // the only thing that tells them apart.
        show(NotificationPolicy(), Fx.Recorder())
        compose.onNodeWithText("Search apps").performTextInput("example.maps")
        compose.waitForIdle()
        compose.onNodeWithText("Maps").assertIsDisplayed()
        compose.onAllNodesWithText("Chat").assertCountEquals(0)
    }

    @Test
    fun a_search_that_matches_nothing_says_so_rather_than_looking_broken() {
        show(NotificationPolicy(), Fx.Recorder())
        compose.onNodeWithText("Search apps").performTextInput("zzzz")
        compose.waitForIdle()
        compose.onNodeWithText("No app matches that search.").assertIsDisplayed()
    }

    // --- what cannot be in the list ----------------------------------------

    @Test
    fun omnibridges_own_package_is_not_offered() {
        // The rule lives in `NotificationApps.build`, which is what produces
        // the list the picker is handed on a real device, so the list here is
        // built the same way rather than written by hand. A hand-written row
        // would only have proved that the screen renders what it is given,
        // which is not the invariant and is not where the rule is.
        //
        // The inputs are deliberately hostile: OmniBridge's own package arrives
        // from all three sources at once — a launcher entry, the notification
        // shade, and a stored policy that already names it.
        val recorder = Fx.Recorder()
        val labels = mapOf(
            "com.example.bank" to "Banco",
            "com.example.chat" to "Chat",
            "com.example.maps" to "Maps",
            Fx.OWN_PACKAGE to "OmniBridge",
        )
        val offered = NotificationApps.build(
            launchable = labels.keys,
            notifying = listOf(Fx.OWN_PACKAGE),
            allowed = setOf(Fx.OWN_PACKAGE),
            ownPackage = Fx.OWN_PACKAGE,
            labels = { labels[it] ?: it },
        )
        show(NotificationPolicy(allowedApps = setOf(Fx.OWN_PACKAGE)), recorder, apps = offered)

        // Not one picker row for it, and nothing that could tick it.
        compose.onAllNodes(pickerRowFor(Fx.OWN_PACKAGE)).assertCountEquals(0)
        compose
            .onAllNodes(isToggleable() and hasContentDescription("OmniBridge"), useUnmergedTree = true)
            .assertCountEquals(0)
        compose.onAllNodesWithText("OmniBridge").assertCountEquals(0)
        // And the three real applications are still offered, so this is not an
        // empty list passing by accident.
        compose.onAllNodes(pickerRowFor("com.example.chat")).assertCountEquals(1)
        compose.onNodeWithText("Banco").assertIsDisplayed()
    }

    @Test
    fun an_application_that_is_notifying_now_is_marked_as_such() {
        // The only source that can name a package with no launcher entry, and
        // the one a person will be looking for during set-up.
        show(
            NotificationPolicy(),
            Fx.Recorder(),
            apps = catalogue + Fx.app("com.android.shell", "Shell", notifying = true),
        )
        compose.onNodeWithText("Shell").assertIsDisplayed()
        compose.onNodeWithText("Notifying now").assertIsDisplayed()
    }

    @Test
    fun a_computer_whose_grant_was_withdrawn_has_nothing_to_choose() {
        val recorder = Fx.Recorder()
        show(NotificationPolicy(), recorder, granted = false)
        compose
            .onNodeWithText("cannot see any notification", substring = true)
            .assertIsDisplayed()
        compose.onAllNodesWithText("Chat").assertCountEquals(0)
        assertEquals(0, recorder.appsLoaded)
    }

    @Test
    fun an_empty_inventory_explains_itself_rather_than_showing_a_blank_screen() {
        show(NotificationPolicy(), Fx.Recorder(), apps = emptyList())
        compose
            .onNodeWithText("No app could be listed", substring = true)
            .assertIsDisplayed()
        // And neither bulk action is offered when there is nothing to act on.
        compose.onNodeWithText("Select all").assertIsNotEnabled()
        compose.onNodeWithText("Clear all").assertIsNotEnabled()
    }

    @Test
    fun the_package_name_is_shown_beside_every_label() {
        show(NotificationPolicy(), Fx.Recorder())
        compose.onNodeWithText("com.example.bank").assertIsDisplayed()
        compose.onNodeWithText("com.example.chat").assertIsDisplayed()
    }
}
