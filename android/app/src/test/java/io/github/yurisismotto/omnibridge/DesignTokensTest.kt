package io.github.yurisismotto.omnibridge

import io.github.yurisismotto.omnibridge.ui.theme.AccentOnDark
import io.github.yurisismotto.omnibridge.ui.theme.AccentOnLight
import io.github.yurisismotto.omnibridge.ui.theme.OmniBridgeGradient
import io.github.yurisismotto.omnibridge.ui.theme.OmniBridgeRadius
import io.github.yurisismotto.omnibridge.ui.theme.OmniBridgeSpacing
import io.github.yurisismotto.omnibridge.ui.theme.Brand
import io.github.yurisismotto.omnibridge.ui.theme.Neutral
import io.github.yurisismotto.omnibridge.ui.theme.OmniBridgeLayout
import io.github.yurisismotto.omnibridge.ui.theme.OmniBridgeStatus
import io.github.yurisismotto.omnibridge.ui.theme.DarkColors
import io.github.yurisismotto.omnibridge.ui.theme.LightColors
import androidx.compose.ui.graphics.Color
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import kotlin.math.pow

/**
 * The design tokens, checked against the cross-platform contract.
 *
 * `docs/design/tokens.json` is the single source of truth for both front
 * ends, and the desktop has a mirror of this test in
 * `desktop/gui/src/theme.rs`. Change a value in one place and the other side
 * fails — which is the only thing that stops two apps that are supposed to be
 * one product from drifting apart a hex at a time.
 *
 * The contrast assertions matter more than the equality ones. They are what
 * make the accessibility claim executable rather than merely stated: Bridge
 * Cyan on white is 2.40:1, and the whole reason [AccentOnLight] exists apart
 * from [Brand] is that a label may never be painted with it.
 */
class DesignTokensTest {

    private val tokens: JSONObject by lazy {
        val stream = javaClass.classLoader!!.getResourceAsStream("tokens.json")
            ?: error("docs/design/tokens.json is not on the test classpath")
        JSONObject(stream.bufferedReader().readText())
    }

    private fun hex(vararg path: String): String {
        var node = tokens
        for (key in path.dropLast(1)) node = node.getJSONObject(key)
        return node.getString(path.last()).uppercase()
    }

    private fun Color.hex(): String {
        val argb = (alpha * 255).toInt().shl(24) or
            (red * 255).toInt().shl(16) or
            (green * 255).toInt().shl(8) or
            (blue * 255).toInt()
        return String.format("#%06X", argb and 0xFFFFFF)
    }

    @Test
    fun `the brand palette matches the canonical tokens`() {
        assertEquals(hex("brand", "cyan"), Brand.Cyan.hex())
        assertEquals(hex("brand", "blue"), Brand.Blue.hex())
        assertEquals(hex("brand", "violet"), Brand.Violet.hex())
        assertEquals(hex("brand", "dark"), Brand.Dark.hex())
        assertEquals(hex("brand", "surface"), Brand.Surface.hex())
    }

    @Test
    fun `the corrected accents match the canonical tokens`() {
        assertEquals(hex("on_light", "cyan"), AccentOnLight.Cyan.hex())
        assertEquals(hex("on_light", "blue"), AccentOnLight.Blue.hex())
        assertEquals(hex("on_light", "violet"), AccentOnLight.Violet.hex())
        assertEquals(hex("on_light", "amber"), AccentOnLight.Amber.hex())
        assertEquals(hex("on_light", "red"), AccentOnLight.Red.hex())
        assertEquals(hex("on_dark", "cyan"), AccentOnDark.Cyan.hex())
        assertEquals(hex("on_dark", "blue"), AccentOnDark.Blue.hex())
        assertEquals(hex("on_dark", "violet"), AccentOnDark.Violet.hex())
        assertEquals(hex("on_dark", "amber"), AccentOnDark.Amber.hex())
        assertEquals(hex("on_dark", "red"), AccentOnDark.Red.hex())
    }

