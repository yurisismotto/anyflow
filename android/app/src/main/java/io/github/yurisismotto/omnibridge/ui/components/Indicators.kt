package io.github.yurisismotto.omnibridge.ui.components

import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
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
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.progressBarRangeInfo
import androidx.compose.ui.semantics.ProgressBarRangeInfo
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import io.github.yurisismotto.omnibridge.R
import io.github.yurisismotto.omnibridge.ui.theme.OmniBridgeGradient
import io.github.yurisismotto.omnibridge.ui.theme.OmniBridgeIconSize
import io.github.yurisismotto.omnibridge.ui.theme.OmniBridgeMotion
import io.github.yurisismotto.omnibridge.ui.theme.OmniBridgeRadius
import io.github.yurisismotto.omnibridge.ui.theme.OmniBridgeSpacing
import io.github.yurisismotto.omnibridge.ui.theme.OmniBridgeStatus
import io.github.yurisismotto.omnibridge.ui.theme.OmniBridgeTheme
import io.github.yurisismotto.omnibridge.ui.theme.OmniBridgeType
import io.github.yurisismotto.omnibridge.ui.theme.LocalReducedMotion
import io.github.yurisismotto.omnibridge.ui.theme.motionDuration

/**
 * A device or transfer state: dot, icon, word.
 *
 * All three, always. See [OmniBridgeStatus] for why the word is not optional.
 * The dot and the icon are marked decorative so a screen reader announces the
 * state once, not three times.
 */
@Composable
fun OmniBridgeStatusBadge(
    status: OmniBridgeStatus,
    modifier: Modifier = Modifier,
    label: String = status.label,
    showIcon: Boolean = true,
) {
    val color = status.color()
    Row(
        modifier = modifier.semantics(mergeDescendants = true) { contentDescription = label },
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(OmniBridgeSpacing.xxs),
    ) {
        Box(
            Modifier
                .size(8.dp)
                .clip(RoundedCornerShape(OmniBridgeRadius.full))
                .background(status.dot()),
        )
        if (showIcon) {
            Icon(
                painter = painterResource(status.icon),
                contentDescription = null,
                tint = color,
                modifier = Modifier.size(OmniBridgeIconSize.small),
            )
        }
        Text(label, style = OmniBridgeType.label, color = color)
    }
}

/**
 * The OmniBridge mark, at whatever size the caller needs.
 *
 * Product placements: app bar, transfer motifs, empty states. [OmniBridgeBrandMark]
 * is the same artwork at the institutional size — there is one mark now, not two.
 */
@Composable
fun OmniBridgeGradientMark(
    modifier: Modifier = Modifier,
    size: Dp = OmniBridgeIconSize.large,
    contentDescription: String? = null,
) {
    Icon(
        painter = painterResource(R.drawable.logo_omnibridge_mark),
        contentDescription = contentDescription,
        tint = Color.Unspecified,
        modifier = modifier.size(size),
    )
}

/**
 * The OmniBridge mark, institutional size: About, onboarding, the empty first run.
 *
 * The same artwork the launcher icon wears and the same one the desktop wears.
 * `logo_omnibridge_mark.xml` carries the geometry of
 * `docs/design/assets/omnibridge-mark.svg` verbatim, and `BrandingResourcesTest`
 * asserts the equality, so the three platforms cannot drift apart.
 */
@Composable
fun OmniBridgeBrandMark(
    modifier: Modifier = Modifier,
    size: Dp = OmniBridgeIconSize.hero,
    contentDescription: String? = null,
) {
    Icon(
        painter = painterResource(R.drawable.logo_omnibridge_mark),
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
fun OmniBridgeProgressBar(
    fraction: Float?,
    modifier: Modifier = Modifier,
    height: Dp = 6.dp,
) {
    val colors = OmniBridgeTheme.colors
    val reduced = LocalReducedMotion.current

    // Indeterminate work still has to look alive; under reduced motion it
    // simply sits at a fixed width instead of sweeping.
    val infinite = rememberInfiniteTransition(label = "indeterminate")
    val sweep by infinite.animateFloat(
        initialValue = 0f,
        targetValue = 1f,
        animationSpec = infiniteRepeatable(
            animation = tween(OmniBridgeMotion.RIBBON_PULSE_MS, easing = OmniBridgeMotion.Flow),
            repeatMode = RepeatMode.Restart,
        ),
        label = "sweep",
    )
    val target = fraction?.coerceIn(0f, 1f)
    val animated by animateFloatAsState(
        targetValue = target ?: 0f,
        animationSpec = tween(motionDuration(OmniBridgeMotion.NORMAL_MS), easing = OmniBridgeMotion.Flow),
        label = "progress",
    )

    Box(
        modifier
            .fillMaxWidth()
            .height(height)
            .clip(RoundedCornerShape(OmniBridgeRadius.full))
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
                    .clip(RoundedCornerShape(OmniBridgeRadius.full))
                    .background(OmniBridgeGradient.progress()),
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
                    .clip(RoundedCornerShape(OmniBridgeRadius.full))
                    .background(OmniBridgeGradient.progress()),
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
fun OmniBridgeBatteryPill(
    percentage: Int,
    modifier: Modifier = Modifier,
    charging: Boolean = false,
    stale: Boolean = false,
) {
    val colors = OmniBridgeTheme.colors
    val tint = when {
        stale -> colors.textMuted
        percentage <= 15 -> colors.accentRed
        else -> colors.accentCyan
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
        horizontalArrangement = Arrangement.spacedBy(OmniBridgeSpacing.xxs),
    ) {
        Icon(
            painter = painterResource(R.drawable.ic_battery),
            contentDescription = null,
            tint = tint,
            modifier = Modifier.size(OmniBridgeIconSize.small),
        )
        Text("$percentage%", style = OmniBridgeType.label, color = tint)
        if (charging && !stale) {
            Text("charging", style = OmniBridgeType.caption, color = colors.textMuted)
        }
        if (stale) {
            Text("last known", style = OmniBridgeType.caption, color = colors.textMuted)
        }
    }
}
