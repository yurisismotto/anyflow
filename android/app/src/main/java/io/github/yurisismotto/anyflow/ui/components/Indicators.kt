package io.github.yurisismotto.anyflow.ui.components

import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.progressBarRangeInfo
import androidx.compose.ui.semantics.ProgressBarRangeInfo
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import io.github.yurisismotto.anyflow.R
import io.github.yurisismotto.anyflow.ui.theme.AnyFlowGradient
import io.github.yurisismotto.anyflow.ui.theme.AnyFlowIconSize
import io.github.yurisismotto.anyflow.ui.theme.AnyFlowMotion
import io.github.yurisismotto.anyflow.ui.theme.AnyFlowRadius
import io.github.yurisismotto.anyflow.ui.theme.AnyFlowSpacing
import io.github.yurisismotto.anyflow.ui.theme.AnyFlowStatus
import io.github.yurisismotto.anyflow.ui.theme.AnyFlowTheme
import io.github.yurisismotto.anyflow.ui.theme.AnyFlowType
import io.github.yurisismotto.anyflow.ui.theme.LocalReducedMotion
import io.github.yurisismotto.anyflow.ui.theme.motionDuration

/**
 * A device or transfer state: dot, icon, word.
 *
 * All three, always. See [AnyFlowStatus] for why the word is not optional.
 * The dot and the icon are marked decorative so a screen reader announces the
 * state once, not three times.
 */
@Composable
fun AnyFlowStatusBadge(
    status: AnyFlowStatus,
    modifier: Modifier = Modifier,
    label: String = status.label,
    showIcon: Boolean = true,
) {
    val color = status.color()
    Row(
        modifier = modifier.semantics(mergeDescendants = true) { contentDescription = label },
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(AnyFlowSpacing.xxs),
    ) {
        Box(
            Modifier
                .size(8.dp)
                .clip(RoundedCornerShape(AnyFlowRadius.full))
                .background(status.dot()),
        )
        if (showIcon) {
            Icon(
                painter = painterResource(status.icon),
                contentDescription = null,
                tint = color,
                modifier = Modifier.size(AnyFlowIconSize.small),
            )
        }
        Text(label, style = AnyFlowType.label, color = color)
    }
}

/**
 * The Flowing Ribbon, at whatever size the caller needs.
 *
 * The product mark: app bar, transfer motifs, empty states. The institutional
 * mark is the Flowing A ([AnyFlowBrandMark]).
 */
@Composable
fun AnyFlowGradientMark(
    modifier: Modifier = Modifier,
    size: Dp = AnyFlowIconSize.large,
    contentDescription: String? = null,
) {
    Icon(
        painter = painterResource(R.drawable.logo_flowing_ribbon),
        contentDescription = contentDescription,
        tint = Color.Unspecified,
        modifier = modifier.size(size),
    )
}

/** The Flowing A. Institutional: About, onboarding, the empty first run. */
@Composable
fun AnyFlowBrandMark(
    modifier: Modifier = Modifier,
    size: Dp = AnyFlowIconSize.hero,
    contentDescription: String? = null,
) {
    Icon(
        painter = painterResource(R.drawable.logo_flowing_a),
        contentDescription = contentDescription,
        tint = Color.Unspecified,
        modifier = modifier.size(size),
    )
}

/**
 * Transfer progress, in the brand gradient.
 *
 * Rolls its own track and fill rather than tinting a Material bar, because a
 * `LinearProgressIndicator` takes a solid colour and the gradient is the
 * point. The semantics are supplied by hand for the same reason — this must
 * still announce itself as a progress bar.
 */
