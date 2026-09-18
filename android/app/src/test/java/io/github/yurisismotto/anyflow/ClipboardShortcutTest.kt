package io.github.yurisismotto.anyflow

import io.github.yurisismotto.anyflow.ui.ClipboardShortcut
import io.github.yurisismotto.anyflow.ui.ClipboardShortcut.Action
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * GitHub #7 — the Quick Settings clipboard request never runs before focus.
 *
 * These are plain JUnit tests with no Android framework and no Robolectric,
 * because [ClipboardShortcut] holds no Android types. That is not a
 * convenience: the connected-test harness uninstalls the app, which destroys
 * the tablet's pairing and every piece of physical evidence on it, so a rule
 * that can be stated on the JVM is stated on the JVM.
 *
 * The letters map to the brief's §16 list.
 */
class ClipboardShortcutTest {

    private val first = "a".repeat(32)
    private val second = "b".repeat(32)

    /**
     * **F1 — a tile request before focus reads nothing.**
     *
     * The defect itself. The old code called `sendClipboardFromShortcut()`
     * straight out of `handleIntent`, which `onCreate` calls before
     * `setContent`; Android 16 answered `hasPrimaryClip() == false` and the
     * person was told their clipboard was empty.
     */
    @Test
    fun f1_a_request_before_focus_does_not_execute() {
        val shortcut = ClipboardShortcut()
        assertEquals(
            Action.NOTHING,
            shortcut.onRequest(first, hasWindowFocus = false),
        )
        assertTrue("the request must be remembered, not dropped", shortcut.isPending())
    }

    /** **F2 — focus arriving executes it, exactly once.** */
    @Test
    fun f2_focus_drains_the_request_once() {
        val shortcut = ClipboardShortcut()
        shortcut.onRequest(first, hasWindowFocus = false)
        assertEquals(Action.SEND, shortcut.onWindowFocusChanged(true))
        assertFalse(shortcut.isPending())
    }

    /**
     * **F3 — repeated `focus = true` does not repeat the send.**
     *
     * Android calls `onWindowFocusChanged(true)` more than once in ordinary
     * use: coming back from the shade, from a dialog, from the recents
     * switcher. Every one of those would have been another clipboard send.
     */
    @Test
    fun f3_a_second_focus_callback_does_not_execute_again() {
        val shortcut = ClipboardShortcut()
        shortcut.onRequest(first, hasWindowFocus = false)
        assertEquals(Action.SEND, shortcut.onWindowFocusChanged(true))
        repeat(5) {
            assertEquals(Action.NOTHING, shortcut.onWindowFocusChanged(true))
        }
    }

    /**
     * **F4 — losing focus executes nothing, and does not cancel.**
     *
     * The shade coming down over us is not the person changing their mind.
     * The request has to survive it, or a tile press while the phone was
     * busy would silently do nothing at all.
     */
    @Test
    fun f4_losing_focus_neither_executes_nor_cancels() {
        val shortcut = ClipboardShortcut()
        shortcut.onRequest(first, hasWindowFocus = false)
        assertEquals(Action.NOTHING, shortcut.onWindowFocusChanged(false))
        assertTrue("a request must survive a focus loss", shortcut.isPending())
        assertEquals(Action.SEND, shortcut.onWindowFocusChanged(true))
    }

    /**
     * **F5 — an intent arriving at an already-focused screen runs now.**
     *
     * `onNewIntent` on a screen the person is looking at. Waiting here would
     * wait for a focus change that has already happened and will not repeat,
     * so the tile would appear to do nothing.
     */
    @Test
    fun f5_a_request_while_focused_executes_immediately() {
        val shortcut = ClipboardShortcut()
        assertEquals(Action.SEND, shortcut.onRequest(first, hasWindowFocus = true))
        assertFalse(shortcut.isPending())
    }

    /** **F6 — a second, genuine tile press sends again.** */
    @Test
    fun f6_a_second_tile_press_executes_again() {
        val shortcut = ClipboardShortcut()
        assertEquals(Action.SEND, shortcut.onRequest(first, hasWindowFocus = true))
        assertEquals(Action.SEND, shortcut.onRequest(second, hasWindowFocus = true))

        // And through the unfocused path too: the tile is normally pressed
        // from the shade, which is exactly when the app is not focused.
        val fromShade = ClipboardShortcut()
        fromShade.onRequest(first, hasWindowFocus = false)
        assertEquals(Action.SEND, fromShade.onWindowFocusChanged(true))
        fromShade.onRequest(second, hasWindowFocus = false)
        assertEquals(Action.SEND, fromShade.onWindowFocusChanged(true))
    }

    /**
     * **F7 — an ordinary launch reads no clipboard.**
     *
     * Opening the app from the launcher never reaches [ClipboardShortcut] at
     * all, and if focus arrives with nothing pending there is nothing to do.
     */
    @Test
    fun f7_an_ordinary_launch_executes_nothing() {
        val shortcut = ClipboardShortcut()
        assertEquals(Action.NOTHING, shortcut.onWindowFocusChanged(true))
        assertFalse(shortcut.isPending())
    }

