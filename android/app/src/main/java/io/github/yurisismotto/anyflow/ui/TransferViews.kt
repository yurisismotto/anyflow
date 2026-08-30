package io.github.yurisismotto.anyflow.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Card
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import io.github.yurisismotto.anyflow.files.FileTransferManager
import io.github.yurisismotto.anyflow.files.TransferState

/**
 * The whole file-transfer UI for this sprint: enough to choose a device,
 * accept or reject, watch progress, and see how it ended. Deliberately plain.
 */

/** A file a computer wants to send. Nothing arrives until this is answered. */
@Composable
fun IncomingOfferCard(
    offer: FileTransferManager.IncomingOffer,
    onRespond: (Boolean) -> Unit,
) {
    Card(Modifier.fillMaxWidth()) {
        Column(Modifier.padding(12.dp), Arrangement.spacedBy(4.dp)) {
            Text("Incoming file", style = MaterialTheme.typography.titleMedium)
            // Already sanitized by the time it reaches here: a raw
            // peer-supplied name could forge the rest of this card.
            Text(offer.filename)
            Text(humanBytes(offer.sizeBytes), style = MaterialTheme.typography.bodySmall)
            Text(
                "from ${offer.peer.toDisplayShort()}",
                style = MaterialTheme.typography.bodySmall,
            )
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                TextButton(onClick = { onRespond(true) }) { Text("Accept") }
                TextButton(onClick = { onRespond(false) }) { Text("Reject") }
            }
        }
    }
}

/** One transfer's progress and outcome. */
@Composable
fun TransferRow(
    transfer: FileTransferManager.TransferUi,
    onCancel: (() -> Unit)? = null,
) {
    Card(Modifier.fillMaxWidth()) {
        Column(Modifier.padding(12.dp), Arrangement.spacedBy(4.dp)) {
            Text(
                "${if (transfer.sending) "Sending" else "Receiving"} ${transfer.filename}",
                style = MaterialTheme.typography.titleSmall,
            )

            when (transfer.state) {
                TransferState.TRANSFERRING -> {
                    val pct = transfer.percentage
                    if (pct != null) {
                        LinearProgressIndicator(
                            progress = { pct / 100f },
                            modifier = Modifier.fillMaxWidth(),
                        )
                        Text(
                            "$pct%  ${humanBytes(transfer.bytesTransferred)} of " +
                                humanBytes(transfer.sizeBytes),
                            style = MaterialTheme.typography.bodySmall,
                        )
                    } else {
                        Text("Transferring…", style = MaterialTheme.typography.bodySmall)
                    }
                }

                // Named separately from "transferring": the bytes have all
                // arrived and the hash is being checked, which is a different
                // thing to be waiting for.
                TransferState.VERIFYING ->
                    Text("Checking the file…", style = MaterialTheme.typography.bodySmall)

                TransferState.COMPLETED ->
                    Text(
                        if (transfer.sending) "Sent" else "Saved to Downloads/AnyFlow",
                        style = MaterialTheme.typography.bodySmall,
                    )

                // A cancellation is not an error, and is not displayed as one.
                TransferState.CANCELLED ->
                    Text(
                        transfer.failure?.display ?: "Cancelled",
                        style = MaterialTheme.typography.bodySmall,
                    )

                TransferState.FAILED ->
                    Text(
                        "Failed: ${transfer.failure?.display ?: "unknown reason"}",
                        style = MaterialTheme.typography.bodySmall,
                    )

                TransferState.OFFERED, TransferState.WAITING_ACCEPT ->
                    Text("Waiting…", style = MaterialTheme.typography.bodySmall)
            }

            if (onCancel != null && transfer.state.isActive) {
                TextButton(onClick = onCancel) { Text("Cancel") }
            }
        }
    }
}

fun humanBytes(bytes: Long): String {
    val units = listOf("B", "KB", "MB", "GB", "TB")
    var value = bytes.toDouble()
    var unit = 0
    while (value >= 1024 && unit < units.size - 1) {
        value /= 1024
        unit++
    }
    return if (unit == 0) "$bytes B" else "%.1f %s".format(value, units[unit])
}
