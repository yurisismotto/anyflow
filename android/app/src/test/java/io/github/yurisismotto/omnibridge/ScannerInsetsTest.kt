package io.github.yurisismotto.omnibridge

import androidx.core.view.WindowInsetsCompat
import io.github.yurisismotto.omnibridge.ui.ScannerInsets
import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * ANDROID-UX-SCANNER-INSETS-01: the scanner's prompt must not sit behind the
 * system bars.
 *
 * ## The defect, measured
 *
 * On the certification hardware (SM-X620, Android 16 / API 36, One UI 8.0),
 * with the scanner open in portrait:
 *
 * ```text
 * zxing_status_view      [623,2842][1176,2880]
 * navigationBars         [0,2784][1800,2880]   bottom inset 96px
 * ```
 *
 * Every pixel of the prompt was inside the navigation bar. The app targets
 * SDK 35, so Android 15's edge-to-edge enforcement applies and the window
 * spans the display; `zxing-android-embedded:4.3.0` anchors its status
 * `TextView` with `layout_gravity="bottom|center_horizontal"` and consumes no
 * insets anywhere.
 *
 * ## Why the assertions are about relationships
 *
 * `1800×2880` and `96` are this one tablet. A test that asserted them would
 * pass on the tablet and prove nothing about a phone, a foldable or gesture
 * navigation — so the numbers appear here only as *inputs*, and what is
 * asserted is that the prompt clears whatever inset it is given. The physical
 * bounds are evidence and live in the sprint report.
 */
class ScannerInsetsTest {

    private fun edges(left: Int, right: Int, bottom: Int) =
        ScannerInsets.Edges(left, right, bottom)

    /** Every Kotlin source file that ships inside the app. */
    private fun productionSources(): List<File> =
        File("src/main/java").walkTopDown().filter { it.isFile && it.extension == "kt" }.toList()

    private fun withoutKotlinComments(text: String): String = text
        .replace(Regex("/\\*.*?\\*/", RegexOption.DOT_MATCHES_ALL), "")
        .replace(Regex("//[^\n]*"), "")

    // -----------------------------------------------------------------------
    // C. the navigation bar is consumed
    // -----------------------------------------------------------------------

    /**
     * The measured case: a 96px bottom bar in portrait. The prompt gains
     * exactly that much bottom padding, which lifts its text clear.
     */
    @Test
    fun `a bottom navigation bar becomes bottom padding`() {
        val base = edges(0, 0, 0)
        val padded = ScannerInsets.padding(base, edges(0, 0, 96))
        assertEquals(96, padded.bottom)
        assertEquals(0, padded.left)
        assertEquals(0, padded.right)
    }

    /** With the device's real numbers, the prompt clears the bar. */
    @Test
    fun `the measured portrait case clears the measured navigation bar`() {
        val displayHeight = 2880
        val navigationBarTop = 2784
        val inset = displayHeight - navigationBarTop

        val padded = ScannerInsets.padding(edges(0, 0, 0), edges(0, 0, inset))
        // The strip is bottom-anchored, so its text now ends this far above
        // the bottom of the window.
        val textBottom = displayHeight - padded.bottom
        assertTrue(
            "the prompt's text must end at or above the top of the bar " +
                "($textBottom vs $navigationBarTop)",
            textBottom <= navigationBarTop,
        )
    }

    /** The handler asks for every inset that can hide the prompt. */
    @Test
    fun `the handler consumes navigation bars, cutouts and gesture insets`() {
        val types = ScannerInsets.occludingTypes()
        assertNotEquals(
            "the measured defect is the navigation bar",
            0,
            types and WindowInsetsCompat.Type.navigationBars(),
        )
        assertNotEquals(
            "a notch lands on the side the prompt is centred across",
            0,
            types and WindowInsetsCompat.Type.displayCutout(),
        )
        assertNotEquals(
            "gesture navigation reserves more than the bar does",
            0,
            types and WindowInsetsCompat.Type.systemGestures(),
        )
    }