    @Test
    fun `the semantic surfaces match the canonical tokens`() {
        assertEquals(hex("semantic", "light", "background"), LightColors.background.hex())
        assertEquals(hex("semantic", "light", "surface"), LightColors.surface.hex())
        assertEquals(hex("semantic", "light", "border"), LightColors.border.hex())
        assertEquals(hex("semantic", "light", "text_primary"), LightColors.textPrimary.hex())
        assertEquals(hex("semantic", "light", "text_secondary"), LightColors.textSecondary.hex())
        assertEquals(hex("semantic", "dark", "background"), DarkColors.background.hex())
        assertEquals(hex("semantic", "dark", "surface"), DarkColors.surface.hex())
        assertEquals(hex("semantic", "dark", "border"), DarkColors.border.hex())
        assertEquals(hex("semantic", "dark", "text_primary"), DarkColors.textPrimary.hex())
        assertEquals(hex("semantic", "dark", "text_secondary"), DarkColors.textSecondary.hex())
    }

    @Test
    fun `the gradients match the canonical tokens`() {
        val cta = tokens.getJSONObject("gradient").getJSONObject("cta").getJSONArray("stops")
        assertEquals(cta.length(), OmniBridgeGradient.ctaStops.size)
        OmniBridgeGradient.ctaStops.forEachIndexed { i, colour ->
            assertEquals(cta.getString(i).uppercase(), colour.hex())
        }
        val brand = tokens.getJSONObject("gradient").getJSONObject("brand").getJSONArray("stops")
        OmniBridgeGradient.decorativeStops.forEachIndexed { i, colour ->
            assertEquals(brand.getString(i).uppercase(), colour.hex())
        }
    }

    @Test
    fun `the scales match the canonical tokens`() {
        val spacing = tokens.getJSONObject("spacing")
        assertEquals(spacing.getInt("xxs").toFloat(), OmniBridgeSpacing.xxs.value, 0f)
        assertEquals(spacing.getInt("xs").toFloat(), OmniBridgeSpacing.xs.value, 0f)
        assertEquals(spacing.getInt("sm").toFloat(), OmniBridgeSpacing.sm.value, 0f)
        assertEquals(spacing.getInt("md").toFloat(), OmniBridgeSpacing.md.value, 0f)
        assertEquals(spacing.getInt("lg").toFloat(), OmniBridgeSpacing.lg.value, 0f)
        assertEquals(spacing.getInt("xl").toFloat(), OmniBridgeSpacing.xl.value, 0f)
        assertEquals(spacing.getInt("xxl").toFloat(), OmniBridgeSpacing.xxl.value, 0f)

        val radius = tokens.getJSONObject("radius")
        assertEquals(radius.getInt("small").toFloat(), OmniBridgeRadius.small.value, 0f)
        assertEquals(radius.getInt("medium").toFloat(), OmniBridgeRadius.medium.value, 0f)
        assertEquals(radius.getInt("large").toFloat(), OmniBridgeRadius.large.value, 0f)
    }

    // ---- the part that makes the accessibility claim executable ----------

    private fun luminance(hex: String): Double {
        val h = hex.removePrefix("#")
        fun channel(i: Int): Double {
            val v = h.substring(i, i + 2).toInt(16) / 255.0
            return if (v <= 0.04045) v / 12.92 else ((v + 0.055) / 1.055).pow(2.4)
        }
        return 0.2126 * channel(0) + 0.7152 * channel(2) + 0.0722 * channel(4)
    }

    private fun contrast(a: String, b: String): Double {
        val la = luminance(a)
        val lb = luminance(b)
        val hi = maxOf(la, lb)
        val lo = minOf(la, lb)
        return (hi + 0.05) / (lo + 0.05)
    }

    @Test
    fun `every text accent clears AA on its own surface`() {
        val floor = tokens.getJSONObject("contrast_floor").getDouble("text_on_surface")
        listOf(
            "cyan" to AccentOnLight.Cyan,
            "blue" to AccentOnLight.Blue,
            "violet" to AccentOnLight.Violet,
            "amber" to AccentOnLight.Amber,
            "red" to AccentOnLight.Red,
        ).forEach { (name, colour) ->
            val onWhite = contrast(colour.hex(), "#FFFFFF")
            val onSurface = contrast(colour.hex(), Brand.Surface.hex())
            assertTrue("AccentOnLight.$name is $onWhite:1 on white", onWhite >= floor)
            assertTrue("AccentOnLight.$name is $onSurface:1 on Surface", onSurface >= floor)
        }
        listOf(
            "cyan" to AccentOnDark.Cyan,
            "blue" to AccentOnDark.Blue,
            "violet" to AccentOnDark.Violet,
            "amber" to AccentOnDark.Amber,
            "red" to AccentOnDark.Red,
        ).forEach { (name, colour) ->
            val onDarkSurface = contrast(colour.hex(), DarkColors.surface.hex())
            assertTrue(
                "AccentOnDark.$name is $onDarkSurface:1 on the dark surface",
                onDarkSurface >= floor,
            )
            // Elevated is the lighter of the two dark surfaces, so it is the
            // harder one. A card sitting on an elevated sheet must stay AA too.
            val onElevated = contrast(colour.hex(), DarkColors.surfaceElevated.hex())
            assertTrue(
                "AccentOnDark.$name is $onElevated:1 on the elevated dark surface",
                onElevated >= floor,
            )
        }
    }

