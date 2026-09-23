package io.github.yurisismotto.omnibridge.ui

import android.view.View
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat

/**
 * Keeps the pairing scanner's prompt out from under the system bars.
 *
 * ## ANDROID-UX-SCANNER-INSETS-01
 *
 * Once ANDROID-UX-ORIENTATION-01 unlocked the scanner, portrait became usable
 * for the first time and showed a layout defect that had never been visible.
 * Measured on the certification hardware (SM-X620, Android 16 / API 36):
 *
 * ```text
 * zxing_status_view      [623,2842][1176,2880]
 * navigationBars inset   [0,2784][1800,2880]   (bottom = 96px)
 * ```
 *
 * Every pixel of the prompt was inside the navigation bar. Three facts
 * combine to produce that, and none of them is a bug in isolation:
 *
 *  1. the app targets SDK 35, so Android 15's edge-to-edge enforcement
 *     applies: the activity window spans the whole display and the system
 *     bars are drawn *over* it rather than beside it;
 *  2. `zxing-android-embedded:4.3.0` predates that enforcement. Its
 *     `zxing_barcode_scanner` layout anchors `zxing_status_view` with
 *     `layout_gravity="bottom|center_horizontal"` inside a full-bleed
 *     `FrameLayout`, and consumes no insets anywhere;
 *  3. its `zxing_CaptureTheme` inherits `Theme.Holo.NoActionBar.Fullscreen`,
 *     which hides the *status* bar and says nothing about the navigation bar,
 *     and sets no `fitsSystemWindows`.
 *
 * So the prompt is laid out against the bottom of the *window*, and the
 * bottom of the window is behind the navigation bar.
 *
 * ## What this does, and what it deliberately does not do
 *
 * It pads the prompt by the insets that can hide it, and nothing else. The
 * camera preview stays full-bleed, which is what a viewfinder should be and
 * what keeps the framing correct.
 *
 * It does **not** hide the system bars. Turning on immersive mode would make
 * the measurement above read correctly while taking the person's navigation
 * away, which is hiding the defect rather than fixing it.
 */
object ScannerInsets {

    /**
     * The edges of a bottom-anchored strip that a system inset can eat.
     *
     * No top: the prompt is anchored to the bottom, and padding its top would
     * push the text *down*, towards the very bar being avoided.
     */
    data class Edges(val left: Int, val right: Int, val bottom: Int) {
        companion object {
            val NONE = Edges(0, 0, 0)
        }
    }

    /**
     * The insets that can hide the prompt.
     *
     *  * `navigationBars` — the measured defect, in both button and gesture
     *    navigation, and on whichever edge the bar is on in landscape;
     *  * `displayCutout` — a notch or hole-punch, which in landscape lands on
     *    the side the prompt is centred across;
     *  * `systemGestures` — wider than the bar in gesture navigation. On the
     *    certification hardware its frame is identical to `navigationBars`
     *    (`[0,2784][1800,2880]`), so it costs nothing there; including it is
     *    what makes the guarantee hold on a device where it is not.
     *
     * Not `statusBars`: the capture theme is fullscreen and the prompt is at
     * the other end of the window either way.
     */
    fun occludingTypes(): Int =
        WindowInsetsCompat.Type.navigationBars() or
            WindowInsetsCompat.Type.displayCutout() or
            WindowInsetsCompat.Type.systemGestures()

    /**
     * The padding a strip should carry, given the padding it was laid out
     * with and the insets currently over it.
     *
     * Absolute, from a fixed [base], rather than added to whatever the view
     * already had. That is the whole reason this is a function: insets are
     * re-delivered on every rotation, every navigation-mode change and every
     * inset animation frame, and a handler that accumulated would walk the
     * prompt up the screen a little further each time the device was turned.
     *
     * With no insets the answer is [base] exactly, so a device with no
     * navigation bar gets no invented gap.
     */
    fun padding(base: Edges, insets: Edges): Edges = Edges(
        left = base.left + insets.left,
        right = base.right + insets.right,
        bottom = base.bottom + insets.bottom,
    )

    /**
     * Attaches the handler to the scanner's prompt.
     *
     * [base] is read once, here, for the reason [padding] gives. The listener
     * returns the insets **unconsumed**: this view is one child of the
     * scanner's frame, and swallowing them would leave any sibling — now or
     * later — unable to see them.
     */
    fun attach(status: View) {
        val base = Edges(status.paddingLeft, status.paddingRight, status.paddingBottom)
        val top = status.paddingTop
        ViewCompat.setOnApplyWindowInsetsListener(status) { view, windowInsets ->
            val inset = windowInsets.getInsets(occludingTypes())
            val padded = padding(base, Edges(inset.left, inset.right, inset.bottom))
            view.setPadding(padded.left, top, padded.right, padded.bottom)
            windowInsets
        }
        // The view is in the hierarchy but insets may already have been
        // dispatched, so ask for one now rather than waiting for a rotation.
        ViewCompat.requestApplyInsets(status)
    }
}