    /**
     * And not the status bar. The capture theme is fullscreen and the prompt
     * is at the other end of the window; padding for it would push the text
     * down, towards the bar being avoided.
     */
    @Test
    fun `the handler does not pad for the status bar`() {
        assertEquals(
            0,
            ScannerInsets.occludingTypes() and WindowInsetsCompat.Type.statusBars(),
        )
    }

    // -----------------------------------------------------------------------
    // D. no inset, no gap
    // -----------------------------------------------------------------------

    @Test
    fun `no insets means no invented padding`() {
        val base = edges(0, 0, 0)
        assertEquals(base, ScannerInsets.padding(base, ScannerInsets.Edges.NONE))
    }

    /** A view that was laid out with padding of its own keeps exactly it. */
    @Test
    fun `a view with its own padding keeps exactly that when nothing occludes it`() {
        val base = edges(12, 12, 8)
        assertEquals(base, ScannerInsets.padding(base, ScannerInsets.Edges.NONE))
    }

    // -----------------------------------------------------------------------
    // E. insets change; padding does not accumulate
    // -----------------------------------------------------------------------

    /**
     * Insets are re-delivered on every rotation, every navigation-mode change
     * and every inset-animation frame. A handler that added to the view's
     * current padding would walk the prompt up the screen a little further
     * each time the tablet was turned.
     */
    @Test
    fun `repeated inset deliveries do not accumulate padding`() {
        val base = edges(0, 0, 4)

        // Portrait: bar at the bottom.
        val portrait = ScannerInsets.padding(base, edges(0, 0, 96))
        assertEquals(100, portrait.bottom)

        // Rotated: the same bar is now on the right, and the bottom inset is
        // gone. The bottom padding must go back to the base, not stay at 100.
        val landscape = ScannerInsets.padding(base, edges(0, 132, 0))
        assertEquals(4, landscape.bottom)
        assertEquals(132, landscape.right)

        // Rotated back. Identical to the first delivery, not double it.
        assertEquals(portrait, ScannerInsets.padding(base, edges(0, 0, 96)))

        // And a hundred deliveries are the same as one.
        var latest = base
        repeat(100) { latest = ScannerInsets.padding(base, edges(0, 0, 96)) }
        assertEquals(portrait, latest)
    }

    /** The same delivery twice is idempotent by construction. */
    @Test
    fun `the padding rule is a pure function of base and insets`() {
        val base = edges(3, 5, 7)
        val inset = edges(11, 13, 17)
        assertEquals(ScannerInsets.padding(base, inset), ScannerInsets.padding(base, inset))
        assertEquals(edges(14, 18, 24), ScannerInsets.padding(base, inset))
    }

    // -----------------------------------------------------------------------
    // F. portrait and landscape share one rule, and no fixed sizes
    // -----------------------------------------------------------------------

    /**
     * Both orientations go through the same function with different inputs.
     * Nothing about either is special-cased, and nothing is measured in
     * device pixels that this code decides.
     */
    @Test
    fun `every edge is cleared whatever the orientation puts where`() {
        val base = edges(0, 0, 0)
        val cases = listOf(
            // portrait, button navigation
            edges(0, 0, 96),
            // portrait, gesture navigation
            edges(40, 40, 48),
            // landscape, bar on the right
            edges(0, 132, 0),
            // landscape, bar on the left plus a cutout on the right
            edges(132, 88, 0),
            // a foldable with insets on every edge at once
            edges(64, 64, 64),
            // nothing at all
            edges(0, 0, 0),
        )
        for (inset in cases) {
            val padded = ScannerInsets.padding(base, inset)
            assertTrue("$inset left", padded.left >= inset.left)
            assertTrue("$inset right", padded.right >= inset.right)
            assertTrue("$inset bottom", padded.bottom >= inset.bottom)
        }
    }

