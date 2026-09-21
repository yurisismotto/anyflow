package io.github.yurisismotto.omnibridge.ui.theme

import androidx.compose.material3.Typography
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.sp

/**
 * The OmniBridge type system.
 *
 * ## Which face this actually renders in
 *
 * Inter is the brand direction. It is **not bundled**, and that is a
 * deliberate choice rather than an omission: shipping four Inter weights adds
 * roughly a megabyte to an APK whose whole point is to be small, to replace a
 * system face that is already a neo-grotesque with the same tall x-height and
 * near-identical metrics. The brief allows a documented native equivalent
 * where the platform does not want the bundle, and Android is that case.
 *
 * So the scale below — the sizes, weights, line heights and tracking, which
 * are the part a design system actually owns — is applied to the platform UI
 * face. Substituting Inter later is a one-line change to [Ui]: nothing else
 * in the app names a font.
 *
 * Fingerprints and device ids use [Mono]. That is not decoration either: a
 * fingerprint is compared character by character against another screen, and
 * a proportional face makes `1` / `l` and `0` / `O` a coin toss.
 */
object OmniBridgeType {
    val Ui: FontFamily = FontFamily.SansSerif
    val Mono: FontFamily = FontFamily.Monospace

    val display = TextStyle(
        fontFamily = Ui, fontSize = 32.sp, fontWeight = FontWeight.Bold,
        lineHeight = 40.sp, letterSpacing = (-0.5).sp,
    )
    val title = TextStyle(
        fontFamily = Ui, fontSize = 24.sp, fontWeight = FontWeight.Bold,
        lineHeight = 32.sp, letterSpacing = (-0.25).sp,
    )
    val heading = TextStyle(
        fontFamily = Ui, fontSize = 20.sp, fontWeight = FontWeight.SemiBold,
        lineHeight = 28.sp,
    )
    val subtitle = TextStyle(
        fontFamily = Ui, fontSize = 16.sp, fontWeight = FontWeight.SemiBold,
        lineHeight = 24.sp,
    )
    val body = TextStyle(
        fontFamily = Ui, fontSize = 14.sp, fontWeight = FontWeight.Normal,
        lineHeight = 20.sp,
    )
    val label = TextStyle(
        fontFamily = Ui, fontSize = 13.sp, fontWeight = FontWeight.Medium,
        lineHeight = 18.sp, letterSpacing = 0.1.sp,
    )
    val caption = TextStyle(
        fontFamily = Ui, fontSize = 12.sp, fontWeight = FontWeight.Normal,
        lineHeight = 16.sp, letterSpacing = 0.2.sp,
    )

    /** Fingerprints, device ids, hashes. Never body copy. */
    val mono = TextStyle(
        fontFamily = Mono, fontSize = 13.sp, fontWeight = FontWeight.Normal,
        lineHeight = 20.sp,
    )
}

/**
 * The OmniBridge scale expressed as a Material 3 [Typography].
 *
 * Material components are kept, not replaced, so anything drawn by Material3
 * inherits the brand scale without every call site having to pass a style.
 */
val OmniBridgeTypography = Typography(
    displayLarge = OmniBridgeType.display.copy(fontSize = 40.sp, lineHeight = 48.sp),
    displayMedium = OmniBridgeType.display.copy(fontSize = 36.sp, lineHeight = 44.sp),
    displaySmall = OmniBridgeType.display,
    headlineLarge = OmniBridgeType.title.copy(fontSize = 28.sp, lineHeight = 36.sp),
    headlineMedium = OmniBridgeType.title,
    headlineSmall = OmniBridgeType.heading,
    titleLarge = OmniBridgeType.heading,
    titleMedium = OmniBridgeType.subtitle,
    titleSmall = OmniBridgeType.body.copy(fontWeight = FontWeight.SemiBold),
    bodyLarge = OmniBridgeType.body.copy(fontSize = 16.sp, lineHeight = 24.sp),
    bodyMedium = OmniBridgeType.body,
    bodySmall = OmniBridgeType.caption,
    labelLarge = OmniBridgeType.body.copy(fontWeight = FontWeight.Medium),
    labelMedium = OmniBridgeType.label,
    labelSmall = OmniBridgeType.caption.copy(fontWeight = FontWeight.Medium, fontSize = 11.sp),
)
