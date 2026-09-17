package io.github.yurisismotto.anyflow

import io.github.yurisismotto.anyflow.ui.PairingCaptureActivity
import io.github.yurisismotto.anyflow.ui.PairingScanner
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * ANDROID-UX-ORIENTATION-01: the pairing scanner must not own the user's
 * orientation.
 *
 * The defect had two causes and neither was in AnyFlow's own Kotlin, which is
 * why the coverage here is split between a static check on the manifest and a
 * behavioural one on the scan request:
 *
 *  1. `zxing-android-embedded`'s library manifest declares `CaptureActivity`
 *     with `android:screenOrientation="sensorLandscape"`, and the merger
 *     imported it.
 *  2. `CaptureManager` reads `SCAN_ORIENTATION_LOCKED` **defaulting to true**
 *     and then hard-pins `setRequestedOrientation()`.
 *
 * A fix for one and not the other still leaves a locked scanner, so both are
 * asserted. These run on the JVM: `ScanOptions` is a plain Java builder over a
 * map of intent extras and needs no device, and the manifest is a file.
 *
 * The physical rotation behaviour itself — preview usable sideways, one
 * pairing result per scan, rotation lock respected — is exercised on the real
 * tablet and recorded in the sprint report. These are what stop the defect
 * coming back without anyone noticing.
 */
class PairingScannerOrientationTest {

    /** Gradle runs unit tests with the module directory as the working dir. */
    private val manifest: String by lazy {
        val file = File("src/main/AndroidManifest.xml")
        assertTrue("expected the app manifest at ${file.absolutePath}", file.isFile)
        file.readText()
    }

    /**
     * The manifest with its comments removed.
     *
     * Documentation is not a declaration. The block above the scanner
     * activity names `sensorLandscape` in order to explain what went wrong,
     * and a check that could not tell prose from XML would push the next
     * person towards deleting the explanation rather than keeping it.
     */
    private val manifestDeclarations: String by lazy { withoutXmlComments(manifest) }

    /** Every Kotlin source file that ships inside the app. */
    private fun productionSources(): List<File> =
        File("src/main/java").walkTopDown().filter { it.isFile && it.extension == "kt" }.toList()

    private fun withoutXmlComments(text: String): String =
        text.replace(Regex("<!--.*?-->", RegexOption.DOT_MATCHES_ALL), "")

    /**
     * Kotlin with comments removed, for the same reason. `PairingScanner`'s
     * own documentation has to be able to say the words the check forbids.
     *
     * Deliberately naive — it does not understand string literals — because a
     * false positive here is a failing test someone reads, and the only
     * strings in this app that could contain these tokens would themselves be
     * a defect.
     */
    private fun withoutKotlinComments(text: String): String = text
        .replace(Regex("/\\*.*?\\*/", RegexOption.DOT_MATCHES_ALL), "")
        .replace(Regex("//[^\n]*"), "")

    // -----------------------------------------------------------------------
    // A. the scan request
    // -----------------------------------------------------------------------

    /** The extras `ScanOptions` accumulates, which is what reaches the intent. */
    @Suppress("UNCHECKED_CAST")
    private fun scanExtras(): Map<String, Any?> =
        PairingScanner.options().moreExtras as Map<String, Any?>

    /**
     * The library locks the orientation unless it is explicitly told not to,
     * so the absence of this extra is not neutral — it is the defect.
     */
    @Test
    fun `the scan request explicitly unlocks the orientation`() {
        val extras = scanExtras()
        assertEquals(
            "SCAN_ORIENTATION_LOCKED must be present and false; the library " +
                "defaults it to true and then pins setRequestedOrientation()",
            false,
            extras["SCAN_ORIENTATION_LOCKED"],
        )
    }

    /** Nothing in the request asks for a landscape, or any other, lock. */
    @Test
    fun `the scan request carries no orientation forcing configuration`() {
        val extras = scanExtras()
        val forcing = extras.filterValues { value ->
            value.toString().contains("LANDSCAPE", ignoreCase = true) ||
                value.toString().contains("PORTRAIT", ignoreCase = true)
        }
        assertTrue("the scan request must not name an orientation: $forcing", forcing.isEmpty())
    }

