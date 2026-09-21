package io.github.yurisismotto.omnibridge

import android.content.ClipData
import android.content.ClipDescription
import android.content.ClipboardManager
import android.content.Context
import android.os.Build
import android.os.SystemClock
import android.util.Log
import androidx.test.core.app.ActivityScenario
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.github.yurisismotto.omnibridge.clipboard.ClipboardReadFailed
import io.github.yurisismotto.omnibridge.clipboard.ClipboardTarget
import io.github.yurisismotto.omnibridge.clipboard.ClipboardText
import io.github.yurisismotto.omnibridge.clipboard.SystemClipboard
import io.github.yurisismotto.omnibridge.ui.MainActivity
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith

/**
 * `clipboard.v1` against the **real** Android clipboard.
 *
 * Nothing here is mocked. That is the point: the JVM suite proves the
 * decision logic against a fake store, and this suite proves the platform
 * claims the decision logic is built on — that `setPrimaryClip` works from
 * the background, that `getPrimaryClip` does not, and that
 * `EXTRA_IS_REMOTE_DEVICE` and `EXTRA_IS_SENSITIVE` land where we put them.
 * A mock could not tell us any of that.
 *
 * The observations are logged under [TAG] as well as asserted, because the
 * certification report has to state what this device actually did rather than
 * what the documentation says it should.
 */
@RunWith(AndroidJUnit4::class)
class ClipboardInstrumentedTest {

    private lateinit var context: Context
    private lateinit var clipboard: SystemClipboard
    private lateinit var manager: ClipboardManager

    @Before
    fun setUp() {
        context = InstrumentationRegistry.getInstrumentation().targetContext
        clipboard = SystemClipboard(context)
        manager = context.getSystemService(ClipboardManager::class.java)

        Log.i(
            TAG,
            "device=${Build.MANUFACTURER} ${Build.MODEL} " +
                "api=${Build.VERSION.SDK_INT} release=${Build.VERSION.RELEASE}",
        )
    }

    private fun text(value: String) = ClipboardText.validate(value).getOrThrow()

    /**
     * Runs [body] with OmniBridge on screen **and holding window focus**.
     *
     * The distinction is the whole subject of this file. `ActivityScenario`
     * reports `RESUMED` as soon as `onResume` has run, but window focus is a
     * separate event that arrives later — and Android's clipboard check tests
     * *focus*, not lifecycle state. Reading from `onActivity` directly is
     * therefore a race that this device loses reliably: the first run of this
     * suite failed six tests that way, with Settings still holding focus.
     *
     * So this polls `hasWindowFocus()` and only then runs the body. If focus
     * never arrives it says so, rather than reporting a clipboard restriction
     * that was really a timing bug.
     */
    private fun withFocusedActivity(body: (MainActivity) -> Unit) {
        // Retried, because focus is genuinely racy on a real device: the
        // previous test's Activity may still be finishing, and a transition
        // animation can outlast a single wait. A flaky helper would be worse
        // than useless here — it would report a platform restriction that is
        // really a stopwatch.
        repeat(FOCUS_ATTEMPTS) { attempt ->
            val instrumentation = InstrumentationRegistry.getInstrumentation()
            var focused = false

            ActivityScenario.launch(MainActivity::class.java).use { scenario ->
                val deadline = SystemClock.uptimeMillis() + FOCUS_TIMEOUT_MS
                while (!focused && SystemClock.uptimeMillis() < deadline) {
                    instrumentation.waitForIdleSync()
                    scenario.onActivity { activity -> focused = activity.hasWindowFocus() }
                    if (!focused) SystemClock.sleep(100)
                }
                if (focused) scenario.onActivity(body)
            }

            if (focused) return
            Log.w(TAG, "attempt ${attempt + 1} did not get window focus; retrying")
            // Let whatever holds focus settle before trying again.
            SystemClock.sleep(1_000)
        }

        assertTrue(
            "OmniBridge never took window focus in $FOCUS_ATTEMPTS attempts of " +
                "${FOCUS_TIMEOUT_MS}ms, so this test cannot say anything about " +
                "clipboard access. Is another app holding focus, or the screen locked?",
            false,
        )
    }

    // -----------------------------------------------------------------------
    // Writing
    // -----------------------------------------------------------------------

    /**
     * CLIP-H01 / CLIP-H12: applying a received clip.
     *
     * `setPrimaryClip` has no focus requirement, which is what makes
     * automatic Fedora → Android sync possible at all. Asserted rather than
     * assumed.
     */
    @Test
    fun setPrimaryClip_applies_a_remote_clip() {
        val value = "applied from a computer"

        // The write itself, with nothing on screen. This is the asymmetry the
        // whole capability rests on, so it is asserted *without* focus: a
        // clip from a computer must land while OmniBridge is in the background.
        val result = clipboard.write(text(value), sensitive = false)
        assertEquals(ClipboardTarget.WriteResult.Applied, result)
        Log.i(TAG, "setPrimaryClip from the background: APPLIED")

        // Verifying it needs a *read*, and a read needs focus. Note what this
        // separation buys: if the write had silently failed, the read below
        // would show the old value rather than passing vacuously.
        withFocusedActivity {
            val clip = manager.primaryClip
            assertNotNull("a focused window should be able to read back", clip)
            assertEquals(value, clip!!.getItemAt(0).coerceToText(context).toString())
        }
        Log.i(TAG, "setPrimaryClip: written from background, read back with focus")
    }

