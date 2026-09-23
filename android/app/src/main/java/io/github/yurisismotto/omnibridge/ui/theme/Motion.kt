package io.github.yurisismotto.omnibridge.ui.theme

import androidx.compose.animation.core.CubicBezierEasing
import androidx.compose.animation.core.Easing

/**
 * Motion tokens.
 *
 * Movement in OmniBridge means flow: something travelling from one device to
 * another. It is short, it eases, and it never loops for decoration alone.
 *
 * Every duration here is read through
 * [io.github.yurisismotto.omnibridge.ui.theme.rememberReducedMotion], which
 * returns zero when the person has asked the system to remove animation. A
 * value used raw is a bug.
 */
object OmniBridgeMotion {
    const val INSTANT_MS = 90
    const val FAST_MS = 160
    const val NORMAL_MS = 240
    const val SLOW_MS = 400

    /** One pass of the ribbon pulse on a connecting/transferring surface. */
    const val RIBBON_PULSE_MS = 2200

    /** Standard easing: quick to leave, gentle to arrive. */
    val Flow: Easing = CubicBezierEasing(0.2f, 0f, 0f, 1f)

    /** For something entering the screen. */
    val Enter: Easing = CubicBezierEasing(0.05f, 0.7f, 0.1f, 1f)
}
