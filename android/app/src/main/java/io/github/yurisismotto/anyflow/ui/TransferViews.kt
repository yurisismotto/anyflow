package io.github.yurisismotto.anyflow.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.width
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import io.github.yurisismotto.anyflow.R
import io.github.yurisismotto.anyflow.files.FileTransferManager
import io.github.yurisismotto.anyflow.files.TransferState
import io.github.yurisismotto.anyflow.ui.components.AnyFlowCard
import io.github.yurisismotto.anyflow.ui.components.AnyFlowIconTile
import io.github.yurisismotto.anyflow.ui.components.AnyFlowPrimaryButton
import io.github.yurisismotto.anyflow.ui.components.AnyFlowSecondaryButton
import io.github.yurisismotto.anyflow.ui.components.AnyFlowTransferCard
import io.github.yurisismotto.anyflow.ui.theme.AnyFlowSpacing
import io.github.yurisismotto.anyflow.ui.theme.AnyFlowStatus
import io.github.yurisismotto.anyflow.ui.theme.AnyFlowTheme
import io.github.yurisismotto.anyflow.ui.theme.AnyFlowType

/**
 * File transfer, in the AnyFlow visual language.
 *
 * The behaviour is unchanged from the previous release — accept or reject,
 * watch progress, see the outcome. What changed is only how it looks.
 */

/** A file a computer wants to send. Nothing arrives until this is answered. */
@Composable
fun IncomingOfferCard(
    offer: FileTransferManager.IncomingOffer,
    onRespond: (Boolean) -> Unit,
    modifier: Modifier = Modifier,
) {
    val colors = AnyFlowTheme.colors
    AnyFlowCard(modifier) {
        Row(verticalAlignment = androidx.compose.ui.Alignment.CenterVertically) {
            AnyFlowIconTile(icon = R.drawable.ic_download, accent = colors.accentViolet)
            Spacer(Modifier.width(AnyFlowSpacing.sm))
            androidx.compose.foundation.layout.Column {
                Text("Incoming file", style = AnyFlowType.label, color = colors.accentViolet)
                // Already sanitized before it reaches here: a raw peer-supplied
                // name could otherwise forge the rest of this card.
                Text(offer.filename, style = AnyFlowType.subtitle, color = colors.textPrimary)
                Text(
                    "${humanBytes(offer.sizeBytes)} · from ${offer.peer.toDisplayShort()}",
                    style = AnyFlowType.caption,
                    color = colors.textSecondary,
                )
            }
        }
        Spacer(Modifier.width(AnyFlowSpacing.xs))
        Row(horizontalArrangement = Arrangement.spacedBy(AnyFlowSpacing.xs)) {
            AnyFlowPrimaryButton(
                text = "Accept",
                onClick = { onRespond(true) },
                icon = R.drawable.ic_check,
                modifier = Modifier.weight(1f),
            )
            AnyFlowSecondaryButton(
                text = "Reject",
                onClick = { onRespond(false) },
                modifier = Modifier.weight(1f),
            )
        }
    }
}

/** One transfer's progress and outcome. */
@Composable
fun TransferRow(
    transfer: FileTransferManager.TransferUi,
    modifier: Modifier = Modifier,
    onCancel: (() -> Unit)? = null,
) {
    val colors = AnyFlowTheme.colors
    val direction = if (transfer.sending) "To" else "From"
    val status = when (transfer.state) {
        TransferState.TRANSFERRING -> AnyFlowStatus.Transferring
        TransferState.VERIFYING -> AnyFlowStatus.Transferring
        TransferState.COMPLETED -> AnyFlowStatus.Success
        // A cancellation is not an error and is not displayed as one.
        TransferState.CANCELLED -> AnyFlowStatus.Disconnected
        TransferState.FAILED -> AnyFlowStatus.Error
        TransferState.OFFERED, TransferState.WAITING_ACCEPT -> AnyFlowStatus.Connecting
    }
    val detail = when (transfer.state) {
        TransferState.TRANSFERRING -> transfer.percentage?.let {
            "${humanBytes(transfer.bytesTransferred)} of ${humanBytes(transfer.sizeBytes)}"
        }
        // Named apart from "transferring": the bytes have all arrived and the
        // hash is being checked, which is a different thing to be waiting for.
        TransferState.VERIFYING -> "Checking the file…"
        TransferState.COMPLETED ->
            if (transfer.sending) "Sent" else "Saved to Downloads/AnyFlow"
        TransferState.CANCELLED -> transfer.failure?.display ?: "Cancelled"
        TransferState.FAILED -> "Failed: ${transfer.failure?.display ?: "unknown reason"}"
        TransferState.OFFERED, TransferState.WAITING_ACCEPT -> "Waiting…"
    }

    AnyFlowTransferCard(
        modifier = modifier,
        filename = transfer.filename,
        subtitle = "$direction ${humanBytes(transfer.sizeBytes)}",
        status = status,
        fraction = transfer.percentage?.let { it / 100f },
        detail = detail,
        percentLabel = transfer.percentage
            ?.takeIf { transfer.state == TransferState.TRANSFERRING }
            ?.let { "$it%" },
        icon = if (transfer.sending) R.drawable.ic_send else R.drawable.ic_download,
        accent = if (transfer.sending) colors.accentBlue else colors.accentViolet,
        onCancel = onCancel?.takeIf { transfer.state.isActive },
    )
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
