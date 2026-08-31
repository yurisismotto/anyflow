package io.github.yurisismotto.anyflow.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import io.github.yurisismotto.anyflow.clipboard.ClipboardCapabilities
import io.github.yurisismotto.anyflow.clipboard.ClipboardPolicy
import io.github.yurisismotto.anyflow.clipboard.ClipboardSync
import io.github.yurisismotto.anyflow.store.TrustStore

/**
 * The clipboard controls on a computer's card.
 *
 * Three things are kept visibly separate, because collapsing them is how a
 * person comes to believe sync is running when it is not:
 *
 *  * whether `clipboard.v1` is **granted** at all;
 *  * what the per-direction **policy** permits;
 *  * what this **platform** can actually do — Android cannot push
 *    automatically, and the UI says so instead of offering a dead toggle.
 */
@Composable
fun ClipboardSection(
    peer: TrustStore.TrustedPeer,
    connected: Boolean,
    lastOutcome: ClipboardSync.Outcome?,
    onSetGrant: (Boolean) -> Unit,
    onSetPolicy: (ClipboardPolicy) -> Unit,
    onSendClipboard: () -> Unit,
) {
    val granted = peer.allows(TrustStore.CLIPBOARD_CAPABILITY_ID)
    val policy = peer.clipboardPolicy

    Column(Modifier.padding(top = 8.dp), Arrangement.spacedBy(4.dp)) {
        Text("Clipboard", style = MaterialTheme.typography.titleSmall)

        SwitchRow(
            label = "Share clipboard with this computer",
            checked = granted,
            onCheckedChange = onSetGrant,
        )

        if (!granted) {
            Text(
                "Off. This computer cannot send or receive clipboard text.",
                style = MaterialTheme.typography.bodySmall,
            )
            return@Column
        }

        SwitchRow(
            label = "Receive clipboard from this computer",
            checked = policy.allowReceive,
            onCheckedChange = { onSetPolicy(policy.copy(allowReceive = it)) },
        )

        SwitchRow(
            label = "Apply received clipboard automatically",
            checked = policy.autoReceive,
            enabled = policy.allowReceive,
            onCheckedChange = { onSetPolicy(policy.copy(autoReceive = it)) },
        )
        Text(
            if (policy.mayAutoReceive()) {
                "Received text replaces your clipboard as it arrives."
            } else {
                "Received text waits in a notification until you tap Copy."
            },
            style = MaterialTheme.typography.bodySmall,
        )

        SwitchRow(
            label = "Send clipboard to this computer",
            checked = policy.allowSend,
            onCheckedChange = { onSetPolicy(policy.copy(allowSend = it)) },
        )

        // No auto-send toggle. Offering one that cannot work would be worse
        // than not offering it: the person would turn it on and quietly get
        // nothing. The reason is stated instead.
        if (!ClipboardCapabilities.AUTO_SEND_SUPPORTED) {
            Text(
                ClipboardCapabilities.AUTO_SEND_REASON,
                style = MaterialTheme.typography.bodySmall,
            )
        }

        Button(
            onClick = onSendClipboard,
            enabled = granted && policy.allowSend && connected,
            modifier = Modifier.fillMaxWidth(),
        ) {
            Text("Send clipboard")
        }
        if (!connected) {
            Text(
                "Connect to this computer to send your clipboard.",
                style = MaterialTheme.typography.bodySmall,
            )
        }

        lastOutcome?.let {
            Text("Last result: ${it.name.lowercase().replace('_', ' ')}",
                style = MaterialTheme.typography.bodySmall)
        }
    }
}

@Composable
private fun SwitchRow(
    label: String,
    checked: Boolean,
    onCheckedChange: (Boolean) -> Unit,
    enabled: Boolean = true,
) {
    Row(
        modifier = Modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.SpaceBetween,
    ) {
        Text(label, style = MaterialTheme.typography.bodyMedium)
        Switch(checked = checked, onCheckedChange = onCheckedChange, enabled = enabled)
    }
}

/**
 * The extra confirmation for a clip the platform marked sensitive.
 *
 * Android's `EXTRA_IS_SENSITIVE` is a hint from whichever app produced the
 * clip — a password manager sets it. It is not an access control and nothing
 * in AnyFlow depends on it. What it earns is this: one more deliberate act
 * before a password leaves the device, with the destination named, so
 * "Send clipboard" cannot become a one-tap exfiltration of a credential the
 * person forgot was there.
 */
@Composable
fun SensitiveClipDialog(
    computerName: String,
    bytes: Int,
    onConfirm: () -> Unit,
    onDismiss: () -> Unit,
) {
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("This clipboard is marked sensitive") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                // The size and the destination — never the text. Rendering a
                // preview here would put a password on screen, which is
                // exactly what the sensitive flag exists to prevent.
                Text(
                    "The app you copied from marked this text as sensitive — " +
                        "a password, a recovery code, or similar.",
                )
                Text("Send $bytes bytes to $computerName?")
            }
        },
        confirmButton = { TextButton(onClick = onConfirm) { Text("Send") } },
        dismissButton = { TextButton(onClick = onDismiss) { Text("Cancel") } },
    )
}

/** A clip held because auto-apply is off. Shows size and origin, never text. */
@Composable
fun PendingClipCard(
    clip: ClipboardSync.PendingClipInfo,
    onApply: () -> Unit,
    onDismiss: () -> Unit,
) {
    Card(Modifier.fillMaxWidth()) {
        Column(Modifier.padding(12.dp), Arrangement.spacedBy(4.dp)) {
            Text(
                "Clipboard from ${clip.peerName}",
                style = MaterialTheme.typography.titleMedium,
            )
            Text(
                buildString {
                    append("${clip.bytes} bytes")
                    if (clip.sensitive) append(" · marked sensitive")
                },
                style = MaterialTheme.typography.bodySmall,
            )
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Button(onClick = onApply) { Text("Copy") }
                OutlinedButton(onClick = onDismiss) { Text("Dismiss") }
            }
        }
    }
}
