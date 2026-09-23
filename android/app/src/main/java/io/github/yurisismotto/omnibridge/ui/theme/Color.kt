package io.github.yurisismotto.omnibridge.ui.theme

import androidx.compose.ui.graphics.Color

/**
 * OmniBridge colour tokens.
 *
 * The canonical values live in `docs/design/tokens.json` and
 * [io.github.yurisismotto.omnibridge.DesignTokensTest] reads that file and fails
 * if this table drifts from it. The desktop side is held to the same file, so
 * the two front ends cannot quietly disagree about what "connected teal" is.
 *
 * ## Why there are two sets of accent colours
 *
 * The brand hues are chosen for identity, not for legibility. Bridge Cyan on
 * white is **2.40:1** — well under the 4.5:1 WCAG AA needs for text. Using it
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
    /** Bridge Cyan — the flow origin, connected, toggles. */
    val Cyan = Color(0xFF18B8C9)

    /** Primary Blue — primary actions, links, progress, selection. */
    val Blue = Color(0xFF4F6BFF)

    /** Accent Violet — secondary accent, the far end of the sweep. */
    val Violet = Color(0xFF7C5CFC)

    /** Dark — primary text, dark surfaces, the dark-theme card. */
    val Dark = Color(0xFF0B1020)

    /** Surface — the application background in light. */
    val Surface = Color(0xFFF7F9FC)
}

/**
 * The neutral ramp. Surface is its 50 and Dark its 900, so the brand fixes
 * both ends.
 *
 * The middle stays genuinely neutral. Interpolating the ramp towards Dark's
 * own saturation looks principled and is wrong in practice: it puts a blue
 * cast on every body paragraph and every hairline border, which is the
 * opposite of the calm the palette is for.
 */
object Neutral {
    val N50 = Color(0xFFF7F9FC)
    val N100 = Color(0xFFF1F4F9)
    val N200 = Color(0xFFE2E7F0)
    val N300 = Color(0xFFCBD3E1)
    val N400 = Color(0xFF94A0B8)
    val N500 = Color(0xFF64718B)
    val N600 = Color(0xFF475269)
    val N700 = Color(0xFF333E55)
    val N800 = Color(0xFF1E263B)
    val N900 = Color(0xFF0B1020)
    val N950 = Color(0xFF05070F)
}

/** Accents corrected for text and small icons on light surfaces. All >= 4.5:1 on white and on Surface. */
object AccentOnLight {
    val Cyan = Color(0xFF10747E)
    val Blue = Color(0xFF445CDD)
    val Violet = Color(0xFF6A49EE)
    val Amber = Color(0xFFB45309)
    val Red = Color(0xFFDC2626)
}

/** Accents lifted for text and small icons on dark surfaces. All >= 6.2:1 on Dark and on surfaceElevated. */
object AccentOnDark {
    val Cyan = Color(0xFF3DC9D7)
    val Blue = Color(0xFF90A1FF)
    val Violet = Color(0xFFA28CFA)
    val Amber = Color(0xFFFBBF24)
    val Red = Color(0xFFF87171)
}

/**
 * The surface, text and accent tokens for one theme.
 *
 * Material's own [androidx.compose.material3.ColorScheme] carries what
 * Material components need. This carries what OmniBridge needs on top of it:
 * a muted text tier, an explicit hairline border colour, and the accents
 * already corrected for the current theme so a call site never has to ask
 * which one it is in.
 */
data class OmniBridgeColorScheme(
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
    val accentCyan: Color,
    val accentBlue: Color,
    val accentViolet: Color,
    val accentAmber: Color,
    val accentRed: Color,
    val isDark: Boolean,
)

val LightColors = OmniBridgeColorScheme(
    background = Brand.Surface,
    surface = Color.White,
    surfaceElevated = Color.White,
    surfaceSunken = Neutral.N100,
    border = Neutral.N200,
    borderStrong = Neutral.N300,
    textPrimary = Brand.Dark,
    textSecondary = Neutral.N600,
    textMuted = Neutral.N500,
    disabled = Neutral.N400,
    accentCyan = AccentOnLight.Cyan,
    accentBlue = AccentOnLight.Blue,
    accentViolet = AccentOnLight.Violet,
    accentAmber = AccentOnLight.Amber,
    accentRed = AccentOnLight.Red,
    isDark = false,
)

/**
 * The dark theme is derived from Dark rather than inverted from light.
 *
 * The background sits just *below* Dark and Dark itself becomes the card
 * surface, so a card reads as lifted out of the page the same way it does in
 * light. Inverting the light ramp instead would have put the lightest neutral
 * behind the darkest text and lost that relationship entirely.
 */
val DarkColors = OmniBridgeColorScheme(
    background = Color(0xFF080C18),
    surface = Brand.Dark,
    surfaceElevated = Color(0xFF141B30),
    surfaceSunken = Color(0xFF05070F),
    border = Neutral.N800,
    borderStrong = Neutral.N700,
    textPrimary = Neutral.N100,
    textSecondary = Neutral.N400,
    textMuted = Color(0xFF7C88A3),
    disabled = Neutral.N600,
    accentCyan = AccentOnDark.Cyan,
    accentBlue = AccentOnDark.Blue,
    accentViolet = AccentOnDark.Violet,
    accentAmber = AccentOnDark.Amber,
    accentRed = AccentOnDark.Red,
    isDark = true,
)
