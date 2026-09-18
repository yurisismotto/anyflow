package io.github.yurisismotto.anyflow

import java.io.File
import kotlin.math.hypot
import kotlin.math.max
import kotlin.math.min
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The Android launcher icon wears the Flow A, and provably the same Flow A.
 *
 * ## Why this test exists at all
 *
 * The branding sprint that introduced `logo-flow-a.svg` moved the product's
 * identity mark and rewrote `app-icon.svg` to carry it. The desktop followed;
 * Android did not, and went on shipping the older Flowing Ribbon on its
 * launcher icon. Nothing failed, because nothing was checking — an app icon
 * is the one asset whose correctness is normally established by somebody
 * looking at a home screen.
 *
 * So these are the checks that would have caught it, and they are deliberately
 * the same shape as `desktop/gui/tests/brand_assets.rs`: the icon is compared
 * to the canonical SVG **by its geometry**, not by its filename.
 *
 * ## Why the geometry is recomputed rather than described
 *
 * The safe-zone claims in `ic_launcher_foreground.xml`'s comment are numbers
 * somebody worked out once. A comment cannot fail. So this test parses the
 * drawable's own path and group transform, flattens the curves, and measures
 * where the ink actually lands — which means editing the icon and getting the
 * placement wrong breaks the build rather than the home screen.
 *
 * All of it runs on the JVM: a VectorDrawable is an XML file and a cubic
 * Bézier is arithmetic. Nothing here needs a device.
 */
class BrandingResourcesTest {

    // -----------------------------------------------------------------------
    // Reading the resources. Gradle runs unit tests with the module directory
    // as the working directory.
    // -----------------------------------------------------------------------

    private fun res(path: String): String {
        val file = File("src/main/res/$path")
        assertTrue("expected a resource at ${file.absolutePath}", file.isFile)
        return file.readText()
    }

    private val manifest: String by lazy {
        File("src/main/AndroidManifest.xml").readText()
    }

    /**
     * The manifest and the drawables with their comments removed.
     *
     * Documentation is not a declaration. These files explain at length what
     * they are *not* — the ribbon they no longer use, the gradient the
     * monochrome layer must not have — and a check that could not tell prose
     * from XML would push the next person into deleting the explanation.
     */
    private fun withoutComments(xml: String): String =
        xml.replace(Regex("<!--.*?-->", RegexOption.DOT_MATCHES_ALL), "")

    /** The canonical mark, from `docs/design/` on the test classpath. */
    private fun canonicalSvg(name: String): String =
        javaClass.classLoader!!.getResourceAsStream("assets/$name")
            ?.bufferedReader()?.readText()
            ?: error("docs/design/assets/$name is not on the test classpath")

