package io.github.yurisismotto.omnibridge.ui.theme

import android.provider.Settings
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.ProvidableCompositionLocal
import androidx.compose.runtime.ReadOnlyComposable
import androidx.compose.runtime.remember
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext

/**
 * The OmniBridge tokens for the current theme.
 *
 * Static rather than dynamic: the scheme changes only when the theme does,
 * and a static local avoids re-composing every reader on unrelated changes.
 */
val LocalOmniBridgeColors: ProvidableCompositionLocal<OmniBridgeColorScheme> =
    staticCompositionLocalOf { LightColors }

/** True when the person has asked the system to remove animation. */
val LocalReducedMotion: ProvidableCompositionLocal<Boolean> =
    staticCompositionLocalOf { false }

/**
 * Reads the platform's animator scale.
 *
 * Android has no single "reduce motion" switch; `ANIMATOR_DURATION_SCALE` set
 * to zero is what both the accessibility setting and Developer Options
 * actually write, and it is what the platform's own animations honour.
 */
@Composable
fun rememberReducedMotion(): Boolean {
    val resolver = LocalContext.current.contentResolver
    return remember(resolver) {
        runCatching {
            Settings.Global.getFloat(resolver, Settings.Global.ANIMATOR_DURATION_SCALE, 1f) == 0f
        }.getOrDefault(false)
    }
}

/** Scales a motion duration to zero when animation is switched off. */
@Composable
@ReadOnlyComposable
fun motionDuration(millis: Int): Int = if (LocalReducedMotion.current) 0 else millis

/**
 * Material 3 built from the OmniBridge tokens.
 *
 * Material semantics are kept deliberately. OmniBridge should look like OmniBridge
 * *on Android* — a Switch still behaves and reads as an Android Switch, a
 * dialog still sits where Android puts one — rather than like a foreign
 * design language pasted onto the platform.
 *
 * Dynamic colour is not used. OmniBridge's palette carries meaning: teal is
 * "connected", amber is "stale", red is "revoked". Letting the wallpaper
 * recolour that would recolour the status language with it.
 */
private fun materialLight() = lightColorScheme(
    primary = AccentOnLight.Blue,
    onPrimary = Color.White,
    primaryContainer = Color(0xFFE6ECFF),
    onPrimaryContainer = Color(0xFF16255C),
    secondary = AccentOnLight.Cyan,
    onSecondary = Color.White,
    secondaryContainer = Color(0xFFD9F2EE),
    onSecondaryContainer = Color(0xFF05332E),
    tertiary = AccentOnLight.Violet,
    onTertiary = Color.White,
    tertiaryContainer = Color(0xFFEDE4FE),
    onTertiaryContainer = Color(0xFF2C1065),
    error = AccentOnLight.Red,
    onError = Color.White,
    errorContainer = Color(0xFFFDE7E7),
    onErrorContainer = Color(0xFF5C1010),
    background = LightColors.background,
    onBackground = LightColors.textPrimary,
    surface = LightColors.surface,
    onSurface = LightColors.textPrimary,
    surfaceVariant = LightColors.surfaceSunken,
    onSurfaceVariant = LightColors.textSecondary,
    outline = LightColors.borderStrong,
    outlineVariant = LightColors.border,
)

private fun materialDark() = darkColorScheme(
    primary = AccentOnDark.Blue,
    onPrimary = Color(0xFF0A1330),
    primaryContainer = Color(0xFF1D2A52),
    onPrimaryContainer = Color(0xFFD7E0FF),
    secondary = AccentOnDark.Cyan,
    onSecondary = Color(0xFF00201C),
    secondaryContainer = Color(0xFF10453F),
    onSecondaryContainer = Color(0xFFB6F0E7),
    tertiary = AccentOnDark.Violet,
    onTertiary = Color(0xFF1C0A3E),
    tertiaryContainer = Color(0xFF382263),
    onTertiaryContainer = Color(0xFFE7DBFF),
    error = AccentOnDark.Red,
    onError = Color(0xFF3A0A0A),
    errorContainer = Color(0xFF5C1B1B),
    onErrorContainer = Color(0xFFFFD9D9),
    background = DarkColors.background,
    onBackground = DarkColors.textPrimary,
    surface = DarkColors.surface,
    onSurface = DarkColors.textPrimary,
    surfaceVariant = DarkColors.surfaceElevated,
    onSurfaceVariant = DarkColors.textSecondary,
    outline = DarkColors.borderStrong,
    outlineVariant = DarkColors.border,
)

@Composable
fun OmniBridgeTheme(
    darkTheme: Boolean = isSystemInDarkTheme(),
    content: @Composable () -> Unit,
) {
    val tokens = if (darkTheme) DarkColors else LightColors
    CompositionLocalProvider(
        LocalOmniBridgeColors provides tokens,
        LocalReducedMotion provides rememberReducedMotion(),
    ) {
        MaterialTheme(
            colorScheme = if (darkTheme) materialDark() else materialLight(),
            typography = OmniBridgeTypography,
            content = content,
        )
    }
}

/** Shorthand for the OmniBridge tokens at a call site: `OmniBridgeTheme.colors`. */
object OmniBridgeTheme {
    val colors: OmniBridgeColorScheme
        @Composable @ReadOnlyComposable get() = LocalOmniBridgeColors.current
}