    /** And it still asks for what it always asked for. */
    @Test
    fun `the scan request is still a silent scan with the same prompt`() {
        val extras = scanExtras()
        assertEquals(false, extras["BEEP_ENABLED"])
        assertEquals(PairingScanner.PROMPT, extras["PROMPT_MESSAGE"])
    }

    // -----------------------------------------------------------------------
    // B. the manifest
    // -----------------------------------------------------------------------

    /**
     * The library's `sensorLandscape` is overridden, and overridden to the
     * one value that defers to the user rather than to another lock.
     */
    @Test
    fun `the scanner activity is declared unspecified and overrides the library`() {
        val declaration = scannerDeclaration()
        assertTrue(
            "the scanner activity must declare screenOrientation=\"unspecified\": $declaration",
            declaration.contains("android:screenOrientation=\"unspecified\""),
        )
        assertTrue(
            "without tools:replace the merger keeps the library's " +
                "sensorLandscape (or fails the build): $declaration",
            declaration.contains("tools:replace=\"android:screenOrientation\""),
        )
    }

    /**
     * `fullSensor` and `fullUser` are the values the library's own README
     * suggests, and both are wrong here: they force sensor rotation and
     * override a person's system rotation lock, which is the same defect
     * pointing the other way.
     */
    @Test
    fun `no activity in the manifest forces or overrides an orientation`() {
        val orientations = Regex("android:screenOrientation=\"([^\"]+)\"")
            .findAll(manifestDeclarations)
            .map { it.groupValues[1] }
            .toList()

        assertEquals(
            "both scanner activities must declare an orientation, and both " +
                "must defer to the user: $orientations",
            listOf("unspecified", "unspecified"),
            orientations,
        )
    }

    /**
     * ANDROID-UX-SCANNER-INSETS-01 introduced AnyFlow's own capture activity,
     * and a subclass gets a *separate* manifest entry: nothing is inherited
     * from the library's declaration. So the orientation guarantee has to be
     * restated on it, or a custom activity becomes how `sensorLandscape`
     * creeps back in.
     */
    @Test
    fun `AnyFlow's own scanner activity is declared unspecified too`() {
        val declaration = declarationFor(".ui.PairingCaptureActivity")
        assertTrue(
            "AnyFlow's capture activity must declare screenOrientation=" +
                "\"unspecified\": $declaration",
            declaration.contains("android:screenOrientation=\"unspecified\""),
        )
        assertFalse(
            "it is launched by ScanContract from inside this app and by " +
                "nothing else: $declaration",
            declaration.contains("android:exported=\"true\""),
        )
        assertTrue(
            "the library's capture theme has to be restated, not inherited: $declaration",
            declaration.contains("android:theme=\"@style/zxing_CaptureTheme\""),
        )
    }

    /** And it is the one the scan request actually launches. */
    @Test
    fun `the scan request launches AnyFlow's capture activity`() {
        assertEquals(
            "without this the contract launches the library's own screen, " +
                "whose prompt lays out behind the navigation bar",
            PairingCaptureActivity::class.java,
            PairingScanner.options().captureActivity,
        )
    }

    // -----------------------------------------------------------------------
    // C. the production code
    // -----------------------------------------------------------------------

    /**
     * No Kotlin that ships may pin the orientation at runtime. This is a
     * source scan on purpose: `setRequestedOrientation` takes effect wherever
     * it is called from, so the assertion has to cover the whole app rather
     * than one screen.
     */
    @Test
    fun `no production code locks the screen orientation`() {
        val forbidden = listOf(
            "setRequestedOrientation",
            "SCREEN_ORIENTATION_LANDSCAPE",
            "SCREEN_ORIENTATION_SENSOR_LANDSCAPE",
            "SCREEN_ORIENTATION_REVERSE_LANDSCAPE",
            "SCREEN_ORIENTATION_PORTRAIT",
            "SCREEN_ORIENTATION_LOCKED",
            "SCREEN_ORIENTATION_NOSENSOR",
        )
        val offenders = productionSources().flatMap { file ->
            val code = withoutKotlinComments(file.readText())
            forbidden.filter { code.contains(it) }.map { "${file.name}: $it" }
        }
        assertTrue("AnyFlow must not own the user's orientation: $offenders", offenders.isEmpty())
    }