    /** No pixel constant of this device, or any device, is written down. */
    @Test
    fun `the inset rule hard-codes no sizes`() {
        val code = withoutKotlinComments(
            File("src/main/java/io/github/yurisismotto/omnibridge/ui/ScannerInsets.kt").readText(),
        )
        val numbers = Regex("\\b\\d{2,}\\b").findAll(code).map { it.value }.toList()
        assertTrue("ScannerInsets must contain no pixel sizes: $numbers", numbers.isEmpty())
    }

    // -----------------------------------------------------------------------
    // I and J. the fix takes nothing and hides nothing
    // -----------------------------------------------------------------------

    /**
     * J. The alternative to insetting the prompt was hiding the bar, which
     * would make the measurement read correctly while taking the person's
     * navigation away. Nothing that ships may do it, from anywhere.
     */
    @Test
    fun `no production code hides the system bars`() {
        val forbidden = listOf(
            "SYSTEM_UI_FLAG_HIDE_NAVIGATION",
            "SYSTEM_UI_FLAG_FULLSCREEN",
            "SYSTEM_UI_FLAG_IMMERSIVE",
            "setSystemUiVisibility",
            "BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE",
            "WindowInsetsControllerCompat",
            "windowInsetsController",
            "FLAG_FULLSCREEN",
            "FLAG_LAYOUT_NO_LIMITS",
        )
        val offenders = productionSources().flatMap { file ->
            val code = withoutKotlinComments(file.readText())
            forbidden.filter { code.contains(it) }.map { "${file.name}: $it" }
        }
        assertTrue(
            "the prompt is moved out of the bars, never the other way round: $offenders",
            offenders.isEmpty(),
        )
    }

    /** Nor may a theme do it on their behalf. */
    @Test
    fun `no OmniBridge theme turns on a fullscreen or translucent bar flag`() {
        val themes = File("src/main/res").walkTopDown()
            .filter { it.isFile && it.extension == "xml" }
            .toList()
        val forbidden = listOf(
            "windowFullscreen",
            "windowTranslucentNavigation",
            "windowHideNavigationBar",
        )
        val offenders = themes.flatMap { file ->
            val text = file.readText()
            forbidden.filter { text.contains(it) }.map { "${file.name}: $it" }
        }
        assertTrue("$offenders", offenders.isEmpty())
    }

    /**
     * I. Insetting a view needs no permission, and a scanner already had the
     * only one it uses. `CAMERA` stays the single camera-related entry and
     * nothing was added beside it.
     */
    @Test
    fun `the fix asks for no new permission`() {
        val manifest = File("src/main/AndroidManifest.xml").readText()
        val permissions = Regex("<uses-permission android:name=\"([^\"]+)\"")
            .findAll(manifest)
            .map { it.groupValues[1].removePrefix("android.permission.") }
            .toList()
        assertEquals(
            listOf(
                "INTERNET",
                "ACCESS_NETWORK_STATE",
                "CHANGE_WIFI_MULTICAST_STATE",
                "CHANGE_NETWORK_STATE",
                "FOREGROUND_SERVICE",
                "FOREGROUND_SERVICE_CONNECTED_DEVICE",
                "POST_NOTIFICATIONS",
                "CAMERA",
            ),
            permissions,
        )
    }

    /** And the capture screen delegates every barcode to ZXing, as before. */
    @Test
    fun `OmniBridge's capture activity adds inset handling and nothing else`() {
        val code = File(
            "src/main/java/io/github/yurisismotto/omnibridge/ui/PairingCaptureActivity.kt",
        ).readText()
        val body = withoutKotlinComments(code)
        assertTrue("it must still be the library's activity", body.contains(": CaptureActivity()"))
        assertTrue(body.contains("ScannerInsets.attach"))
        for (forbidden in listOf("BarcodeCallback", "decodeSingle", "CaptureManager", "Camera")) {
            assertFalse(
                "a second scanner stack is exactly what this must not become: $forbidden",
                body.contains(forbidden),
            )
        }
    }
}