    /**
     * **F8 — an unidentified request is refused.**
     *
     * `MainActivity` is exported for the launcher, so an explicit intent from
     * another application can reach it carrying `ACTION_SEND_CLIPBOARD`. Our
     * own tile always stamps an id; an intent without one cannot be told
     * apart from its own replay, and "send the clipboard somewhere" is not an
     * instruction worth guessing at.
     *
     * This is not the security boundary and is not claimed as one — the grant,
     * the per-peer policy and the sensitive-clip confirmation are, and they
     * are asked downstream regardless. It is the difference between refusing
     * an unidentified request and honouring it.
     */
    @Test
    fun f8_a_request_without_an_id_is_refused() {
        val shortcut = ClipboardShortcut()
        assertEquals(Action.NOTHING, shortcut.onRequest(null, hasWindowFocus = true))
        assertEquals(Action.NOTHING, shortcut.onRequest("", hasWindowFocus = true))
        assertFalse(shortcut.isPending())
        // And it does not arm anything for a later focus change either.
        assertEquals(Action.NOTHING, shortcut.onWindowFocusChanged(true))
    }

    /** **F9 — a lifecycle callback with nothing pending does nothing.** */
    @Test
    fun f9_focus_without_a_pending_request_is_inert() {
        val shortcut = ClipboardShortcut()
        shortcut.onRequest(first, hasWindowFocus = true)
        repeat(3) {
            assertEquals(Action.NOTHING, shortcut.onWindowFocusChanged(false))
            assertEquals(Action.NOTHING, shortcut.onWindowFocusChanged(true))
        }
    }

    /**
     * **F10 — the request is cleared before execution, so re-entry cannot
     * duplicate it.**
     *
     * The caller starts a coroutine that reads the clipboard. Anything that
     * re-enters this object while that runs must find nothing to do. Stated
     * as a property of the object rather than of the call site, because the
     * call site is what changes.
     */
    @Test
    fun f10_the_request_is_spent_before_the_caller_acts() {
        val shortcut = ClipboardShortcut()
        shortcut.onRequest(first, hasWindowFocus = false)

        var reentrantActions = 0
        // Simulates the caller acting on SEND and something calling back in.
        if (shortcut.onWindowFocusChanged(true) == Action.SEND) {
            if (shortcut.onWindowFocusChanged(true) == Action.SEND) reentrantActions++
            if (shortcut.onRequest(first, hasWindowFocus = true) == Action.SEND) {
                reentrantActions++
            }
        }
        assertEquals(0, reentrantActions)
    }

    /**
     * **F11 — several presses before focus produce one send.**
     *
     * Defined, not accidental. Two presses before the window comes up are the
     * same instruction about the same clipboard, so they coalesce; queueing
     * them would send the same clip twice to the same computer. The id that
     * wins is the newest, which is the one the person pressed last.
     */
    @Test
    fun f11_rapid_presses_before_focus_coalesce_to_one_send() {
        val shortcut = ClipboardShortcut()
        val third = "c".repeat(32)
        shortcut.onRequest(first, hasWindowFocus = false)
        shortcut.onRequest(second, hasWindowFocus = false)
        shortcut.onRequest(third, hasWindowFocus = false)

        assertEquals(Action.SEND, shortcut.onWindowFocusChanged(true))
        assertEquals(Action.NOTHING, shortcut.onWindowFocusChanged(true))
        assertEquals(third, shortcut.consumedId())
        // The two that were superseded are spent too: neither can come back
        // through a replayed intent.
        assertEquals(Action.SEND, shortcut.onRequest(first, hasWindowFocus = true))
    }

    /**
     * **F12 — a configuration change does not replay a consumed request.**
     *
     * The one that needed the id. `getIntent()` keeps returning the tile's
     * intent, so `onCreate` runs `handleIntent` again after a rotation. With
     * a boolean, rotating the phone after a tile send would have sent the
     * clipboard a second time, silently.
     */
    @Test
    fun f12_recreation_does_not_replay_a_consumed_request() {
        val before = ClipboardShortcut()
        before.onRequest(first, hasWindowFocus = false)
        assertEquals(Action.SEND, before.onWindowFocusChanged(true))

        // The Activity is recreated. Saved state carries the spent id; the
        // same launch intent is handed to the new instance.
        val after = ClipboardShortcut()
        after.restore(before.consumedId())
        assertEquals(Action.NOTHING, after.onRequest(first, hasWindowFocus = false))
        assertFalse(after.isPending())
        assertEquals(Action.NOTHING, after.onWindowFocusChanged(true))

        // A genuine press after the rotation still works.
        assertEquals(Action.SEND, after.onRequest(second, hasWindowFocus = true))
    }

    /**
     * Process death loses the saved state, and that is the honest worst case.
     *
     * With no `consumedId` restored, a replayed launch intent would run once
     * more. It is bounded — one extra send, of the person's own clipboard, to
     * the computer they already chose, with the Activity visible — and the
     * alternative is persisting a clipboard-shaped breadcrumb to disk, which
     * this project does not do for anything clipboard-related.
     */
    @Test
    fun a_restore_of_nothing_leaves_a_fresh_machine() {
        val shortcut = ClipboardShortcut()
        shortcut.restore(null)
        assertNull(shortcut.consumedId())
        assertEquals(Action.SEND, shortcut.onRequest(first, hasWindowFocus = true))
    }
}