    @Test
    fun `the body text tiers clear AA on their own surfaces`() {
        val floor = tokens.getJSONObject("contrast_floor").getDouble("text_on_surface")
        listOf(
            "textPrimary" to LightColors.textPrimary,
            "textSecondary" to LightColors.textSecondary,
            "textMuted" to LightColors.textMuted,
        ).forEach { (name, colour) ->
            val ratio = contrast(colour.hex(), LightColors.surface.hex())
            assertTrue("light $name is $ratio:1", ratio >= floor)
        }
        listOf(
            "textPrimary" to DarkColors.textPrimary,
            "textSecondary" to DarkColors.textSecondary,
            "textMuted" to DarkColors.textMuted,
        ).forEach { (name, colour) ->
            val ratio = contrast(colour.hex(), DarkColors.surface.hex())
            assertTrue("dark $name is $ratio:1", ratio >= floor)
        }
    }

    /**
     * The brand hues are known to fail as text, and that must stay true.
     *
     * A guard, not a curiosity: if a palette change ever made Bridge Cyan
     * legible as a label, the two-family split could be collapsed — and this
     * failing is how anyone would find out.
     */
    @Test
    fun `the raw brand hues are known to fail as text`() {
        val floor = tokens.getJSONObject("contrast_floor").getDouble("text_on_surface")
        assertTrue(
            "Bridge Cyan now passes AA on white — revisit the two-family palette " +
                "split in docs/design/BRAND.md before using it for text",
            contrast(Brand.Cyan.hex(), "#FFFFFF") < floor,
        )
    }

    /**
     * White must stay legible across the whole CTA sweep, not just its stops.
     *
     * Sampling the interpolation is the point: a gradient can pass at both
     * ends and fail in the middle, and the button label sits on all of it.
     */
    @Test
    fun `white stays legible across the whole CTA gradient`() {
        val floor = tokens.getJSONObject("contrast_floor").getDouble("white_on_cta_gradient")
        var worst = Double.MAX_VALUE
        var worstAt = ""
        OmniBridgeGradient.ctaStops.zipWithNext { a, b ->
            for (step in 0..20) {
                val t = step / 20f
                val mixed = Color(
                    red = a.red + (b.red - a.red) * t,
                    green = a.green + (b.green - a.green) * t,
                    blue = a.blue + (b.blue - a.blue) * t,
                )
                val ratio = contrast("#FFFFFF", mixed.hex())
                if (ratio < worst) {
                    worst = ratio
                    worstAt = mixed.hex()
                }
            }
        }
        assertTrue(
            "white on the CTA gradient falls to $worst:1 at $worstAt",
            worst >= floor,
        )
    }

    @Test
    fun `the neutral ramp matches the canonical tokens and is anchored by the brand`() {
        val ramp = tokens.getJSONObject("neutral")
        listOf(
            "50" to Neutral.N50, "100" to Neutral.N100, "200" to Neutral.N200,
            "300" to Neutral.N300, "400" to Neutral.N400, "500" to Neutral.N500,
            "600" to Neutral.N600, "700" to Neutral.N700, "800" to Neutral.N800,
            "900" to Neutral.N900, "950" to Neutral.N950,
        ).forEach { (step, colour) ->
            assertEquals("neutral.$step", ramp.getString(step).uppercase(), colour.hex())
        }
        // The two ends are the brand, not merely near it. This is what makes
        // "Surface and Dark are the ends of one ramp" a fact rather than a
        // description that drifted.
        assertEquals(Brand.Surface.hex(), Neutral.N50.hex())
        assertEquals(Brand.Dark.hex(), Neutral.N900.hex())
    }

