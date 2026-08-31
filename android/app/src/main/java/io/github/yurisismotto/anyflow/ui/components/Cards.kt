package io.github.yurisismotto.anyflow.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Icon
import androidx.compose.material3.Switch
import androidx.compose.material3.SwitchDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import io.github.yurisismotto.anyflow.R
import io.github.yurisismotto.anyflow.ui.theme.AnyFlowIconSize
import io.github.yurisismotto.anyflow.ui.theme.AnyFlowRadius
import io.github.yurisismotto.anyflow.ui.theme.AnyFlowSpacing
import io.github.yurisismotto.anyflow.ui.theme.AnyFlowStatus
import io.github.yurisismotto.anyflow.ui.theme.AnyFlowTheme
import io.github.yurisismotto.anyflow.ui.theme.AnyFlowType
import io.github.yurisismotto.anyflow.ui.theme.MinTouchTarget

/** An icon on a soft tint of its own accent. Used for files, capabilities, actions. */
@Composable
fun AnyFlowIconTile(
    icon: Int,
    accent: Color,
    modifier: Modifier = Modifier,
    size: Dp = 40.dp,
) {
    val tint = accent.copy(alpha = if (AnyFlowTheme.colors.isDark) 0.22f else 0.10f)
    Box(
        modifier
            .size(size)
            .clip(RoundedCornerShape(AnyFlowRadius.medium))
            .background(tint),
        contentAlignment = Alignment.Center,
    ) {
        Icon(
            painter = painterResource(icon),
            contentDescription = null,
            tint = accent,
            modifier = Modifier.size(AnyFlowIconSize.large),
        )
    }
}

/**
 * A paired computer, as the home screen shows it.
 *
 * The ribbon flourish behind it is decoration and nothing more: the state is
 * carried by the badge, which has a word in it. A card whose only signal of
 * "connected" was a coloured wave would be unreadable to anyone who cannot
 * separate the hues, and invisible to a screen reader entirely.
 */
@Composable
fun AnyFlowDeviceCard(
    name: String,
    platform: String,
    status: AnyFlowStatus,
    modifier: Modifier = Modifier,
    deviceIcon: Int = R.drawable.ic_device_desktop,
    batteryPercent: Int? = null,
    batteryCharging: Boolean = false,
    batteryStale: Boolean = false,
    onClick: (() -> Unit)? = null,
    footer: (@Composable () -> Unit)? = null,
) {
    val colors = AnyFlowTheme.colors
    AnyFlowCard(
        modifier = modifier.then(
            if (onClick != null) {
                Modifier
                    .clip(RoundedCornerShape(AnyFlowRadius.large))
                    .clickable(role = Role.Button, onClick = onClick)
            } else {
                Modifier
            },
        ),
        contentPadding = 0.dp,
    ) {
        Box {
            // Only a live device gets the flourish; a disconnected card stays
            // plain, so the artwork tracks reality instead of decorating it.
            if (status.isPositive) {
                // A corner accent, not a band: constrained to the upper right
                // so it never runs through the name or the capability chips.
                AnyFlowRibbonFlourish(
                    Modifier
                        .align(Alignment.TopEnd)
                        .fillMaxWidth(0.55f)
                        .height(72.dp),
                )
            }
            Column(
                Modifier.padding(AnyFlowSpacing.md),
                verticalArrangement = Arrangement.spacedBy(AnyFlowSpacing.xs),
            ) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    AnyFlowStatusBadge(status)
                    Spacer(Modifier.weight(1f))
                    Icon(
                        painter = painterResource(deviceIcon),
                        contentDescription = null,
                        tint = colors.textMuted,
                        modifier = Modifier.size(AnyFlowIconSize.large),
                    )
                }
                Text(
                    name,
                    style = AnyFlowType.title,
                    color = colors.textPrimary,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                Text(platform, style = AnyFlowType.body, color = colors.textSecondary)
                if (batteryPercent != null) {
                    AnyFlowBatteryPill(
                        percentage = batteryPercent,
                        charging = batteryCharging,
                        stale = batteryStale,
                    )
                }
                if (footer != null) {
                    Spacer(Modifier.height(AnyFlowSpacing.xxs))
                    footer()
                }
            }
        }
    }
}

/**
 * One permission or automation switch: icon, what it is, what it does, toggle.
 *
 * [description] is not filler. Every row here changes what another machine is
 * allowed to do with this one, and "Clipboard" alone does not tell anybody
 * whether turning it on means sending, receiving or both.
 */