    /** And nothing re-locks the scanner through the library's own switch. */
    @Test
    fun `no production code re-locks the scanner orientation`() {
        val offenders = productionSources().filter {
            withoutKotlinComments(it.readText()).contains("setOrientationLocked(true)")
        }
        assertTrue("$offenders must not lock the scanner", offenders.isEmpty())
    }

    // -----------------------------------------------------------------------
    // D and E. the result still means what it meant
    // -----------------------------------------------------------------------

    private val fingerprintHex = "a".repeat(64)
    private val tokenBase32 = "FUPJRRCMFSG6KYYHR5ODB5BBLM7HVSCZ"
    private val deviceId = "deb769060ca6dce3bee7a92f6a82e606"
    private val validCode = "anyflow1:$fingerprintHex:$tokenBase32:$deviceId:192.168.1.10:55432"

    /** D. Back out of the scanner and nothing at all happens. */
    @Test
    fun `a cancelled scan is cancelled and not a failed pairing`() {
        assertEquals(PairingScanner.Outcome.Cancelled, PairingScanner.outcomeOf(null))
    }

    /** E. A real code still reaches the existing parser, unchanged. */
    @Test
    fun `a valid code flows into the existing pairing parser`() {
        val outcome = PairingScanner.outcomeOf(validCode)
        assertTrue("expected a pairing outcome, got $outcome", outcome is PairingScanner.Outcome.Pair)
        val payload = (outcome as PairingScanner.Outcome.Pair).payload
        assertEquals(deviceId, payload.deviceId)
        assertEquals(fingerprintHex, payload.fingerprint.toHex())
        assertNotNull(payload.token)
    }

    /**
     * Orientation changes nothing about what a code has to be. A scan is the
     * same bytes whichever way the device was held, and arbitrary URIs still
     * cannot pair.
     */
    @Test
    fun `an arbitrary uri is still not a pairing code`() {
        val rejected = listOf(
            "https://evil.example/pair",
            "anyflow9:$fingerprintHex:$tokenBase32:$deviceId:10.0.0.1:1",
            "",
            "anyflow1:not-hex:$tokenBase32:$deviceId:10.0.0.1:1",
        )
        for (code in rejected) {
            assertEquals(
                "'$code' must not pair",
                PairingScanner.Outcome.NotAnyFlowCode,
                PairingScanner.outcomeOf(code),
            )
        }
    }

    /** A rejected code says nothing about what was scanned. */
    @Test
    fun `a rejected outcome carries none of the scanned text`() {
        val secret = "anyflow1:$fingerprintHex:$tokenBase32:$deviceId"
        val outcome = PairingScanner.outcomeOf("$secret-corrupted")
        assertFalse(
            "a scanned code may contain a pairing token and must never be echoed",
            outcome.toString().contains(tokenBase32),
        )
    }

    /** Extracts the `<activity>` element naming [name]. */
    private fun declarationFor(name: String): String {
        val manifest = manifestDeclarations
        val start = manifest.indexOf("android:name=\"$name\"")
        assertTrue("the app manifest must declare $name", start >= 0)
        val open = manifest.lastIndexOf("<activity", start)
        val close = manifest.indexOf(">", start)
        return manifest.substring(open, close + 1)
    }

    /** Extracts the `<activity>` element for the zxing capture screen. */
    private fun scannerDeclaration(): String {
        val manifest = manifestDeclarations
        val start = manifest.indexOf("com.journeyapps.barcodescanner.CaptureActivity")
        assertTrue(
            "the app manifest must re-declare CaptureActivity; without its own " +
                "declaration the library's sensorLandscape is what ships",
            start >= 0,
        )
        val open = manifest.lastIndexOf("<activity", start)
        val close = manifest.indexOf(">", start)
        return manifest.substring(open, close + 1)
    }
}