    /**
     * Pulls an attribute off the first element that carries it.
     *
     * The leading boundary is load-bearing. Without it, asking for `d` finds
     * the `d` at the end of `id="anyflow-flow-a"` and reports a gradient's
     * name as the mark's path data — which is exactly the kind of false
     * green a geometry test must not be capable of.
     */
    private fun attr(xml: String, name: String): String? =
        Regex("""(?<![\w:.-])$name\s*=\s*"([^"]*)"""").find(xml)?.groupValues?.get(1)

    // -----------------------------------------------------------------------
    // Geometry
    // -----------------------------------------------------------------------

    private data class Point(val x: Double, val y: Double)

    /**
     * Flattens an `M`/`C` path into points along its centreline.
     *
     * Only those two commands are accepted, and that is itself part of the
     * assertion: the Flow A is one continuous ribbon drawn as a single
     * subpath, so a second `M` or any other command would mean the mark has
     * been redrawn as something else.
     */
    private fun flatten(pathData: String): List<Point> {
        val tokens = pathData.replace(",", " ").trim().split(Regex("\\s+"))
        val points = mutableListOf<Point>()
        var cursor: Point? = null
        var i = 0
        var moves = 0

        fun number(at: Int): Double = tokens[at].toDoubleOrNull()
            ?: error("unparsable number '${tokens[at]}' in path data")

        while (i < tokens.size) {
            when (tokens[i]) {
                "M" -> {
                    moves++
                    cursor = Point(number(i + 1), number(i + 2))
                    points += cursor
                    i += 3
                }
                "C" -> {
                    val from = cursor ?: error("a curve before any move")
                    val c1 = Point(number(i + 1), number(i + 2))
                    val c2 = Point(number(i + 3), number(i + 4))
                    val to = Point(number(i + 5), number(i + 6))
                    for (step in 1..STEPS) {
                        val t = step.toDouble() / STEPS
                        val m = 1 - t
                        points += Point(
                            m * m * m * from.x + 3 * m * m * t * c1.x +
                                3 * m * t * t * c2.x + t * t * t * to.x,
                            m * m * m * from.y + 3 * m * m * t * c1.y +
                                3 * m * t * t * c2.y + t * t * t * to.y,
                        )
                    }
                    cursor = to
                    i += 7
                }
                else -> error("unexpected path command '${tokens[i]}': the mark is M and C only")
            }
        }
        assertEquals("the ribbon never lifts, so it has one subpath", 1, moves)
        return points
    }

    /** A `<group>`'s transform, as a VectorDrawable applies it: scale, then translate. */
    private data class Group(val scale: Double, val translateX: Double, val translateY: Double) {
        fun apply(p: Point) = Point(p.x * scale + translateX, p.y * scale + translateY)
    }

    private fun group(xml: String): Group {
        val block = Regex("<group[^>]*>", RegexOption.DOT_MATCHES_ALL).find(xml)?.value
            ?: error("the launcher layer draws its mark inside a <group>")
        // A non-zero pivot would change the arithmetic below, and the icon is
        // deliberately written without one.
        assertEquals("pivotX must be left at its default", null, attr(block, "android:pivotX"))
        assertEquals("pivotY must be left at its default", null, attr(block, "android:pivotY"))
        val scaleX = attr(block, "android:scaleX")?.toDouble() ?: 1.0
        val scaleY = attr(block, "android:scaleY")?.toDouble() ?: 1.0
        assertEquals("the mark is never stretched: scaleX and scaleY agree", scaleX, scaleY, 0.0)
        return Group(
            scale = scaleX,
            translateX = attr(block, "android:translateX")?.toDouble() ?: 0.0,
            translateY = attr(block, "android:translateY")?.toDouble() ?: 0.0,
        )
    }

    // =======================================================================
    // B1 — the adaptive icon's layers exist
    // =======================================================================

    @Test
    fun `every adaptive icon layer resolves to a resource that exists`() {
        for (icon in ADAPTIVE_ICONS) {
            val xml = res("mipmap-anydpi-v26/$icon")
            val colours = res("values/colors.xml")

            for (layer in listOf("background", "foreground", "monochrome")) {
                val reference = Regex("""<$layer[^>]*android:drawable="([^"]*)"""")
                    .find(xml)?.groupValues?.get(1)
                    ?: error("$icon has no <$layer> layer")

                when {
                    reference.startsWith("@drawable/") -> assertTrue(
                        "$icon's $layer names @drawable/${reference.removePrefix("@drawable/")}, " +
                            "which is not in res/drawable",
                        File("src/main/res/drawable/${reference.removePrefix("@drawable/")}.xml")
                            .isFile,
                    )
                    reference.startsWith("@color/") -> assertTrue(
                        "$icon's $layer names $reference, which colors.xml does not define",
                        colours.contains("name=\"${reference.removePrefix("@color/")}\""),
                    )
                    else -> error("$icon's $layer is neither a drawable nor a colour: $reference")
                }
            }
        }
    }

    // =======================================================================
    // B2 — the themed icon is declared
    // =======================================================================

    @Test
    fun `both launcher icons declare a monochrome layer`() {
        for (icon in ADAPTIVE_ICONS) {
            assertTrue(
                "$icon has no <monochrome> layer, so Android 13+ themed icons " +
                    "fall back to a generic tile",
                withoutComments(res("mipmap-anydpi-v26/$icon")).contains("<monochrome"),
            )
        }
    }

    // =======================================================================
    // B3 — the manifest points at them
    // =======================================================================

    @Test
    fun `the manifest names launcher icons that exist`() {
        val declared = withoutComments(manifest)
        assertEquals("@mipmap/ic_launcher", attr(declared, "android:icon"))
        assertEquals("@mipmap/ic_launcher_round", attr(declared, "android:roundIcon"))
        for (icon in ADAPTIVE_ICONS) {
            assertTrue(
                "the manifest names @mipmap/${icon.removeSuffix(".xml")} and it is missing",
                File("src/main/res/mipmap-anydpi-v26/$icon").isFile,
            )
        }
    }

    // =======================================================================
    // B4 — nothing still points at the mark this replaced
    // =======================================================================

    @Test
    fun `no launcher resource still draws the ribbon`() {
        for (layer in LAUNCHER_LAYERS + ADAPTIVE_ICONS.map { "mipmap-anydpi-v26/$it" }) {
            val drawn = withoutComments(res(layer))
            assertFalse(
                "$layer still references the product mark; the launcher wears the Flow A",
                drawn.contains("ribbon", ignoreCase = true),
            )
        }

        // The older institutional mark is gone rather than merely unused: two
        // "A" marks in one res/ directory is how a screen quietly keeps
        // wearing the wrong one.
        assertFalse(
            "logo_flowing_a.xml is still present; the Flow A replaced it",
            File("src/main/res/drawable/logo_flowing_a.xml").exists(),
        )
        val sources = File("src/main/java").walkTopDown()
            .filter { it.isFile && it.extension == "kt" }
        for (source in sources) {
            assertFalse(
                "${source.name} still asks for R.drawable.logo_flowing_a",
                source.readText().contains("logo_flowing_a"),
            )
        }
    }

    // =======================================================================
    // B5 — the mark is the canonical one, and it is not clipped
    // =======================================================================

    @Test
    fun `the launcher foreground carries the canonical Flow A geometry`() {
        val canonical = attr(canonicalSvg("logo-flow-a.svg"), "d")
            ?: error("logo-flow-a.svg draws no path")

        for (layer in LAUNCHER_LAYERS + listOf("drawable/logo_flow_a.xml")) {
            val drawn = attr(res(layer), "android:pathData")
                ?: error("$layer draws no path")
            assertEquals(
                "$layer has been redrawn rather than generated from logo-flow-a.svg",
                canonical.replace(Regex("\\s+"), " ").trim(),
                drawn.replace(Regex("\\s+"), " ").trim(),
            )
        }

        // The stroke weight is what makes the marks a family (BRAND.md). At
        // the launcher's scale of 1 it is the same number on both sides.
        val canonicalStroke = attr(canonicalSvg("logo-flow-a.svg"), "stroke-width")!!.toDouble()
        for (layer in LAUNCHER_LAYERS) {
            assertEquals(
                "$layer's stroke has drifted from the canonical mark",
                canonicalStroke,
                attr(res(layer), "android:strokeWidth")!!.toDouble(),
                0.0001,
            )
        }
    }

    @Test
    fun `the mark stays inside every launcher mask`() {
        for (layer in LAUNCHER_LAYERS) {
            val xml = res(layer)
            assertEquals("the adaptive canvas is 108 units", "108", attr(xml, "android:viewportWidth"))
            assertEquals("the adaptive canvas is 108 units", "108", attr(xml, "android:viewportHeight"))

            val transform = group(xml)
            val stroke = attr(xml, "android:strokeWidth")!!.toDouble() * transform.scale
            val ink = flatten(attr(xml, "android:pathData")!!).map(transform::apply)
            val half = stroke / 2

            // The square safe zone: the inner 72 of 108, which every mask
            // shows. Measured against the *stroked* extent, because a
            // round-capped stroke puts ink half a width beyond its centreline.
            val left = ink.minOf { it.x } - half
            val right = ink.maxOf { it.x } + half
            val top = ink.minOf { it.y } - half
            val bottom = ink.maxOf { it.y } + half
            val squareMargin = minOf(left - SAFE_MIN, top - SAFE_MIN, SAFE_MAX - right, SAFE_MAX - bottom)
            assertTrue(
                "$layer leaves the 72x72 safe zone: ink spans x[$left, $right] y[$top, $bottom]",
                squareMargin >= 0,
            )

            // The round mask is stricter than the square one and is the one
            // that actually clips a mark whose bounding box fits: Android
            // guarantees only a 66-unit circle about the centre.
            val radius = ink.maxOf { hypot(it.x - CENTRE, it.y - CENTRE) } + half
            assertTrue(
                "$layer leaves the 66-unit round safe zone: ink reaches $radius of $ROUND_LIMIT",
                radius <= ROUND_LIMIT,
            )

            // And it should not be so small that the tile reads as mostly
            // background. This is the other half of "not clipped".
            val span = max(right - left, bottom - top)
            assertTrue(
                "$layer's mark spans only $span of the 72-unit safe zone",
                span >= 0.7 * (SAFE_MAX - SAFE_MIN),
            )
        }
    }

    @Test
    fun `the icon carries no text`() {
        for (layer in LAUNCHER_LAYERS) {
            // A VectorDrawable cannot render text at all — there is no
            // element for it — so the only way text reaches a launcher icon
            // is as a raster. Both are refused below and in `B7`.
            assertFalse(layer, withoutComments(res(layer)).contains("<text"))
        }
    }

    // =======================================================================
    // B6 — the themed layer is genuinely monochrome
    // =======================================================================

    @Test
    fun `the monochrome layer names no colour of its own`() {
        val drawn = withoutComments(res("drawable/ic_launcher_monochrome.xml"))

        assertFalse(
            "a themed icon is tinted by the system; a gradient in it is thrown away",
            drawn.contains("gradient", ignoreCase = true),
        )
        for (brand in BRAND_HEXES) {
            assertFalse(
                "the monochrome layer paints with brand colour $brand",
                drawn.contains(brand, ignoreCase = true),
            )
        }
        // Whatever colours it does name must all be the same one.
        val colours = Regex("#[0-9A-Fa-f]{6,8}").findAll(drawn)
            .map { it.value.uppercase() }
            .toSet()
        assertEquals("the monochrome layer uses exactly one colour: $colours", 1, colours.size)
    }

    @Test
    fun `the coloured foreground uses the brand gradient and nothing else`() {
        val drawn = withoutComments(res("drawable/ic_launcher_foreground.xml"))
        val colours = Regex("#[0-9A-Fa-f]{8}").findAll(drawn).map { it.value.uppercase() }.toList()
        assertEquals(
            "the foreground's stops are the brand gradient, in order",
            listOf("#FF16B8A6", "#FF4F7CFF", "#FF8B5CF6"),
            colours,
        )
        // Ink, and from the one place the platform can read before Compose runs.
        assertTrue(
            "the launcher background is Ink",
            res("values/colors.xml").contains("""<color name="ic_launcher_background">#0F172A</color>"""),
        )
    }

    // =======================================================================
    // B7 — nothing is fetched, and nothing is a screenshot
    // =======================================================================

    @Test
    fun `the launcher icon is drawn, not downloaded and not photographed`() {
        val icons = File("src/main/res").walkTopDown()
            .filter { it.isFile && it.name.startsWith("ic_launcher") }
            .toList()
        assertTrue("no launcher resources were found at all", icons.isNotEmpty())

        for (icon in icons) {
            assertEquals(
                "${icon.name} is a bitmap; a launcher icon is vector artwork",
                "xml",
                icon.extension,
            )
            val text = icon.readText()
            assertFalse("${icon.name} embeds a raster", text.contains("<bitmap"))
            // Namespace declarations are URLs but are not references to
            // anything: `xmlns:android="http://schemas.android.com/..."` names
            // a vocabulary, and nothing is ever fetched from it.
            val fetchable = withoutComments(text)
                .replace(Regex("""xmlns:[\w-]+\s*=\s*"[^"]*""""), "")
            assertFalse(
                "${icon.name} reaches outside the APK",
                Regex("https?://|content://|file://").containsMatchIn(fetchable),
            )
        }

        // And no image-loading dependency was added to draw them with.
        val build = File("build.gradle.kts").readText()
        for (library in listOf("coil", "glide", "picasso", "fresco")) {
            assertFalse(
                "$library was added; a vector drawable needs no runtime image library",
                build.contains(library, ignoreCase = true),
            )
        }
    }

    // =======================================================================
    // B8 — one manifest, so debug and release cannot disagree
    // =======================================================================

    @Test
    fun `no build variant overrides the application icon`() {
        for (variant in listOf("debug", "release")) {
            assertFalse(
                "src/$variant/AndroidManifest.xml exists and could rename or re-icon the app",
                File("src/$variant/AndroidManifest.xml").exists(),
            )
            assertFalse(
                "src/$variant/res exists and could shadow the launcher icon",
                File("src/$variant/res").exists(),
            )
        }
        // A manifest placeholder would do the same thing from inside Gradle.
        val build = File("build.gradle.kts").readText()
        assertFalse(
            "a manifest placeholder can vary android:icon per build type",
            build.contains("manifestPlaceholders"),
        )
    }

    @Test
    fun `the app label is the product name and comes from resources`() {
        assertEquals("@string/app_name", attr(withoutComments(manifest), "android:label"))
        assertTrue(
            "the launcher entry must read AnyFlow",
            res("values/strings.xml").contains("""<string name="app_name">AnyFlow</string>"""),
        )
    }

    private companion object {
        /** Points per curve when flattening. Far finer than the grid. */
        const val STEPS = 400

        /** The 108-unit canvas, and the inner 72 every mask shows. */
        const val SAFE_MIN = 18.0
        const val SAFE_MAX = 90.0
        const val CENTRE = 54.0

        /**
         * Android guarantees only a 66-unit circle about the centre, which is
         * stricter than the square and is what clips a mark whose bounding
         * box fits but whose corners do not.
         */
        const val ROUND_LIMIT = 33.0

        val ADAPTIVE_ICONS = listOf("ic_launcher.xml", "ic_launcher_round.xml")

        val LAUNCHER_LAYERS = listOf(
            "drawable/ic_launcher_foreground.xml",
            "drawable/ic_launcher_monochrome.xml",
        )

        val BRAND_HEXES = listOf("16B8A6", "4F7CFF", "8B5CF6")
    }
}
