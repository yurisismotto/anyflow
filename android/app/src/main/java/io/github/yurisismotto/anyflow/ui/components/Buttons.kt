package io.github.yurisismotto.anyflow.ui.components

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.defaultMinSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Icon
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.ButtonDefaults
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import io.github.yurisismotto.anyflow.ui.theme.AnyFlowBorder
import io.github.yurisismotto.anyflow.ui.theme.AnyFlowGradient
import io.github.yurisismotto.anyflow.ui.theme.AnyFlowIconSize
import io.github.yurisismotto.anyflow.ui.theme.AnyFlowRadius
import io.github.yurisismotto.anyflow.ui.theme.AnyFlowSpacing
import io.github.yurisismotto.anyflow.ui.theme.AnyFlowTheme
import io.github.yurisismotto.anyflow.ui.theme.AnyFlowType
import io.github.yurisismotto.anyflow.ui.theme.MinTouchTarget

/**
 * The branded primary action.
 *
 * Wears the CTA gradient — the one whose teal start is deepened until white
 * clears AA at every point along the sweep. It is *not* the decorative brand
 * gradient: white on that one's teal end is 2.49:1, and this button always
 * has a label on it. See [AnyFlowGradient].
 *
 * Disabled state is a flat neutral rather than a faded gradient, because a
 * translucent gradient reads as "still tappable, just pretty".
 */
@Composable
fun AnyFlowPrimaryButton(
    text: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    icon: Int? = null,
) {
    val colors = AnyFlowTheme.colors
    val shape = RoundedCornerShape(AnyFlowRadius.full)
    val background: Brush = if (enabled) {
        AnyFlowGradient.cta()
    } else {
        Brush.linearGradient(listOf(colors.surfaceSunken, colors.surfaceSunken))
    }
    val content = if (enabled) Color.White else colors.disabled

    Box(
        modifier = modifier
            .fillMaxWidth()
            .heightIn(min = MinTouchTarget)
            .clip(shape)
            .background(background)
            .then(
                if (enabled) {
                    Modifier.clickable(role = Role.Button, onClick = onClick)
                } else {
                    Modifier
                },
            ),
        contentAlignment = Alignment.Center,
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            if (icon != null) {
                Icon(
                    painter = painterResource(icon),
                    contentDescription = null,
                    tint = content,
                    modifier = Modifier.size(AnyFlowIconSize.medium),
                )
                Spacer(Modifier.width(AnyFlowSpacing.xs))
            }
            Text(text, style = AnyFlowType.body.copy(fontWeight = androidx.compose.ui.text.font.FontWeight.SemiBold), color = content)
        }
    }
}

/** The quieter action beside a primary one. */
@Composable
fun AnyFlowSecondaryButton(
    text: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    icon: Int? = null,
) {
    val colors = AnyFlowTheme.colors
    OutlinedButton(
        onClick = onClick,
        enabled = enabled,
        modifier = modifier.heightIn(min = MinTouchTarget),
        shape = RoundedCornerShape(AnyFlowRadius.full),
        border = BorderStroke(AnyFlowBorder.hairline, colors.borderStrong),
        colors = ButtonDefaults.outlinedButtonColors(
            contentColor = colors.textPrimary,
            disabledContentColor = colors.disabled,
        ),
    ) {
        if (icon != null) {
            Icon(
                painter = painterResource(icon),
                contentDescription = null,
                modifier = Modifier.size(AnyFlowIconSize.medium),
            )
            Spacer(Modifier.width(AnyFlowSpacing.xs))
        }
        Text(text, style = AnyFlowType.body)
    }
}

/**
 * An action that takes something away — forgetting a device, revoking trust.
 *
 * Kept visually apart from everything else: red, outlined rather than filled,
 * and never placed adjacent to a confirming button. Filling it red would make
 * it the loudest thing on a screen whose main job is usually something else,
 * and sitting it next to "Send" is how a mis-tap becomes a lost pairing.
 */
@Composable
fun AnyFlowDestructiveButton(
    text: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    icon: Int? = null,
) {
    val colors = AnyFlowTheme.colors
    OutlinedButton(
        onClick = onClick,
        enabled = enabled,
        modifier = modifier
            .fillMaxWidth()
            .heightIn(min = MinTouchTarget),
        shape = RoundedCornerShape(AnyFlowRadius.full),
        border = BorderStroke(AnyFlowBorder.hairline, colors.accentRed.copy(alpha = 0.5f)),
        colors = ButtonDefaults.outlinedButtonColors(
            contentColor = colors.accentRed,
            disabledContentColor = colors.disabled,
        ),
    ) {
        if (icon != null) {
            Icon(
                painter = painterResource(icon),
                contentDescription = null,
                modifier = Modifier.size(AnyFlowIconSize.medium),
            )
            Spacer(Modifier.width(AnyFlowSpacing.xs))
        }
        Text(text, style = AnyFlowType.body)
    }
}

/** A low-emphasis inline action: "Cancel", "Dismiss", "View all". */
@Composable
fun AnyFlowTextButton(
    text: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
) {
    TextButton(
        onClick = onClick,
        enabled = enabled,
        modifier = modifier.heightIn(min = MinTouchTarget),
    ) {
        Text(
            text,
            style = AnyFlowType.body,
            color = if (enabled) AnyFlowTheme.colors.accentBlue else AnyFlowTheme.colors.disabled,
        )
    }
}

/**
 * One tile in the "Quick actions" row: icon over a short label.
 *
 * [enabled] is false when the action genuinely cannot run right now — nothing
 * is paired, nothing is connected. It stays visible and greys out rather than
 * disappearing, so the row does not reflow every time a device drops.
 */
@Composable
fun AnyFlowQuickAction(
    label: String,
    icon: Int,
    accent: Color,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
) {
    val colors = AnyFlowTheme.colors
    Column(
        modifier = modifier
            .defaultMinSize(minWidth = 76.dp)
            .clip(RoundedCornerShape(AnyFlowRadius.large))
            .background(colors.surface)
            .then(
                if (enabled) Modifier.clickable(role = Role.Button, onClick = onClick) else Modifier,
            )
            .padding(vertical = AnyFlowSpacing.sm, horizontal = AnyFlowSpacing.xs),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(AnyFlowSpacing.xs),
    ) {
        Icon(
            painter = painterResource(icon),
            contentDescription = null,
            tint = if (enabled) accent else colors.disabled,
            modifier = Modifier.size(AnyFlowIconSize.large),
        )
        Text(
            label,
            style = AnyFlowType.caption,
            color = if (enabled) colors.textSecondary else colors.disabled,
            textAlign = TextAlign.Center,
        )
    }
}
