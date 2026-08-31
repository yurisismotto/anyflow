package io.github.yurisismotto.anyflow.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import io.github.yurisismotto.anyflow.ui.components.AnyFlowEmptyState
import io.github.yurisismotto.anyflow.ui.components.AnyFlowSectionLabel
import io.github.yurisismotto.anyflow.ui.theme.AnyFlowSpacing
import io.github.yurisismotto.anyflow.ui.theme.AnyFlowTheme
import io.github.yurisismotto.anyflow.ui.theme.AnyFlowType

/**
 * What is happening right now.
 *
 * ## Why there is no History tab
 *
 * The design reference shows "Incoming | History". AnyFlow has no history to
 * show: transfers live in memory for the life of the process and clipboard
 * content is never persisted at all, by design. A History tab would either be
 * permanently empty or would have to be fed by starting to store what the
 * product deliberately does not store.
 *
 * So this screen shows the truth — work in flight, and what finished since
 * the app started — and the "Completed" heading says *since this app
 * started* rather than implying a durable log.
 */
@Composable
fun ActivityScreen(
    state: MainUiState,
    actions: MainActions,
    modifier: Modifier = Modifier,
) {
    val colors = AnyFlowTheme.colors
    val active = state.transfers.filter { it.state.isActive }
    val finished = state.transfers.filterNot { it.state.isActive }

    if (!state.hasActivity) {
        AnyFlowEmptyState(
            modifier = modifier.fillMaxSize(),
            title = "Nothing in flight",
            subtitle = "Files and clipboard text you send or receive will appear here " +
                "while they are moving.",
        )
        return
    }

    LazyColumn(
        modifier = modifier
            .fillMaxSize()
            .padding(horizontal = AnyFlowSpacing.md),
        verticalArrangement = Arrangement.spacedBy(AnyFlowSpacing.sm),
        contentPadding = PaddingValues(bottom = AnyFlowSpacing.xxl),
    ) {
        if (state.offers.isNotEmpty()) {
            item { AnyFlowSectionLabel("Waiting for you") }
            items(state.offers, key = { UiMapping.offerKey(it.transferId) }) { offer ->
                IncomingOfferCard(
                    offer = offer,
                    onRespond = { accept -> actions.onRespondToOffer(offer.transferId, accept) },
                )
            }
        }

        if (state.pendingClips.isNotEmpty()) {
            item { AnyFlowSectionLabel("Clipboard waiting") }
            items(state.pendingClips, key = { UiMapping.clipKey(it.peer.toHex()) }) { clip ->
                PendingClipCard(
                    clip = clip,
                    onApply = { actions.onApplyClip(clip.peer) },
                    onDismiss = { actions.onDismissClip(clip.peer) },
                )
            }
        }

        if (active.isNotEmpty()) {
            item { AnyFlowSectionLabel("In progress") }
            items(active, key = { UiMapping.transferKey(it.transferId) }) { transfer ->
                TransferRow(
                    transfer = transfer,
                    onCancel = { actions.onCancelTransfer(transfer.transferId) },
                )
            }
        }

        if (finished.isNotEmpty()) {
            item { AnyFlowSectionLabel("Completed") }
            items(finished, key = { UiMapping.transferKey(it.transferId) }) { transfer ->
                TransferRow(transfer = transfer)
            }
            item {
                // Says plainly that this list is not a log.
                Text(
                    "Cleared when AnyFlow closes. Nothing about a transfer is kept " +
                        "on disk once it finishes.",
                    style = AnyFlowType.caption,
                    color = colors.textMuted,
                )
            }
        }
    }
}

