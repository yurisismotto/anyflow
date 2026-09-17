package io.github.yurisismotto.anyflow.ui

import com.journeyapps.barcodescanner.ScanOptions
import io.github.yurisismotto.anyflow.pairing.QrPayload

/**
 * How AnyFlow asks for a pairing QR code, and what it does with the answer.
 *
 * Both halves live here rather than inline in [MainActivity] so that they can
 * be tested on the JVM: neither the options nor the outcome touches an Android
 * framework class, and both encode a rule worth pinning.
 *
 * ## ANDROID-UX-ORIENTATION-01
 *
 * The scanner used to force landscape whatever the device was doing, and
 * there were **two** causes, neither of them visible in AnyFlow's own code:
 *
 *  1. `zxing-android-embedded` declares `CaptureActivity` with
 *     `android:screenOrientation="sensorLandscape"` in its own manifest, and
 *     the manifest merger imported that. Fixed in `AndroidManifest.xml` with
 *     a `tools:replace` re-declaration.
 *  2. `CaptureManager.initializeFromIntent` reads the `SCAN_ORIENTATION_LOCKED`
 *     extra **defaulting to true**, and on true calls `lockOrientation()`,
 *     which hard-pins `setRequestedOrientation()` to whatever orientation the
 *     activity has at launch. So even with the manifest fixed the scanner
 *     would freeze at the orientation it opened in and refuse to rotate.
 *     Fixed by [options] below.
 *
 * Fixing either alone leaves the scanner locked, which is why both are named
 * in one place.
 *
 * AnyFlow does not own the user's orientation during a scan. With both causes
 * removed the activity is `unspecified`, which is Android applying the user's
 * own policy: rotation locked to portrait stays portrait, auto-rotate on
 * follows the device.
 *
 * ## ANDROID-UX-SCANNER-INSETS-01
 *
 * Unlocking the orientation made portrait usable for the first time, and that
 * exposed a layout defect underneath it: the library's prompt strip lays out
 * against the bottom of an edge-to-edge window, which on API 35+ is behind
 * the navigation bar. [options] therefore launches
 * [PairingCaptureActivity] — the library's own screen plus an inset handler —
 * rather than the library's `CaptureActivity` directly. See [ScannerInsets].
 */
object PairingScanner {

    /** What the scanner screen tells the user to point the camera at. */
    const val PROMPT = "Point at the QR code shown by `anyflow pair`"

    /**
     * The scan request.
     *
     * [ScanOptions.setOrientationLocked] with `false` is the load-bearing
     * line: without it the library pins the orientation at launch. It is
     * spelled out rather than left to a default because the library's default
     * is the opposite of what this app wants.
     */
    fun options(): ScanOptions = ScanOptions()
        .setDesiredBarcodeFormats(ScanOptions.QR_CODE)
        .setPrompt(PROMPT)
        .setBeepEnabled(false)
        // See the class docs. The library's default is `true`, which pins the
        // activity to its launch orientation for as long as the camera is up.
        .setOrientationLocked(false)
        // ANDROID-UX-SCANNER-INSETS-01. Without this the contract launches
        // the library's own `CaptureActivity`, whose prompt lays out behind
        // the navigation bar. Ours is that activity plus an inset handler and
        // nothing else; see `PairingCaptureActivity`.
        .setCaptureActivity(PairingCaptureActivity::class.java)

    /**
     * What a finished scan means.
     *
     * A sealed set rather than a nullable payload, so that "the user backed
     * out" and "that code was not ours" cannot be collapsed into one silent
     * `return`. They are different events and the second one is worth saying
     * out loud.
     */
    sealed interface Outcome {
        /** Back, or the scanner was dismissed. Nothing happened. */
        data object Cancelled : Outcome

        /**
         * Something was scanned and it is not an AnyFlow pairing code.
         *
         * Carries nothing. The scanned text is attacker-supplied and may hold
         * a pairing token, so it is never echoed to a screen, a log or a
         * crash report.
         */
        data object NotAnyFlowCode : Outcome

        /** A well-formed AnyFlow pairing code. */
        data class Pair(val payload: QrPayload) : Outcome
    }

    /**
     * Interprets a scan result.
     *
     * `null` contents is how `ScanContract` reports a cancellation, which is
     * also what a rotation-induced re-scan must never be mistaken for.
     *
     * Everything else goes through [QrPayload.parse], unchanged: the scheme,
     * the fingerprint, the token length and the device id are checked there
     * and nowhere else. Orientation has no bearing on any of it — a code
     * scanned sideways is the same bytes.
     */
    fun outcomeOf(contents: String?): Outcome = when {
        contents == null -> Outcome.Cancelled
        else -> QrPayload.parse(contents)
            ?.let { Outcome.Pair(it) }
            ?: Outcome.NotAnyFlowCode
    }
}
