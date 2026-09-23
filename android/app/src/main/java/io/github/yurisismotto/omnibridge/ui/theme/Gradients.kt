package io.github.yurisismotto.omnibridge.ui.theme

import androidx.compose.ui.graphics.Brush

/**
 * The brand gradients.
 *
 * There are two, and picking the wrong one is an accessibility bug rather
 * than a taste question.
 *
 *  * [decorative] is the identity gradient, cyan to blue to violet. It is for
 *    marks, ribbons and progress fills — surfaces with **no text on them**.
 *    White on its cyan end is 2.40:1.
 *  * [cta] is the one that carries a label. It is the corrected accent triple,
 *    so white clears 4.5:1 at every interpolated point along the sweep —
 *    5.48:1 at the worst point — which the token test asserts by sampling the
 *    interpolation rather than just the stops.
 *
 * Both keep all three brand hues, so the deepened one still reads as OmniBridge.
 */
object OmniBridgeGradient {
    val decorativeStops = listOf(Brand.Cyan, Brand.Blue, Brand.Violet)

    /**
     * The same three hues, already corrected — this is exactly the
     * [AccentOnLight] triple rather than a fourth colour set to keep in step.
     */
    val ctaStops = listOf(AccentOnLight.Cyan, AccentOnLight.Blue, AccentOnLight.Violet)
    val progressStops = listOf(Brand.Cyan, Brand.Blue)

    /** Identity sweep for marks and illustration. Never put a label on it. */
    fun decorative(): Brush = Brush.linearGradient(decorativeStops)

    /** The gradient a primary button may wear. White text is AA on all of it. */
    fun cta(): Brush = Brush.linearGradient(ctaStops)

    /** Transfer progress. No text sits on it, so the vivid stops are fine. */
    fun progress(): Brush = Brush.linearGradient(progressStops)
}
