package io.github.yurisismotto.omnibridge.ui.components

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
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.wrapContentWidth
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
import io.github.yurisismotto.omnibridge.R
import io.github.yurisismotto.omnibridge.ui.theme.OmniBridgeBorder
import io.github.yurisismotto.omnibridge.ui.theme.OmniBridgeElevation
import io.github.yurisismotto.omnibridge.ui.theme.OmniBridgeIconSize
import io.github.yurisismotto.omnibridge.ui.theme.OmniBridgeLayout
import io.github.yurisismotto.omnibridge.ui.theme.OmniBridgeRadius
import io.github.yurisismotto.omnibridge.ui.theme.OmniBridgeSpacing
import io.github.yurisismotto.omnibridge.ui.theme.OmniBridgeTheme
import io.github.yurisismotto.omnibridge.ui.theme.OmniBridgeType

/**
 * The base card.
 *
 * One surface, one hairline border, one small radius — every card in OmniBridge
 * is this, so they cannot drift apart. Depth comes from the border rather
 * than a shadow: the reference is flat and calm, and a 6dp Material lift on
 * every card would make the whole screen hover.
 */
@Composable
fun OmniBridgeCard(
    modifier: Modifier = Modifier,
    contentPadding: androidx.compose.ui.unit.Dp = OmniBridgeSpacing.md,
    content: @Composable ColumnScope.() -> Unit,
) {
    val colors = OmniBridgeTheme.colors
    Surface(
        modifier = modifier.fillMaxWidth(),
        shape = RoundedCornerShape(OmniBridgeRadius.large),
        color = colors.surface,
        border = BorderStroke(OmniBridgeBorder.hairline, colors.border),
        tonalElevation = OmniBridgeElevation.none,
        shadowElevation = OmniBridgeElevation.card,
    ) {
        Column(
            Modifier.padding(contentPadding),
            verticalArrangement = Arrangement.spacedBy(OmniBridgeSpacing.xs),
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
fun OmniBridgeSectionLabel(text: String, modifier: Modifier = Modifier) {
    Text(
        text = text,
        style = OmniBridgeType.label,
        color = OmniBridgeTheme.colors.textSecondary,
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
fun OmniBridgeSecurityNotice(
    title: String,
    body: String? = null,
    tone: NoticeTone = NoticeTone.Info,
    icon: Int = R.drawable.ic_shield_check,
    modifier: Modifier = Modifier,
) {
    val colors = OmniBridgeTheme.colors
    val accent = when (tone) {
        NoticeTone.Info -> colors.accentBlue
        NoticeTone.Caution -> colors.accentAmber
    }
    val tint = accent.copy(alpha = if (colors.isDark) 0.16f else 0.08f)
    Row(
        modifier = modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(OmniBridgeRadius.medium))
            .background(tint)
            .padding(OmniBridgeSpacing.sm),
        verticalAlignment = Alignment.Top,
    ) {
        Icon(
            painter = painterResource(icon),
            // The text beside it says the same thing; announcing the glyph
            // separately would just make the reader say it twice.
            contentDescription = null,
            tint = accent,
            modifier = Modifier.size(OmniBridgeIconSize.medium),
        )
        Spacer(Modifier.width(OmniBridgeSpacing.sm))
        Column(verticalArrangement = Arrangement.spacedBy(OmniBridgeSpacing.xxs)) {
            Text(title, style = OmniBridgeType.label, color = colors.textPrimary)
            if (body != null) {
                Text(body, style = OmniBridgeType.caption, color = colors.textSecondary)
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
fun OmniBridgeFingerprint(
    value: String,
    modifier: Modifier = Modifier,
    color: Color? = null,
) {
    Text(
        text = value,
        style = OmniBridgeType.mono,
        color = color ?: OmniBridgeTheme.colors.textPrimary,
        modifier = modifier.semantics {
            // Read out in groups rather than as one unpronounceable run.
            contentDescription = "Fingerprint ${value.chunked(4).joinToString(", ")}"
        },
    )
}

/** Hides purely decorative artwork from assistive technology. */
fun Modifier.decorative(): Modifier = this.clearAndSetSemantics { }

/**
 * The content column: fills a phone, stops growing on a tablet.
 *
 * On the SM-X620 in landscape the window is roughly 1340dp wide. Every screen
 * here is a single scrolling column of cards, and a card stretched to that
 * width is not a tablet layout — it is a phone layout that has been pulled.
 * A permission row becomes a title at the far left and a switch at the far
 * right with a metre of nothing between them, and the eye has to travel the
 * whole width to connect two things that belong together.
 *
 * So past [OmniBridgeLayout.contentMax] the column stops growing and centres,
 * and the extra width becomes margin. Below it nothing changes at all, which
 * is why this is safe to apply to an existing phone layout.
 *
 * A modifier rather than a wrapper composable on purpose: it works the same
 * on a `Column` and on a `LazyColumn`, so no screen has to be restructured
 * and a lazy list stays lazy. It is a layout constraint and nothing else —
 * no branch, no alternate composition, nothing that could make the tablet
 * show different *content* from the phone.
 */
fun Modifier.omniBridgeContentColumn(): Modifier =
    this
        .fillMaxWidth()
        .wrapContentWidth(Alignment.CenterHorizontally)
        .widthIn(max = OmniBridgeLayout.contentMax)
