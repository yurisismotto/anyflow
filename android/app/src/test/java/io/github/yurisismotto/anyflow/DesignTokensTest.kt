package io.github.yurisismotto.anyflow

import io.github.yurisismotto.anyflow.ui.theme.AccentOnDark
import io.github.yurisismotto.anyflow.ui.theme.AccentOnLight
import io.github.yurisismotto.anyflow.ui.theme.AnyFlowGradient
import io.github.yurisismotto.anyflow.ui.theme.AnyFlowRadius
import io.github.yurisismotto.anyflow.ui.theme.AnyFlowSpacing
import io.github.yurisismotto.anyflow.ui.theme.Brand
import io.github.yurisismotto.anyflow.ui.theme.DarkColors
import io.github.yurisismotto.anyflow.ui.theme.LightColors
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
 * make the accessibility claim executable rather than merely stated: brand
 * teal on white is 2.49:1, and the whole reason [AccentOnLight] exists apart
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
        assertEquals(hex("brand", "teal"), Brand.Teal.hex())
        assertEquals(hex("brand", "blue"), Brand.Blue.hex())
        assertEquals(hex("brand", "violet"), Brand.Violet.hex())
        assertEquals(hex("brand", "ink"), Brand.Ink.hex())
        assertEquals(hex("brand", "paper"), Brand.Paper.hex())
    }

    @Test
    fun `the corrected accents match the canonical tokens`() {
        assertEquals(hex("on_light", "teal"), AccentOnLight.Teal.hex())
        assertEquals(hex("on_light", "blue"), AccentOnLight.Blue.hex())
        assertEquals(hex("on_light", "violet"), AccentOnLight.Violet.hex())
        assertEquals(hex("on_light", "amber"), AccentOnLight.Amber.hex())
        assertEquals(hex("on_light", "red"), AccentOnLight.Red.hex())
        assertEquals(hex("on_dark", "teal"), AccentOnDark.Teal.hex())
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
        assertEquals(cta.length(), AnyFlowGradient.ctaStops.size)
        AnyFlowGradient.ctaStops.forEachIndexed { i, colour ->
            assertEquals(cta.getString(i).uppercase(), colour.hex())
        }
        val brand = tokens.getJSONObject("gradient").getJSONObject("brand").getJSONArray("stops")
        AnyFlowGradient.decorativeStops.forEachIndexed { i, colour ->
            assertEquals(brand.getString(i).uppercase(), colour.hex())
        }
    }

    @Test
    fun `the scales match the canonical tokens`() {
        val spacing = tokens.getJSONObject("spacing")
        assertEquals(spacing.getInt("xxs").toFloat(), AnyFlowSpacing.xxs.value, 0f)
        assertEquals(spacing.getInt("xs").toFloat(), AnyFlowSpacing.xs.value, 0f)
        assertEquals(spacing.getInt("sm").toFloat(), AnyFlowSpacing.sm.value, 0f)
        assertEquals(spacing.getInt("md").toFloat(), AnyFlowSpacing.md.value, 0f)
        assertEquals(spacing.getInt("lg").toFloat(), AnyFlowSpacing.lg.value, 0f)
        assertEquals(spacing.getInt("xl").toFloat(), AnyFlowSpacing.xl.value, 0f)
        assertEquals(spacing.getInt("xxl").toFloat(), AnyFlowSpacing.xxl.value, 0f)

        val radius = tokens.getJSONObject("radius")
        assertEquals(radius.getInt("small").toFloat(), AnyFlowRadius.small.value, 0f)
        assertEquals(radius.getInt("medium").toFloat(), AnyFlowRadius.medium.value, 0f)
        assertEquals(radius.getInt("large").toFloat(), AnyFlowRadius.large.value, 0f)
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
            "teal" to AccentOnLight.Teal,
            "blue" to AccentOnLight.Blue,
            "violet" to AccentOnLight.Violet,
            "amber" to AccentOnLight.Amber,
            "red" to AccentOnLight.Red,
        ).forEach { (name, colour) ->
            val onWhite = contrast(colour.hex(), "#FFFFFF")
            val onPaper = contrast(colour.hex(), Brand.Paper.hex())
            assertTrue("AccentOnLight.$name is $onWhite:1 on white", onWhite >= floor)
            assertTrue("AccentOnLight.$name is $onPaper:1 on Paper", onPaper >= floor)
        }
        listOf(
            "teal" to AccentOnDark.Teal,
            "blue" to AccentOnDark.Blue,
            "violet" to AccentOnDark.Violet,
            "amber" to AccentOnDark.Amber,
            "red" to AccentOnDark.Red,
        ).forEach { (name, colour) ->
            val onSurface = contrast(colour.hex(), DarkColors.surface.hex())
            assertTrue("AccentOnDark.$name is $onSurface:1 on the dark surface", onSurface >= floor)
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
     * A guard, not a curiosity: if a palette change ever made brand teal
     * legible as a label, the two-family split could be collapsed — and this
     * failing is how anyone would find out.
     */
    @Test
    fun `the raw brand hues are known to fail as text`() {
        val floor = tokens.getJSONObject("contrast_floor").getDouble("text_on_surface")
        assertTrue(
            "brand teal now passes AA on white — revisit the two-family palette " +
                "split in docs/design/BRAND.md before using it for text",
            contrast(Brand.Teal.hex(), "#FFFFFF") < floor,
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
        AnyFlowGradient.ctaStops.zipWithNext { a, b ->
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
}
