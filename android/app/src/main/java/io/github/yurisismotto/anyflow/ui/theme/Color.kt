package io.github.yurisismotto.anyflow.ui.theme

import androidx.compose.ui.graphics.Color

/**
 * AnyFlow colour tokens.
 *
 * The canonical values live in `docs/design/tokens.json` and
 * [io.github.yurisismotto.anyflow.DesignTokensTest] reads that file and fails
 * if this table drifts from it. The desktop side is held to the same file, so
 * the two front ends cannot quietly disagree about what "connected teal" is.
 *
 * ## Why there are two sets of accent colours
 *
 * The brand hues are chosen for identity, not for legibility. Brand teal on
 * white is **2.49:1** — well under the 4.5:1 WCAG AA needs for text. Using it
 * for a "Connected" label, which is exactly what the design reference shows,
 * would make the most important word on the screen the hardest one to read.
 *
 * So the palette is split, and the split is load-bearing:
 *
 *  * [Brand] — identity. Fills, marks, gradients, the indicator dot itself.
 *    A shape big enough that its colour is decoration, not information.
 *  * [AccentOnLight] / [AccentOnDark] — the same hues corrected until they
 *    clear AA against their own surface. Every text label and every small
 *    icon uses these.
 *
 * Reaching for [Brand] where text is involved is the one mistake this file
 * exists to prevent.
 */
object Brand {
    val Teal = Color(0xFF16B8A6)
    val Blue = Color(0xFF4F7CFF)
    val Violet = Color(0xFF8B5CF6)
    val Ink = Color(0xFF0F172A)
    val Paper = Color(0xFFF8FAFC)
}

/** The slate ramp. Paper is its 50 and Ink its 900, so the brand fixes both ends. */
object Neutral {
    val N50 = Color(0xFFF8FAFC)
    val N100 = Color(0xFFF1F5F9)
    val N200 = Color(0xFFE2E8F0)
    val N300 = Color(0xFFCBD5E1)
    val N400 = Color(0xFF94A3B8)
    val N500 = Color(0xFF64748B)
    val N600 = Color(0xFF475569)
    val N700 = Color(0xFF334155)
    val N800 = Color(0xFF1E293B)
    val N900 = Color(0xFF0F172A)
    val N950 = Color(0xFF020617)
}

/** Accents corrected for text and small icons on light surfaces. All >= 4.5:1 on white. */
object AccentOnLight {
    val Teal = Color(0xFF0F766E)
    val Blue = Color(0xFF3B5BDB)
    val Violet = Color(0xFF7C3AED)
    val Amber = Color(0xFFB45309)
    val Red = Color(0xFFDC2626)
}

/** Accents lifted for text and small icons on dark surfaces. All >= 6.4:1 on Ink. */
object AccentOnDark {
    val Teal = Color(0xFF2DD4BF)
    val Blue = Color(0xFF8FA9FF)
    val Violet = Color(0xFFA78BFA)
    val Amber = Color(0xFFFBBF24)
    val Red = Color(0xFFF87171)
}

/**
 * The surface, text and accent tokens for one theme.
 *
 * Material's own [androidx.compose.material3.ColorScheme] carries what
 * Material components need. This carries what AnyFlow needs on top of it:
 * a muted text tier, an explicit hairline border colour, and the accents
 * already corrected for the current theme so a call site never has to ask
 * which one it is in.
 */
data class AnyFlowColorScheme(
    val background: Color,
    val surface: Color,
    val surfaceElevated: Color,
    val surfaceSunken: Color,
    val border: Color,
    val borderStrong: Color,
    val textPrimary: Color,
    val textSecondary: Color,
    val textMuted: Color,
    val disabled: Color,
    val accentTeal: Color,
    val accentBlue: Color,
    val accentViolet: Color,
    val accentAmber: Color,
    val accentRed: Color,
    val isDark: Boolean,
)

val LightColors = AnyFlowColorScheme(
    background = Brand.Paper,
    surface = Color.White,
    surfaceElevated = Color.White,
    surfaceSunken = Neutral.N100,
    border = Neutral.N200,
    borderStrong = Neutral.N300,
    textPrimary = Brand.Ink,
    textSecondary = Neutral.N600,
    textMuted = Neutral.N500,
    disabled = Neutral.N400,
    accentTeal = AccentOnLight.Teal,
    accentBlue = AccentOnLight.Blue,
    accentViolet = AccentOnLight.Violet,
    accentAmber = AccentOnLight.Amber,
    accentRed = AccentOnLight.Red,
    isDark = false,
)

/**
 * Dark is derived from Ink rather than inverted from light.
 *
 * The background sits just *below* Ink and Ink itself becomes the card
 * surface, so a card reads as lifted out of the page the same way it does in
 * light. Inverting the light ramp instead would have put the lightest neutral
 * behind the darkest text and lost that relationship entirely.
 */
val DarkColors = AnyFlowColorScheme(
    background = Color(0xFF0A0F1C),
    surface = Brand.Ink,
    surfaceElevated = Color(0xFF1B2436),
    surfaceSunken = Color(0xFF070B14),
    border = Neutral.N800,
    borderStrong = Neutral.N700,
    textPrimary = Neutral.N100,
    textSecondary = Neutral.N400,
    textMuted = Color(0xFF7C8CA3),
    disabled = Neutral.N600,
    accentTeal = AccentOnDark.Teal,
    accentBlue = AccentOnDark.Blue,
    accentViolet = AccentOnDark.Violet,
    accentAmber = AccentOnDark.Amber,
    accentRed = AccentOnDark.Red,
    isDark = true,
)
