package io.github.yurisismotto.anyflow.ui

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
import io.github.yurisismotto.anyflow.R
import io.github.yurisismotto.anyflow.clipboard.ClipboardSync
import io.github.yurisismotto.anyflow.ui.components.AnyFlowCard
import io.github.yurisismotto.anyflow.ui.components.AnyFlowIconTile
import io.github.yurisismotto.anyflow.ui.components.AnyFlowPrimaryButton
import io.github.yurisismotto.anyflow.ui.components.AnyFlowSecondaryButton
import io.github.yurisismotto.anyflow.ui.theme.AnyFlowRadius
import io.github.yurisismotto.anyflow.ui.theme.AnyFlowSpacing
import io.github.yurisismotto.anyflow.ui.theme.AnyFlowTheme
import io.github.yurisismotto.anyflow.ui.theme.AnyFlowType

/**
 * The extra confirmation for a clip the platform marked sensitive.
 *
 * Android's `EXTRA_IS_SENSITIVE` is a hint from whichever app produced the
 * clip — a password manager sets it. It is not an access control and nothing
 * in AnyFlow depends on it. What it earns is this: one more deliberate act
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
    val colors = AnyFlowTheme.colors
    AlertDialog(
        onDismissRequest = onDismiss,
        containerColor = colors.surface,
        titleContentColor = colors.textPrimary,
        textContentColor = colors.textSecondary,
        shape = androidx.compose.foundation.shape.RoundedCornerShape(AnyFlowRadius.large),
        title = {
            Text("This clipboard is marked sensitive", style = AnyFlowType.subtitle)
        },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(AnyFlowSpacing.xs)) {
                Text(
                    "The app you copied from marked this text as sensitive — " +
                        "a password, a recovery code, or similar.",
                    style = AnyFlowType.body,
                )
                Text("Send $bytes bytes to $computerName?", style = AnyFlowType.body)
            }
        },
        confirmButton = {
            TextButton(onClick = onConfirm) {
                Text("Send", style = AnyFlowType.body, color = colors.accentRed)
            }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) {
                Text("Cancel", style = AnyFlowType.body, color = colors.textSecondary)
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
    val colors = AnyFlowTheme.colors
    AnyFlowCard(modifier) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            AnyFlowIconTile(
                icon = if (clip.sensitive) R.drawable.ic_shield else R.drawable.ic_clipboard,
                accent = if (clip.sensitive) colors.accentAmber else colors.accentTeal,
            )
            Spacer(Modifier.width(AnyFlowSpacing.sm))
            Column {
                Text(
                    "Clipboard from ${clip.peerName}",
                    style = AnyFlowType.subtitle,
                    color = colors.textPrimary,
                )
                Text(
                    buildString {
                        append("${clip.bytes} bytes")
                        if (clip.sensitive) append(" · marked sensitive")
                    },
                    style = AnyFlowType.caption,
                    color = colors.textSecondary,
                )
            }
        }
        Spacer(Modifier.width(AnyFlowSpacing.xxs))
        Row(horizontalArrangement = Arrangement.spacedBy(AnyFlowSpacing.xs)) {
            AnyFlowPrimaryButton(
                text = "Copy",
                onClick = onApply,
                icon = R.drawable.ic_clipboard,
                modifier = Modifier.weight(1f),
            )
            AnyFlowSecondaryButton(
                text = "Dismiss",
                onClick = onDismiss,
                modifier = Modifier.weight(1f),
            )
        }
    }
}