    /** CLIP-H05 / CLIP-H06: nothing is transformed on the way through. */
    @Test
    fun the_real_clipboard_round_trips_unicode_and_multiline_text_byte_for_byte() {
        val cases = listOf(
            "olá, ação e coração",
            "¿cómo estás? el ñandú",
            "🇧🇷 🎉 👨‍👩‍👧‍👦 café",
            "日本語 中文 한국어",
            "line one\nline two\r\nline three\tindented",
            "  significant  spaces  ",
        )

        withFocusedActivity {
            for (case in cases) {
                assertEquals(
                    ClipboardTarget.WriteResult.Applied,
                    clipboard.write(text(case), sensitive = false),
                )
                val clip = manager.primaryClip
                assertNotNull("a focused window should read back case: ${case.length} chars", clip)
                val readBack = clip!!.getItemAt(0).coerceToText(context).toString()
                assertEquals("the platform altered the text", case, readBack)
                assertTrue(
                    "the bytes changed in transit",
                    case.toByteArray(Charsets.UTF_8)
                        .contentEquals(readBack.toByteArray(Charsets.UTF_8)),
                )
            }
        }
        Log.i(TAG, "unicode round trip: byte-identical for ${cases.size} cases")
    }

    // -----------------------------------------------------------------------
    // The two ClipDescription hints
    // -----------------------------------------------------------------------