@Composable
fun AnyFlowProgressBar(
    fraction: Float?,
    modifier: Modifier = Modifier,
    height: Dp = 6.dp,
) {
    val colors = AnyFlowTheme.colors
    val reduced = LocalReducedMotion.current

    // Indeterminate work still has to look alive; under reduced motion it
    // simply sits at a fixed width instead of sweeping.
    val infinite = rememberInfiniteTransition(label = "indeterminate")
    val sweep by infinite.animateFloat(
        initialValue = 0f,
        targetValue = 1f,
        animationSpec = infiniteRepeatable(
            animation = tween(AnyFlowMotion.RIBBON_PULSE_MS, easing = AnyFlowMotion.Flow),
            repeatMode = RepeatMode.Restart,
        ),
        label = "sweep",
    )
    val target = fraction?.coerceIn(0f, 1f)
    val animated by animateFloatAsState(
        targetValue = target ?: 0f,
        animationSpec = tween(motionDuration(AnyFlowMotion.NORMAL_MS), easing = AnyFlowMotion.Flow),
        label = "progress",
    )

    Box(
        modifier
            .fillMaxWidth()
            .height(height)
            .clip(RoundedCornerShape(AnyFlowRadius.full))
            .background(colors.surfaceSunken)
            .semantics {
                progressBarRangeInfo = if (target != null) {
                    ProgressBarRangeInfo(animated, 0f..1f)
                } else {
                    ProgressBarRangeInfo.Indeterminate
                }
            },
    ) {
        if (target != null) {
            Box(
                Modifier
                    .fillMaxHeight()
                    .fillMaxWidth(animated)
                    .clip(RoundedCornerShape(AnyFlowRadius.full))
                    .background(AnyFlowGradient.progress()),
            )
        } else {
            // Indeterminate: a short segment travelling the track. Under
            // reduced motion it parks in the middle rather than sweeping,
            // which still reads as "working" without any movement at all.
            val bias = if (reduced) 0f else (sweep * 2f - 1f)
            Box(
                Modifier
                    .align(androidx.compose.ui.BiasAlignment(bias, 0f))
                    .fillMaxHeight()
                    .fillMaxWidth(SEGMENT)
                    .clip(RoundedCornerShape(AnyFlowRadius.full))
                    .background(AnyFlowGradient.progress()),
            )
        }
    }
}

/** Width of the travelling segment shown while progress is unknown. */
private const val SEGMENT = 0.3f

/**
 * Battery, as a number with a bar beside it.
 *
 * [stale] is surfaced rather than hidden. A percentage from a session that
 * has gone quiet is history, and showing it as though it were current is the
 * exact bug the daemon's `DeviceState` type was introduced to stop.
 */
@Composable
fun AnyFlowBatteryPill(
    percentage: Int,
    modifier: Modifier = Modifier,
    charging: Boolean = false,
    stale: Boolean = false,
) {
    val colors = AnyFlowTheme.colors
    val tint = when {
        stale -> colors.textMuted
        percentage <= 15 -> colors.accentRed
        else -> colors.accentTeal
    }
    val suffix = when {
        stale -> ", last known"
        charging -> ", charging"
        else -> ""
    }
    Row(
        modifier = modifier.semantics(mergeDescendants = true) {
            contentDescription = "Battery $percentage percent$suffix"
        },
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(AnyFlowSpacing.xxs),
    ) {
        Icon(
            painter = painterResource(R.drawable.ic_battery),
            contentDescription = null,
            tint = tint,
            modifier = Modifier.size(AnyFlowIconSize.small),
        )
        Text("$percentage%", style = AnyFlowType.label, color = tint)
        if (charging && !stale) {
            Text("charging", style = AnyFlowType.caption, color = colors.textMuted)
        }
        if (stale) {
            Text("last known", style = AnyFlowType.caption, color = colors.textMuted)
        }
    }
}

/**
 * The ribbon flourish behind a connected device card.
 *
 * Purely decorative — the reference sweeps a soft gradient wave across the
 * card, and this is that wave. It is hidden from assistive technology and it
 * carries no state: a card must not need the artwork to say it is connected.
 */
@Composable
fun AnyFlowRibbonFlourish(modifier: Modifier = Modifier) {
    val alpha = if (AnyFlowTheme.colors.isDark) 0.22f else 0.14f
    Canvas(modifier.clearAndSetSemantics { }) {
        val w = size.width
        val h = size.height
        val path = androidx.compose.ui.graphics.Path().apply {
            moveTo(0f, h * 0.72f)
            cubicTo(w * 0.30f, h * 0.72f, w * 0.24f, h * 0.30f, w * 0.62f, h * 0.36f)
            cubicTo(w * 0.86f, h * 0.40f, w * 0.86f, h * 0.18f, w, h * 0.24f)
        }
        drawPath(
            path = path,
            brush = AnyFlowGradient.decorative(),
            style = androidx.compose.ui.graphics.drawscope.Stroke(
                width = h * 0.16f,
                cap = androidx.compose.ui.graphics.StrokeCap.Round,
            ),
            alpha = alpha,
        )
    }
}
