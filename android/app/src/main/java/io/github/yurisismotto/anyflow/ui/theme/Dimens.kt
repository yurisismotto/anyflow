package io.github.yurisismotto.anyflow.ui.theme

import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp

/**
 * Spacing, on a 4dp base.
 *
 * Named rather than numeric so a screen says what it means. A literal `.dp`
 * in a layout is how a design system erodes: the first arbitrary `13.dp` is
 * never the last.
 */
object AnyFlowSpacing {
    val xxs: Dp = 4.dp
    val xs: Dp = 8.dp
    val sm: Dp = 12.dp
    val md: Dp = 16.dp
    val lg: Dp = 20.dp
    val xl: Dp = 24.dp
    val xxl: Dp = 32.dp
    val xxxl: Dp = 40.dp
    val huge: Dp = 48.dp
    val giant: Dp = 64.dp
}

/** Corner radii. Moderately rounded — the reference is soft, not bubbly. */
object AnyFlowRadius {
    val small: Dp = 8.dp
    val medium: Dp = 12.dp
    val large: Dp = 16.dp
    val xlarge: Dp = 20.dp

    /** Pills: buttons, chips, status badges. */
    val full: Dp = 999.dp
}

object AnyFlowBorder {
    val hairline: Dp = 1.dp
    val strong: Dp = 2.dp
    val focus: Dp = 2.dp
}

/**
 * Elevation, kept deliberately low.
 *
 * The reference builds depth from a 1dp border and a whisper of shadow, not
 * from Material's default lift. A card at 1dp with a hairline border reads as
 * a sheet of paper; the same card at 6dp reads as a floating dialog and
 * competes with the things that genuinely are floating.
 */
object AnyFlowElevation {
    val none: Dp = 0.dp
    val card: Dp = 1.dp
    val raised: Dp = 3.dp
    val dialog: Dp = 8.dp
}

object AnyFlowIconSize {
    val small: Dp = 16.dp
    val medium: Dp = 20.dp
    val large: Dp = 24.dp
    val xlarge: Dp = 32.dp
    val hero: Dp = 48.dp
}

/**
 * The smallest a tappable thing may be.
 *
 * Material asks for 48dp and so does this app. Switches and icon buttons in
 * the reference look smaller than that; they get the visual size the
 * reference shows and this much touch target around it.
 */
val MinTouchTarget: Dp = 48.dp
