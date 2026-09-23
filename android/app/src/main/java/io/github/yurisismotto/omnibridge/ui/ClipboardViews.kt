package io.github.yurisismotto.omnibridge.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.width
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import io.github.yurisismotto.omnibridge.R
import io.github.yurisismotto.omnibridge.clipboard.ClipboardSync
import io.github.yurisismotto.omnibridge.ui.components.OmniBridgeCard
import io.github.yurisismotto.omnibridge.ui.components.OmniBridgeIconTile
import io.github.yurisismotto.omnibridge.ui.components.OmniBridgePrimaryButton
import io.github.yurisismotto.omnibridge.ui.components.OmniBridgeSecondaryButton
import io.github.yurisismotto.omnibridge.ui.theme.OmniBridgeRadius
import io.github.yurisismotto.omnibridge.ui.theme.OmniBridgeSpacing
import io.github.yurisismotto.omnibridge.ui.theme.OmniBridgeTheme
import io.github.yurisismotto.omnibridge.ui.theme.OmniBridgeType

/**
 * The extra confirmation for a clip the platform marked sensitive.
 *
 * Android's `EXTRA_IS_SENSITIVE` is a hint from whichever app produced the
 * clip — a password manager sets it. It is not an access control and nothing
 * in OmniBridge depends on it. What it earns is this: one more deliberate act
 * before a password leaves the device, with the destination named, so
 * "Send clipboard" cannot become a one-tap exfiltration of a credential the
 * person forgot was there.
 *
 * The size and the destination are shown — never the text. Rendering a
 * preview here would put a password on screen, which is exactly what the
 * sensitive flag exists to prevent.
 */
@Composable
fun SensitiveClipDialog(
    computerName: String,
    bytes: Int,
    onConfirm: () -> Unit,
    onDismiss: () -> Unit,
) {
    val colors = OmniBridgeTheme.colors
    AlertDialog(
        onDismissRequest = onDismiss,
        containerColor = colors.surface,
        titleContentColor = colors.textPrimary,
        textContentColor = colors.textSecondary,
        shape = androidx.compose.foundation.shape.RoundedCornerShape(OmniBridgeRadius.large),
        title = {
            Text("This clipboard is marked sensitive", style = OmniBridgeType.subtitle)
        },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(OmniBridgeSpacing.xs)) {
                Text(
                    "The app you copied from marked this text as sensitive — " +
                        "a password, a recovery code, or similar.",
                    style = OmniBridgeType.body,
                )
                Text("Send $bytes bytes to $computerName?", style = OmniBridgeType.body)
            }
        },
        confirmButton = {
            TextButton(onClick = onConfirm) {
                Text("Send", style = OmniBridgeType.body, color = colors.accentRed)
            }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) {
                Text("Cancel", style = OmniBridgeType.body, color = colors.textSecondary)
            }
        },
    )
}

/** A clip held because auto-apply is off. Shows size and origin, never text. */
@Composable
fun PendingClipCard(
    clip: ClipboardSync.PendingClipInfo,
    onApply: () -> Unit,
    onDismiss: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val colors = OmniBridgeTheme.colors
    OmniBridgeCard(modifier) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            OmniBridgeIconTile(
                icon = if (clip.sensitive) R.drawable.ic_shield else R.drawable.ic_clipboard,
                accent = if (clip.sensitive) colors.accentAmber else colors.accentCyan,
            )
            Spacer(Modifier.width(OmniBridgeSpacing.sm))
            Column {
                Text(
                    "Clipboard from ${clip.peerName}",
                    style = OmniBridgeType.subtitle,
                    color = colors.textPrimary,
                )
                Text(
                    buildString {
                        append("${clip.bytes} bytes")
                        if (clip.sensitive) append(" · marked sensitive")
                    },
                    style = OmniBridgeType.caption,
                    color = colors.textSecondary,
                )
            }
        }
        Spacer(Modifier.width(OmniBridgeSpacing.xxs))
        Row(horizontalArrangement = Arrangement.spacedBy(OmniBridgeSpacing.xs)) {
            OmniBridgePrimaryButton(
                text = "Copy",
                onClick = onApply,
                icon = R.drawable.ic_clipboard,
                modifier = Modifier.weight(1f),
            )
            OmniBridgeSecondaryButton(
                text = "Dismiss",
                onClick = onDismiss,
                modifier = Modifier.weight(1f),
            )
        }
    }
}
