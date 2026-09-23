package io.github.yurisismotto.omnibridge.ui.theme

import androidx.annotation.DrawableRes
import androidx.compose.runtime.Composable
import androidx.compose.runtime.ReadOnlyComposable
import androidx.compose.ui.graphics.Color
import io.github.yurisismotto.omnibridge.R

/**
 * The status vocabulary, in one place.
 *
 * ## The rule this type exists to enforce
 *
 * **Nothing in OmniBridge says something with colour alone.** Roughly one man in
 * twelve cannot separate the cyan of "connected" from the amber of "stale",
 * and both of those are claims about whether the thing on screen is true
 * right now. So a status is never a coloured dot: it is a dot, an icon and a
 * word, and the word is the part that carries the meaning.
 *
 * Because every status is declared here with all three, a new one cannot be
 * added as a colour and nothing else — there is no constructor for that.
 *
 * ## Why [dot] is separate from [color]
 *
 * [dot] is the brand hue at full strength, used for a filled shape big enough
 * that its colour is decoration. [color] is the corrected hue that clears
 * WCAG AA, used for the label and the icon beside it. See [Brand].
 */
enum class OmniBridgeStatus(
    val label: String,
    @DrawableRes val icon: Int,
) {
    /** A live session exists and the device is talking to us. */
    Connected("Connected", R.drawable.ic_link),

    /** Paired and reachable, but no session right now. */
    Available("Available", R.drawable.ic_device_desktop),

    Connecting("Connecting…", R.drawable.ic_activity),

    Transferring("Transferring", R.drawable.ic_send),

    Success("Done", R.drawable.ic_check),

    /**
     * A session exists but nothing has arrived for long enough that anything
     * the device last told us should be read as history rather than fact.
     */
    Stale("Not responding", R.drawable.ic_warning),

    Warning("Needs attention", R.drawable.ic_warning),

    Error("Failed", R.drawable.ic_warning),

    Disconnected("Disconnected", R.drawable.ic_link_off),

    /** Trust was withdrawn. The device cannot connect until it pairs again. */
    Revoked("Revoked", R.drawable.ic_shield_off);

    /** The AA-corrected hue, for the label and any icon at label size. */
    @Composable
    @ReadOnlyComposable
    fun color(): Color {
        val c = LocalOmniBridgeColors.current
        return when (this) {
            Connected, Success -> c.accentCyan
            Available, Connecting, Transferring -> c.accentBlue
            Stale, Warning -> c.accentAmber
            Error, Revoked -> c.accentRed
            Disconnected -> c.textMuted
        }
    }

    /** The brand hue at full strength, for the indicator dot only. */
    fun dot(): Color = when (this) {
        Connected, Success -> Brand.Cyan
        Available, Connecting, Transferring -> Brand.Blue
        Stale, Warning -> Color(0xFFF59E0B)
        Error, Revoked -> Color(0xFFEF4444)
        Disconnected -> Neutral.N400
    }

    /** True for states that should read as calm rather than as a problem. */
    val isPositive: Boolean get() = this == Connected || this == Success
}
