package io.github.yurisismotto.omnibridge.ui

import com.journeyapps.barcodescanner.CaptureActivity
import com.journeyapps.barcodescanner.DecoratedBarcodeView

/**
 * OmniBridge's pairing scanner screen.
 *
 * It is `CaptureActivity` from `zxing-android-embedded` with exactly one
 * thing added: the prompt is kept out from under the system bars. See
 * [ScannerInsets] for the measurement and the three causes.
 *
 * ## Why a subclass and not a layout, a theme or a fork
 *
 * The prompt's position comes from the *library's* layout, inflated by the
 * library's own `initializeContent`. There is no attribute on our side to
 * change and nothing to override in a theme: what is missing is an inset
 * listener on a view the library creates. `initializeContent` is the library's
 * documented seam — it is `protected`, it returns the inflated
 * [DecoratedBarcodeView], and `getStatusView()` on that is public — so the
 * whole fix is four lines and the decode path is untouched.
 *
 * Copying `zxing_capture.xml` and `zxing_barcode_scanner.xml` into this app
 * would have worked too, and would have meant owning two layouts that must
 * stay in step with a dependency we do not control. Forking the library was
 * never on the table.
 *
 * ## What it must not become
 *
 * Every barcode is still decoded by ZXing. This class holds no scanning
 * state, no camera handle and no result handling: `CaptureManager` keeps all
 * of it, exactly as it does for the library's own activity, so there is one
 * scanner stack and not two.
 *
 * ## Orientation (ANDROID-UX-ORIENTATION-01)
 *
 * This activity does **not** call `setRequestedOrientation`, and its manifest
 * declaration is `unspecified`. Both halves of the orientation fix survive a
 * custom activity only because nothing here re-introduces a lock — the
 * runtime half still lives in [PairingScanner.options], which tells the
 * library not to pin the orientation at launch.
 */
class PairingCaptureActivity : CaptureActivity() {

    /**
     * Inflates the library's scanner and insets its prompt.
     *
     * Overriding this rather than `onCreate` is what guarantees the view
     * exists: the superclass's `onCreate` calls this, takes the result and
     * hands it straight to `CaptureManager`, so there is no window in which
     * the prompt is on screen without its handler attached.
     */
    override fun initializeContent(): DecoratedBarcodeView {
        val scanner = super.initializeContent()
        ScannerInsets.attach(scanner.statusView)
        return scanner
    }
}