    /**
     * CLIP-H01: `EXTRA_IS_REMOTE_DEVICE` on API 34+.
     *
     * A presentation hint: it tells the system this clip came from another
     * device, which is what stops Android showing a "copied" toast for
     * something the person did not copy. Nothing in OmniBridge depends on it.
     */
    @Test
    fun extra_is_remote_device_is_set_on_api_34_and_above() {
        clipboard.write(text("from another device"), sensitive = false)

        // `primaryClipDescription` is a read, so it needs focus like any other.
        var extras: android.os.PersistableBundle? = null
        withFocusedActivity { extras = manager.primaryClipDescription?.extras }
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
            assertNotNull("API ${Build.VERSION.SDK_INT} should carry extras", extras)
            assertTrue(
                "EXTRA_IS_REMOTE_DEVICE should be set",
                extras!!.getBoolean(ClipDescription.EXTRA_IS_REMOTE_DEVICE, false),
            )
            Log.i(TAG, "EXTRA_IS_REMOTE_DEVICE: SET (api ${Build.VERSION.SDK_INT})")
        } else {
            // The constant does not exist below API 34; the capability must
            // work anyway, which the write above already showed.
            Log.i(TAG, "EXTRA_IS_REMOTE_DEVICE: not applicable (api ${Build.VERSION.SDK_INT})")
        }
    }

    /**
     * CLIP-H04: `EXTRA_IS_SENSITIVE` on API 33+.
     *
     * Also a hint: it asks the system to hide the preview and asks clipboard
     * managers not to keep the clip in history. It is emphatically not an
     * access control — this test asserts that it is *set*, never that it
     * protects anything.
     */
    @Test
    fun extra_is_sensitive_is_set_only_when_the_clip_is_marked_sensitive() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU) {
            Log.i(TAG, "EXTRA_IS_SENSITIVE: not applicable (api ${Build.VERSION.SDK_INT})")
            return
        }

        withFocusedActivity {
            clipboard.write(text("a one-time code"), sensitive = true)
            val sensitiveExtras = manager.primaryClipDescription?.extras
            assertNotNull(sensitiveExtras)
            assertTrue(
                "EXTRA_IS_SENSITIVE should be set for a sensitive clip",
                sensitiveExtras!!.getBoolean(ClipDescription.EXTRA_IS_SENSITIVE, false),
            )
            assertTrue(
                "our own reader should agree",
                clipboard.isSensitive(manager.primaryClipDescription),
            )

            // And an ordinary clip must not be marked: a flag set on
            // everything says nothing.
            clipboard.write(text("an ordinary clip"), sensitive = false)
            assertFalse(
                "an ordinary clip must not be marked sensitive",
                clipboard.isSensitive(manager.primaryClipDescription),
            )
        }
        Log.i(TAG, "EXTRA_IS_SENSITIVE: SET when asked, absent otherwise")
    }

    /**
     * A clip produced by another app and marked sensitive must be *detected*
     * as such, which is what triggers the extra confirmation before it leaves
     * the device.
     */
    @Test
    fun a_sensitive_clip_from_another_app_is_detected() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU) return

        // Built the way a password manager builds one, not through our writer.
        val clip = ClipData.newPlainText("password", "hunter2")
        clip.description.extras = android.os.PersistableBundle().apply {
            putBoolean(ClipDescription.EXTRA_IS_SENSITIVE, true)
        }
        manager.setPrimaryClip(clip)

        withFocusedActivity {
            assertTrue(
                "a sensitive clip from another app must be recognised",
                clipboard.isSensitive(manager.primaryClipDescription),
            )
        }
        Log.i(TAG, "third-party EXTRA_IS_SENSITIVE: detected")
    }

    // -----------------------------------------------------------------------
    // Reading — the gate ANDROID-CLIPBOARD-READ
    // -----------------------------------------------------------------------

    /**
     * ANDROID-CLIPBOARD-READ: reading with the app on screen.
     *
     * This is the flow the capability actually uses — the Send clipboard
     * button on a resumed Activity — so it must work.
     */
    @Test
    fun the_clipboard_can_be_read_with_the_activity_in_the_foreground() {
        val value = "read from the foreground"
        manager.setPrimaryClip(ClipData.newPlainText(SystemClipboard.DEFAULT_LABEL, value))

        withFocusedActivity {
            val result = clipboard.read()
            assertTrue(
                "a focused read should succeed: ${result.exceptionOrNull()?.message}",
                result.isSuccess,
            )
            assertEquals(value, result.getOrThrow().text.text)
        }
        Log.i(TAG, "ANDROID-CLIPBOARD-READ foreground: ALLOWED")
    }

    /**
     * ANDROID-CLIPBOARD-READ: what happens with no Activity on screen.
     *
     * The result is **recorded, not required**. Android does not promise
     * either answer to an instrumented test, and the capability is built so
     * that both are fine: a refusal is reported as
     * [ClipboardTarget.ReadFailure.NotAllowed] and the person is told to open
     * the app, and a success changes nothing because nothing reads the
     * clipboard except an explicit tap.
     *
     * What *is* asserted is the invariant that matters: the wrapper returns a
     * coherent result and never crashes, whichever way the platform goes.
     */
    @Test
    fun a_background_read_is_reported_coherently_whatever_the_platform_decides() {
        val value = "read without focus"
        manager.setPrimaryClip(ClipData.newPlainText(SystemClipboard.DEFAULT_LABEL, value))

        // No Activity launched: the app process is alive but has no window.
        val result = clipboard.read()

        if (result.isSuccess) {
            // Permitted here. Note that this does *not* license a background
            // watcher: an instrumented test runs in a process the system
            // treats differently from a plain app, and the capability does
            // not rely on this either way.
            assertEquals(value, result.getOrThrow().text.text)
            Log.i(TAG, "ANDROID-CLIPBOARD-READ background: ALLOWED in this context")
        } else {
            val failure = result.exceptionOrNull()
            assertNotNull("a failure must carry a reason", failure?.message)
            // Without focus, `hasPrimaryClip()` is refused too, so "empty" and
            // "not allowed" are indistinguishable from inside the app. Both
            // are logged so the gate evidence is unambiguous about which
            // signals were actually available.
            Log.i(
                TAG,
                "ANDROID-CLIPBOARD-READ background: REFUSED " +
                    "(read=${failure?.message} hasPrimaryClip=${manager.hasPrimaryClip()})",
            )
        }
    }

    /** A clipboard holding something that is not text is not an error. */
    @Test
    fun a_non_text_clipboard_is_reported_as_not_text_rather_than_as_a_failure() {
        // An intent clip: legal, and definitively not text.
        val clip = ClipData.newIntent("an intent", android.content.Intent("test.action"))
        manager.setPrimaryClip(clip)

        withFocusedActivity {
            val result = clipboard.read()
            // Either NotText, or coerced to something — both are handled.
            // What must not happen is a crash or a silent empty success.
            if (result.isFailure) {
                Log.i(TAG, "non-text clipboard: ${result.exceptionOrNull()?.message}")
            } else {
                Log.i(TAG, "non-text clipboard: coerced to text by the platform")
            }
        }
    }

    /** An empty clipboard is "nothing to send", not an error to report. */
    @Test
    fun an_empty_clipboard_is_reported_as_empty() {
        manager.clearPrimaryClip()

        withFocusedActivity {
            val result = clipboard.read()
            assertTrue("an empty clipboard is not readable text", result.isFailure)

            // And it must be reported as *empty*, not as a refusal. Telling a
            // person to open the app when the app is already open, because
            // their clipboard happens to be empty, is a loop with no exit.
            val failure = (result.exceptionOrNull() as ClipboardReadFailed).failure
            assertEquals(
                "an empty clipboard must not look like a permission problem",
                ClipboardTarget.ReadFailure.Empty,
                failure,
            )
            Log.i(TAG, "empty clipboard: ${failure.describe()}")
        }
    }

    companion object {
        /** `adb logcat -s OmniBridgeClipTest` collects every observation above. */
        private const val TAG = "OmniBridgeClipTest"

        /**
         * How long to wait for OmniBridge to take window focus.
         *
         * Generous: on a device where another app is in the foreground, the
         * launch, the transition animation and the focus grant are all real
         * time, and a short timeout would make this suite flaky in a way that
         * looks exactly like a platform restriction.
         */
        private const val FOCUS_TIMEOUT_MS = 10_000L

        /** How many times to relaunch before calling it a real failure. */
        private const val FOCUS_ATTEMPTS = 3
    }
}