@Composable
fun AnyFlowCapabilityRow(
    title: String,
    description: String,
    icon: Int,
    accent: Color,
    checked: Boolean,
    onCheckedChange: (Boolean) -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
) {
    val colors = AnyFlowTheme.colors
    Row(
        modifier = modifier
            .fillMaxWidth()
            .heightIn(min = MinTouchTarget)
            .padding(vertical = AnyFlowSpacing.xs),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(
            painter = painterResource(icon),
            contentDescription = null,
            tint = if (enabled) accent else colors.disabled,
            modifier = Modifier.size(AnyFlowIconSize.large),
        )
        Spacer(Modifier.width(AnyFlowSpacing.sm))
        Column(Modifier.weight(1f)) {
            Text(
                title,
                style = AnyFlowType.body,
                color = if (enabled) colors.textPrimary else colors.disabled,
            )
            Text(
                description,
                style = AnyFlowType.caption,
                color = if (enabled) colors.textSecondary else colors.disabled,
            )
        }
        Spacer(Modifier.width(AnyFlowSpacing.xs))
        Switch(
            checked = checked,
            onCheckedChange = onCheckedChange,
            enabled = enabled,
            // The row already reads its own title and description; letting the
            // switch announce a second, contextless label would double it up.
            modifier = Modifier.semantics { contentDescription = title },
            colors = SwitchDefaults.colors(
                checkedTrackColor = colors.accentTeal,
                checkedThumbColor = Color.White,
                checkedBorderColor = colors.accentTeal,
            ),
        )
    }
}

/**
 * A file on its way somewhere, or one that has arrived.
 *
 * Direction is stated in words ("From Fedora" / "To Fedora") rather than
 * implied by an arrow's rotation, and the state line never disappears — a
 * transfer that failed keeps saying so until it is cleared.
 */
@Composable
fun AnyFlowTransferCard(
    filename: String,
    subtitle: String,
    status: AnyFlowStatus,
    modifier: Modifier = Modifier,
    fraction: Float? = null,
    detail: String? = null,
    percentLabel: String? = null,
    icon: Int = R.drawable.ic_file,
    accent: Color? = null,
    onCancel: (() -> Unit)? = null,
) {
    val colors = AnyFlowTheme.colors
    val tileAccent = accent ?: colors.accentBlue
    AnyFlowCard(modifier) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            AnyFlowIconTile(icon = icon, accent = tileAccent)
            Spacer(Modifier.width(AnyFlowSpacing.sm))
            Column(Modifier.weight(1f)) {
                Text(
                    filename,
                    style = AnyFlowType.subtitle,
                    color = colors.textPrimary,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                Text(subtitle, style = AnyFlowType.caption, color = colors.textSecondary)
            }
            if (percentLabel != null) {
                Spacer(Modifier.width(AnyFlowSpacing.xs))
                Text(percentLabel, style = AnyFlowType.label, color = colors.accentBlue)
            } else if (status == AnyFlowStatus.Success) {
                Icon(
                    painter = painterResource(R.drawable.ic_check),
                    contentDescription = status.label,
                    tint = colors.accentTeal,
                    modifier = Modifier.size(AnyFlowIconSize.large),
                )
            }
        }
        if (fraction != null || status == AnyFlowStatus.Transferring) {
            Spacer(Modifier.height(AnyFlowSpacing.xxs))
            AnyFlowProgressBar(fraction)
        }
        Row(verticalAlignment = Alignment.CenterVertically) {
            if (detail != null) {
                Text(detail, style = AnyFlowType.caption, color = colors.textSecondary)
            } else if (status != AnyFlowStatus.Success) {
                AnyFlowStatusBadge(status, showIcon = false)
            }
            Spacer(Modifier.weight(1f))
            if (onCancel != null) {
                AnyFlowTextButton("Cancel", onCancel)
            }
        }
    }
}

/**
 * Nothing here yet.
 *
 * Uses the connection ribbon rather than a stock illustration: the mark
 * already means "two devices, one flow", which is exactly what the person is
 * being invited to create.
 */
@Composable
fun AnyFlowEmptyState(
    title: String,
    subtitle: String,
    modifier: Modifier = Modifier,
    actionLabel: String? = null,
    onAction: (() -> Unit)? = null,
) {
    val colors = AnyFlowTheme.colors
    Column(
        modifier = modifier
            .fillMaxWidth()
            .padding(vertical = AnyFlowSpacing.xxl, horizontal = AnyFlowSpacing.lg),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(AnyFlowSpacing.sm),
    ) {
        Icon(
            painter = painterResource(R.drawable.ribbon_connection),
            contentDescription = null,
            tint = Color.Unspecified,
            modifier = Modifier
                .width(160.dp)
                .height(58.dp),
        )
        Text(
            title,
            style = AnyFlowType.heading,
            color = colors.textPrimary,
            textAlign = TextAlign.Center,
        )
        Text(
            subtitle,
            style = AnyFlowType.body,
            color = colors.textSecondary,
            textAlign = TextAlign.Center,
        )
        if (actionLabel != null && onAction != null) {
            Spacer(Modifier.height(AnyFlowSpacing.xs))
            AnyFlowPrimaryButton(
                text = actionLabel,
                onClick = onAction,
                icon = R.drawable.ic_qr,
                modifier = Modifier.width(240.dp),
            )
        }
    }
}
