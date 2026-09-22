package io.github.yurisismotto.omnibridge.ui.components

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
import androidx.compose.foundation.shape.CircleShape
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
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import io.github.yurisismotto.omnibridge.R
import io.github.yurisismotto.omnibridge.ui.theme.OmniBridgeIconSize
import io.github.yurisismotto.omnibridge.ui.theme.OmniBridgeRadius
import io.github.yurisismotto.omnibridge.ui.theme.OmniBridgeSpacing
import io.github.yurisismotto.omnibridge.ui.theme.OmniBridgeStatus
import io.github.yurisismotto.omnibridge.ui.theme.OmniBridgeTheme
import io.github.yurisismotto.omnibridge.ui.theme.OmniBridgeType
import io.github.yurisismotto.omnibridge.ui.theme.MinTouchTarget

/** An icon on a soft tint of its own accent. Used for files, capabilities, actions. */
@Composable
fun OmniBridgeIconTile(
    icon: Int,
    accent: Color,
    modifier: Modifier = Modifier,
    size: Dp = 40.dp,
) {
    val tint = accent.copy(alpha = if (OmniBridgeTheme.colors.isDark) 0.22f else 0.10f)
    Box(
        modifier
            .size(size)
            .clip(RoundedCornerShape(OmniBridgeRadius.medium))
            .background(tint),
        contentAlignment = Alignment.Center,
    ) {
        Icon(
            painter = painterResource(icon),
            contentDescription = null,
            tint = accent,
            modifier = Modifier.size(OmniBridgeIconSize.large),
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
fun OmniBridgeDeviceCard(
    name: String,
    /**
     * The line under the name: the peer's platform while a session is up,
     * and its short fingerprint otherwise.
     *
     * Was `platform`, and was a literal `"Desktop · Linux"` at every call
     * site — including for a phone. See [UiMapping.peerIdentityLine].
     */
    subtitle: String,
    status: OmniBridgeStatus,
    modifier: Modifier = Modifier,
    deviceIcon: Int = R.drawable.ic_device_desktop,
    batteryPercent: Int? = null,
    batteryCharging: Boolean = false,
    batteryStale: Boolean = false,
    onClick: (() -> Unit)? = null,
    /**
     * A state announced alongside the card's own text, never instead of it.
     *
     * `stateDescription` rather than `contentDescription` on purpose: the
     * second would *replace* the name, the subtitle and the badge with one
     * string, and a card whose children had stopped speaking is the N3
     * regression this project already paid for once. This is additive — the
     * row still reads its name and its badge, and gains a state.
     */
    stateDescription: String? = null,
    footer: (@Composable () -> Unit)? = null,
) {
    val colors = OmniBridgeTheme.colors
    OmniBridgeCard(
        modifier = modifier
            .then(
                if (stateDescription != null) {
                    Modifier.semantics { this.stateDescription = stateDescription }
                } else {
                    Modifier
                },
            )
            .then(
            if (onClick != null) {
                Modifier
                    .clip(RoundedCornerShape(OmniBridgeRadius.large))
                    .clickable(role = Role.Button, onClick = onClick)
            } else {
                Modifier
            },
        ),
        contentPadding = 0.dp,
    ) {
        // No decorative wave behind the card any more.
        //
        // It was a thin cyan-to-violet stroke in the upper right, drawn only
        // for a connected device — and it said nothing the badge, the name,
        // the subtitle and the battery pill were not already saying, while
        // running a gradient behind the very corner the device glyph sits in.
        // A card does not need artwork to fill space it is not short of.
        Box {
            Column(
                Modifier.padding(OmniBridgeSpacing.md),
                verticalArrangement = Arrangement.spacedBy(OmniBridgeSpacing.xs),
            ) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    OmniBridgeStatusBadge(status)
                    Spacer(Modifier.weight(1f))
                    Icon(
                        painter = painterResource(deviceIcon),
                        contentDescription = null,
                        tint = colors.textMuted,
                        modifier = Modifier.size(OmniBridgeIconSize.large),
                    )
                }
                Text(
                    name,
                    style = OmniBridgeType.title,
                    color = colors.textPrimary,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                // Monospace: this is a fingerprint, and a fingerprint is
                // compared character by character against another screen.
                Text(
                    subtitle,
                    style = OmniBridgeType.mono,
                    color = colors.textMuted,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                if (batteryPercent != null) {
                    OmniBridgeBatteryPill(
                        percentage = batteryPercent,
                        charging = batteryCharging,
                        stale = batteryStale,
                    )
                }
                if (footer != null) {
                    Spacer(Modifier.height(OmniBridgeSpacing.xxs))
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
fun OmniBridgeCapabilityRow(
    title: String,
    description: String,
    icon: Int,
    accent: Color,
    checked: Boolean,
    onCheckedChange: (Boolean) -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
) {
    val colors = OmniBridgeTheme.colors
    Row(
        modifier = modifier
            .fillMaxWidth()
            .heightIn(min = MinTouchTarget)
            .padding(vertical = OmniBridgeSpacing.xs),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(
            painter = painterResource(icon),
            contentDescription = null,
            tint = if (enabled) accent else colors.disabled,
            modifier = Modifier.size(OmniBridgeIconSize.large),
        )
        Spacer(Modifier.width(OmniBridgeSpacing.sm))
        Column(Modifier.weight(1f)) {
            Text(
                title,
                style = OmniBridgeType.body,
                color = if (enabled) colors.textPrimary else colors.disabled,
            )
            Text(
                description,
                style = OmniBridgeType.caption,
                color = if (enabled) colors.textSecondary else colors.disabled,
            )
        }
        Spacer(Modifier.width(OmniBridgeSpacing.xs))
        Switch(
            checked = checked,
            onCheckedChange = onCheckedChange,
            enabled = enabled,
            // The row already reads its own title and description; letting the
            // switch announce a second, contextless label would double it up.
            modifier = Modifier.semantics { contentDescription = title },
            colors = SwitchDefaults.colors(
                checkedTrackColor = colors.accentCyan,
                checkedThumbColor = Color.White,
                checkedBorderColor = colors.accentCyan,
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
 *
 * @param statusLabel the word for the state, when the caller owns a more
 *   specific vocabulary than [OmniBridgeStatus] does. The Files screen does:
 *   "Declined", "Timed out" and "Disconnected" are three different endings
 *   that all wear the same muted colour, and each is a separate localised
 *   string. The colour still comes from [status], so the two cannot disagree.
 * @param rowDescription what a screen reader reads instead of the card's
 *   four separate fragments. Supplied rather than assembled here, because
 *   only the caller knows the order the sentence should be in.
 * @param action a trailing action for a settled transfer — Open, typically.
 *   A slot rather than a label and a lambda so the caller keeps the decision
 *   about whether the action is offerable at all.
 */
@Composable
fun OmniBridgeTransferCard(
    filename: String,
    subtitle: String,
    status: OmniBridgeStatus,
    modifier: Modifier = Modifier,
    fraction: Float? = null,
    detail: String? = null,
    percentLabel: String? = null,
    icon: Int = R.drawable.ic_file,
    accent: Color? = null,
    statusLabel: String? = null,
    rowDescription: String? = null,
    cancelLabel: String = "Cancel",
    /** Names what would be cancelled; every row's button reads "Cancel". */
    cancelDescription: String? = null,
    onCancel: (() -> Unit)? = null,
    action: (@Composable () -> Unit)? = null,
) {
    val colors = OmniBridgeTheme.colors
    val tileAccent = accent ?: colors.accentBlue
    OmniBridgeCard(modifier) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            OmniBridgeIconTile(icon = icon, accent = tileAccent)
            Spacer(Modifier.width(OmniBridgeSpacing.sm))
            Column(
                Modifier
                    .weight(1f)
                    // One sentence, not four fragments. The filename is
                    // elided on screen when it is long; the description is
                    // not, so a screen reader still reads the whole name.
                    .then(
                        if (rowDescription == null) {
                            Modifier
                        } else {
                            Modifier.semantics(mergeDescendants = true) {
                                contentDescription = rowDescription
                            }
                        },
                    ),
            ) {
                Text(
                    filename,
                    style = OmniBridgeType.subtitle,
                    color = colors.textPrimary,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                Text(subtitle, style = OmniBridgeType.caption, color = colors.textSecondary)
            }
            if (percentLabel != null) {
                Spacer(Modifier.width(OmniBridgeSpacing.xs))
                Text(percentLabel, style = OmniBridgeType.label, color = colors.accentBlue)
            } else if (status == OmniBridgeStatus.Success) {
                Icon(
                    painter = painterResource(R.drawable.ic_check),
                    contentDescription = status.label,
                    tint = colors.accentCyan,
                    modifier = Modifier.size(OmniBridgeIconSize.large),
                )
            }
        }
        if (fraction != null || status == OmniBridgeStatus.Transferring) {
            Spacer(Modifier.height(OmniBridgeSpacing.xxs))
            OmniBridgeProgressBar(fraction)
        }
        Row(verticalAlignment = Alignment.CenterVertically) {
            if (statusLabel != null) {
                OmniBridgeStatusBadge(status, label = statusLabel, showIcon = false)
            } else if (detail != null) {
                Text(detail, style = OmniBridgeType.caption, color = colors.textSecondary)
            } else if (status != OmniBridgeStatus.Success) {
                OmniBridgeStatusBadge(status, showIcon = false)
            }
            if (statusLabel != null && detail != null) {
                Spacer(Modifier.width(OmniBridgeSpacing.xs))
                Text(detail, style = OmniBridgeType.caption, color = colors.textSecondary)
            }
            Spacer(Modifier.weight(1f))
            action?.invoke()
            if (onCancel != null) {
                OmniBridgeTextButton(
                    cancelLabel,
                    onCancel,
                    modifier = if (cancelDescription == null) {
                        Modifier
                    } else {
                        Modifier.semantics { contentDescription = cancelDescription }
                    },
                )
            }
        }
    }
}

/**
 * A semantic glyph on a soft disc of its own accent: the empty-state mark.
 *
 * Shared by the Files list and by the exchange flows' payload cards, so the
 * app has one way of saying "there is nothing here" rather than one per
 * screen. The glyph names the thing that is missing — a file, a clipboard —
 * because an abstract flourish tells a person nothing about *which* of the
 * app's empty states they are looking at.
 */
@Composable
fun OmniBridgeEmptyArt(
    icon: Int,
    accent: Color,
    modifier: Modifier = Modifier,
    size: Dp = 88.dp,
) {
    val tint = accent.copy(alpha = if (OmniBridgeTheme.colors.isDark) 0.16f else 0.08f)
    Box(
        modifier
            .size(size)
            .clip(CircleShape)
            .background(tint)
            .clearAndSetSemantics { },
        contentAlignment = Alignment.Center,
    ) {
        Icon(
            painter = painterResource(icon),
            contentDescription = null,
            tint = accent,
            modifier = Modifier.size(OmniBridgeIconSize.hero),
        )
    }
}

/**
 * Nothing here yet.
 *
 * @param icon the semantic mark for what is missing, drawn by
 *   [OmniBridgeEmptyArt]. The Files list passes one: its empty state used to
 *   be the connection ribbon, a thin tricolour stroke that read as a stray
 *   underline above the text and said nothing about files.
 *
 *   Null keeps the ribbon, and exactly one caller wants it — the "connect
 *   your first device" state, where "two devices, one flow" is not decoration
 *   but literally the thing the person is being invited to create.
 */
@Composable
fun OmniBridgeEmptyState(
    title: String,
    subtitle: String,
    modifier: Modifier = Modifier,
    icon: Int? = null,
    accent: Color? = null,
    actionLabel: String? = null,
    onAction: (() -> Unit)? = null,
    /**
     * Whether this empty state owns the whole content area.
     *
     * When it does — a Files tab with no transfers, say — it is placed into
     * that area rather than left against the top edge. On an 1440dp-tall
     * tablet the difference is the whole point: the same block that looks
     * deliberate on a phone looks abandoned under the app bar of a tablet.
     *
     * Off by default because the other caller puts this inside a
     * `LazyColumn` item, where the height is **unbounded** and a weighted
     * spacer would not merely look wrong, it would fail to measure.
     */
    fillsContentArea: Boolean = false,
) {
    val colors = OmniBridgeTheme.colors
    Column(
        modifier = modifier
            .fillMaxWidth()
            .padding(vertical = OmniBridgeSpacing.xxl, horizontal = OmniBridgeSpacing.lg),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(OmniBridgeSpacing.sm),
    ) {
        // Optical placement, not arithmetic centring: the leading space is
        // the smaller of the two, so the block settles a little above the
        // true middle. Centred exactly, a short block under a top app bar
        // reads as sitting low.
        if (fillsContentArea) Spacer(Modifier.weight(0.62f))
        if (icon != null) {
            OmniBridgeEmptyArt(icon = icon, accent = accent ?: colors.accentBlue)
            Spacer(Modifier.height(OmniBridgeSpacing.xxs))
        } else {
            Icon(
                painter = painterResource(R.drawable.ribbon_connection),
                contentDescription = null,
                tint = Color.Unspecified,
                modifier = Modifier
                    .width(160.dp)
                    .height(58.dp),
            )
        }
        Text(
            title,
            style = OmniBridgeType.heading,
            color = colors.textPrimary,
            textAlign = TextAlign.Center,
        )
        Text(
            subtitle,
            style = OmniBridgeType.body,
            color = colors.textSecondary,
            textAlign = TextAlign.Center,
        )
        if (actionLabel != null && onAction != null) {
            Spacer(Modifier.height(OmniBridgeSpacing.xs))
            OmniBridgePrimaryButton(
                text = actionLabel,
                onClick = onAction,
                icon = R.drawable.ic_qr,
                modifier = Modifier.width(240.dp),
            )
        }
        if (fillsContentArea) Spacer(Modifier.weight(1f))
    }
}