    @Test
    fun `the status colours match the canonical tokens`() {
        val status = tokens.getJSONObject("status")
        fun expect(name: String, key: String) = status.getJSONObject(name).getString(key).uppercase()

        // dot() is theme-independent, so it can be checked without composing.
        listOf(
            "connected" to OmniBridgeStatus.Connected,
            "success" to OmniBridgeStatus.Success,
            "available" to OmniBridgeStatus.Available,
            "transferring" to OmniBridgeStatus.Transferring,
            "warning" to OmniBridgeStatus.Warning,
            "stale" to OmniBridgeStatus.Stale,
            "error" to OmniBridgeStatus.Error,
            "revoked" to OmniBridgeStatus.Revoked,
            "disconnected" to OmniBridgeStatus.Disconnected,
        ).forEach { (name, value) ->
            assertEquals("status.$name.dot", expect(name, "dot"), value.dot().hex())
        }
    }

    /**
     * Every status still carries a word and an icon, not just a colour.
     *
     * Rule 1 of the UI guidelines, made executable. A status added as a
     * colour and nothing else is the failure mode this catches.
     */
    @Test
    fun `every status carries a word and an icon`() {
        OmniBridgeStatus.entries.forEach { status ->
            assertTrue("${status.name} has no label", status.label.isNotBlank())
            assertTrue("${status.name} has no icon", status.icon != 0)
        }
        // And the words are distinct: two statuses that read the same are not
        // distinguishable by a screen reader either.
        val labels = OmniBridgeStatus.entries.map { it.label }
        assertEquals(labels.size, labels.toSet().size)
    }

    /**
     * Text must clear AA on the *background* as well as on a card.
     *
     * The background is Surface, not white, so a tier that only ever got
     * checked against white was checked against the easier of the two.
     */
    @Test
    fun `the body text tiers clear AA on the background too`() {
        val floor = tokens.getJSONObject("contrast_floor").getDouble("text_on_surface")
        listOf(
            "textPrimary" to LightColors.textPrimary,
            "textSecondary" to LightColors.textSecondary,
            "textMuted" to LightColors.textMuted,
        ).forEach { (name, colour) ->
            val ratio = contrast(colour.hex(), LightColors.background.hex())
            assertTrue("light $name is $ratio:1 on the background", ratio >= floor)
        }
        listOf(
            "textPrimary" to DarkColors.textPrimary,
            "textSecondary" to DarkColors.textSecondary,
            "textMuted" to DarkColors.textMuted,
        ).forEach { (name, colour) ->
            val onBackground = contrast(colour.hex(), DarkColors.background.hex())
            val onElevated = contrast(colour.hex(), DarkColors.surfaceElevated.hex())
            assertTrue("dark $name is $onBackground:1 on the background", onBackground >= floor)
            assertTrue("dark $name is $onElevated:1 on the elevated surface", onElevated >= floor)
        }
    }

    /**
     * The dark surfaces must stay ordered, or the card stops reading as lifted.
     *
     * Dark is derived from Dark rather than inverted, and the whole point of
     * that derivation is this ordering: sunken < background < surface <
     * elevated. Nudging one value without the others is how it gets lost.
     */
    @Test
    fun `the dark surfaces stay ordered so a card still reads as lifted`() {
        fun lum(c: androidx.compose.ui.graphics.Color) = luminance(c.hex())
        assertTrue(
            "dark surfaces are out of order",
            lum(DarkColors.surfaceSunken) < lum(DarkColors.background) &&
                lum(DarkColors.background) < lum(DarkColors.surface) &&
                lum(DarkColors.surface) < lum(DarkColors.surfaceElevated),
        )
    }

    /**
     * The CTA gradient is the corrected accent triple, not a fourth set.
     *
     * Keeping them the same object is what stops the button's gradient and
     * the app's accent colours drifting apart the next time one is retuned.
     */
    @Test
    fun `the CTA gradient is exactly the corrected accent triple`() {
        assertEquals(
            listOf(AccentOnLight.Cyan, AccentOnLight.Blue, AccentOnLight.Violet),
            OmniBridgeGradient.ctaStops,
        )
    }

    @Test
    fun `the layout tokens match the canonical tokens`() {
        val layout = tokens.getJSONObject("layout")
        assertEquals(layout.getInt("content_max").toFloat(), OmniBridgeLayout.contentMax.value, 0f)
        assertEquals(layout.getInt("compact_max").toFloat(), OmniBridgeLayout.compactMax.value, 0f)
    }
}
