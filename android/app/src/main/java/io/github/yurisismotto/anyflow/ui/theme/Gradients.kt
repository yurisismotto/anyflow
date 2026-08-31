package io.github.yurisismotto.anyflow.ui.theme

import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color

/**
 * The brand gradients.
 *
 * There are two, and picking the wrong one is an accessibility bug rather
 * than a taste question.
 *
 *  * [decorative] is the identity gradient, teal to blue to violet. It is for
 *    marks, ribbons and progress fills — surfaces with **no text on them**.
 *    White on its teal end is 2.49:1.
 *  * [cta] is the one that carries a label. Its teal start is deepened until
 *    white clears 4.5:1 at every interpolated point along the sweep, which
 *    the token test asserts by sampling the interpolation rather than just
 *    the stops.
 *
 * Both keep all three brand hues, so the deepened one still reads as AnyFlow.
 */
object AnyFlowGradient {
    val decorativeStops = listOf(Brand.Teal, Brand.Blue, Brand.Violet)
    val ctaStops = listOf(Color(0xFF0B7F72), Color(0xFF3B5BDB), Color(0xFF7C3AED))
    val progressStops = listOf(Brand.Teal, Brand.Blue)

    /** Identity sweep for marks and illustration. Never put a label on it. */
    fun decorative(): Brush = Brush.linearGradient(decorativeStops)

    /** The gradient a primary button may wear. White text is AA on all of it. */
    fun cta(): Brush = Brush.linearGradient(ctaStops)

    /** Transfer progress. No text sits on it, so the vivid stops are fine. */
    fun progress(): Brush = Brush.linearGradient(progressStops)
}
