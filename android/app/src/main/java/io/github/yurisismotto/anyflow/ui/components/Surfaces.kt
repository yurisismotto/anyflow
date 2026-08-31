package io.github.yurisismotto.anyflow.ui.components

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Icon
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import io.github.yurisismotto.anyflow.R
import io.github.yurisismotto.anyflow.ui.theme.AnyFlowBorder
import io.github.yurisismotto.anyflow.ui.theme.AnyFlowElevation
import io.github.yurisismotto.anyflow.ui.theme.AnyFlowIconSize
import io.github.yurisismotto.anyflow.ui.theme.AnyFlowRadius
import io.github.yurisismotto.anyflow.ui.theme.AnyFlowSpacing
import io.github.yurisismotto.anyflow.ui.theme.AnyFlowTheme
import io.github.yurisismotto.anyflow.ui.theme.AnyFlowType

/**
 * The base card.
 *
 * One surface, one hairline border, one small radius — every card in AnyFlow
 * is this, so they cannot drift apart. Depth comes from the border rather
 * than a shadow: the reference is flat and calm, and a 6dp Material lift on
 * every card would make the whole screen hover.
 */
@Composable
fun AnyFlowCard(
    modifier: Modifier = Modifier,
    contentPadding: androidx.compose.ui.unit.Dp = AnyFlowSpacing.md,
    content: @Composable ColumnScope.() -> Unit,
) {
    val colors = AnyFlowTheme.colors
    Surface(
        modifier = modifier.fillMaxWidth(),
        shape = RoundedCornerShape(AnyFlowRadius.large),
        color = colors.surface,
        border = BorderStroke(AnyFlowBorder.hairline, colors.border),
        tonalElevation = AnyFlowElevation.none,
        shadowElevation = AnyFlowElevation.card,
    ) {
        Column(
            Modifier.padding(contentPadding),
            verticalArrangement = Arrangement.spacedBy(AnyFlowSpacing.xs),
            content = content,
        )
    }
}

/**
 * The small grey label above a group ("Devices", "Quick actions", "Recent").
 *
 * A heading for assistive technology, not merely small text: the reference
 * uses these to divide the screen, so a screen reader should be able to jump
 * between them.
 */
@Composable
fun AnyFlowSectionLabel(text: String, modifier: Modifier = Modifier) {
    Text(
        text = text,
        style = AnyFlowType.label,
        color = AnyFlowTheme.colors.textSecondary,
        modifier = modifier.semantics { heading() },
    )
}

/**
 * A tinted notice carrying a security or privacy statement.
 *
 * Deliberately quiet. The reference draws these in a soft blue rather than a
 * warning yellow because most of them are reassurance ("this stayed on your
 * network"), and a UI that shouts every security fact teaches people to stop
 * reading them. [tone] raises the volume only when the message is a caution
 * the person has to act on.
 */
enum class NoticeTone { Info, Caution }

@Composable
fun AnyFlowSecurityNotice(
    title: String,
    body: String? = null,
    tone: NoticeTone = NoticeTone.Info,
    icon: Int = R.drawable.ic_shield_check,
    modifier: Modifier = Modifier,
) {
    val colors = AnyFlowTheme.colors
    val accent = when (tone) {
        NoticeTone.Info -> colors.accentBlue
        NoticeTone.Caution -> colors.accentAmber
    }
    val tint = accent.copy(alpha = if (colors.isDark) 0.16f else 0.08f)
    Row(
        modifier = modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(AnyFlowRadius.medium))
            .background(tint)
            .padding(AnyFlowSpacing.sm),
        verticalAlignment = Alignment.Top,
    ) {
        Icon(
            painter = painterResource(icon),
            // The text beside it says the same thing; announcing the glyph
            // separately would just make the reader say it twice.
            contentDescription = null,
            tint = accent,
            modifier = Modifier.size(AnyFlowIconSize.medium),
        )
        Spacer(Modifier.width(AnyFlowSpacing.sm))
        Column(verticalArrangement = Arrangement.spacedBy(AnyFlowSpacing.xxs)) {
            Text(title, style = AnyFlowType.label, color = colors.textPrimary)
            if (body != null) {
                Text(body, style = AnyFlowType.caption, color = colors.textSecondary)
            }
        }
    }
}

/**
 * A fingerprint, device id or hash.
 *
 * Monospace and selectable, never truncated in the middle. This is the string
 * a person compares against another screen before trusting a device, so
 * shortening it for balance would be trading away the only thing that makes
 * pairing safe. The reference shows it in mono for the same reason.
 */
@Composable
fun AnyFlowFingerprint(
    value: String,
    modifier: Modifier = Modifier,
    color: Color? = null,
) {
    Text(
        text = value,
        style = AnyFlowType.mono,
        color = color ?: AnyFlowTheme.colors.textPrimary,
        modifier = modifier.semantics {
            // Read out in groups rather than as one unpronounceable run.
            contentDescription = "Fingerprint ${value.chunked(4).joinToString(", ")}"
        },
    )
}

/** Hides purely decorative artwork from assistive technology. */
fun Modifier.decorative(): Modifier = this.clearAndSetSemantics { }
