package io.github.yurisismotto.anyflow.ui

import androidx.annotation.StringRes
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import io.github.yurisismotto.anyflow.R
import io.github.yurisismotto.anyflow.files.OpenAction
import io.github.yurisismotto.anyflow.ui.components.AnyFlowEmptyState
import io.github.yurisismotto.anyflow.ui.components.AnyFlowSectionLabel
import io.github.yurisismotto.anyflow.ui.components.AnyFlowTextButton
import io.github.yurisismotto.anyflow.ui.components.AnyFlowTransferCard
import io.github.yurisismotto.anyflow.ui.theme.AnyFlowSpacing
import io.github.yurisismotto.anyflow.ui.theme.AnyFlowStatus
import io.github.yurisismotto.anyflow.ui.theme.AnyFlowTheme
import io.github.yurisismotto.anyflow.ui.theme.AnyFlowType

/**
 * Files: what is moving between this device and a paired computer, and what
 * moved earlier in this session.
 *
 * ## What this screen is, and what it is careful not to claim to be
 *
 * It is not a history. AnyFlow writes nothing about a transfer to disk —
 * that is a product decision, not an omission — so the list is everything
 * this *process* has done, and it goes away when the process does. The screen
 * says so, in [R.string.files_recent_scope], rather than letting a person
 * discover it by force-stopping the app and losing what looked like a log.
 *
 * The file itself is a different matter and outlives all of this: a received
 * file is in `Downloads/AnyFlow` and survives the app being uninstalled. That
 * is the sentence the scope note ends on, because "the list is gone" and
 * "your file is gone" are very different pieces of news.
 *
 * ## Why every decision is somewhere else
 *
 * Nothing on this screen works out whether a transfer succeeded, what it is
 * called, whose it is, or whether it can be opened. [FilesMapping] does, in
 * plain functions over plain data, and this file renders the answer. That is
 * what lets the claims be tested in milliseconds on the JVM rather than by
 * looking at a tablet.
 *
 * ## Live, without a timer
 *
 * The rows come from `FileTransferManager.visible`, a `StateFlow` the
 * manager republishes on every state change. There is no polling anywhere in
 * this screen: an offer, a byte of progress, a decline and a clear all arrive
 * as a new list.
 *
 * The one thing that cannot announce itself is a file being deleted, or a URI
 * grant lapsing, while the screen is open — nothing calls back for either. So
 * [MainActions.onRefreshOpenTargets] is run once when the screen appears,
 * which is the moment a stale Open button would be seen.
 */
@Composable
fun FilesScreen(
    state: MainUiState,
    actions: MainActions,
    modifier: Modifier = Modifier,
) {
    val colors = AnyFlowTheme.colors
    val files = FilesMapping.build(state.transfers, state.listedPeers)

    // Re-check what is still openable when the screen comes into view. Keyed
    // on the set of rows, so it also runs when a transfer settles while the
    // screen is already open — and not on every recomposition.
    LaunchedEffect(files.recent.map { it.transferId }) {
        actions.onRefreshOpenTargets()
    }

    if (files.isEmpty && state.offers.isEmpty()) {
        AnyFlowEmptyState(
            modifier = modifier.fillMaxSize(),
            title = stringResource(R.string.files_empty_title),
            subtitle = stringResource(R.string.files_empty_subtitle),
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
        // An offer nobody has answered is the only thing here that needs a
        // person, so it stays at the top and keeps its own accept/reject card
        // rather than becoming a row with a status of "Waiting".
        if (state.offers.isNotEmpty()) {
            item { AnyFlowSectionLabel(stringResource(R.string.files_section_active)) }
            items(state.offers, key = { UiMapping.offerKey(it.transferId) }) { offer ->
                IncomingOfferCard(
                    offer = offer,
                    peerLabel = FilesMapping.peerLabel(offer.peer, state.listedPeers),
                    onRespond = { accept -> actions.onRespondToOffer(offer.transferId, accept) },
                )
            }
        }

        if (files.active.isNotEmpty()) {
            if (state.offers.isEmpty()) {
                item { AnyFlowSectionLabel(stringResource(R.string.files_section_active)) }
            }
            items(files.active, key = { it.key }) { row ->
                FileTransferRow(
                    row = row,
                    onCancel = { actions.onCancelTransfer(row.transferId) },
                )
            }
        }

        if (files.recent.isNotEmpty()) {
            item { AnyFlowSectionLabel(stringResource(R.string.files_section_recent)) }
            items(files.recent, key = { it.key }) { row ->
                FileTransferRow(
                    row = row,
                    onOpen = { actions.onOpenTransfer(row.transferId) },
                )
            }
            item {
                Text(
                    stringResource(R.string.files_recent_scope),
                    style = AnyFlowType.caption,
                    color = colors.textMuted,
                )
            }
        }
    }
}

/**
 * One transfer.
 *
 * Every string comes from resources and every one of them is chosen by
 * [FilesMapping] rather than here: this function turns a [FilesMapping.FileRow]
 * into pixels and makes no judgements of its own.
 */
@Composable
private fun FileTransferRow(
    row: FilesMapping.FileRow,
    modifier: Modifier = Modifier,
    onCancel: (() -> Unit)? = null,
    onOpen: (() -> Unit)? = null,
) {
    val colors = AnyFlowTheme.colors
    val directionLine = stringResource(
        if (row.direction == FilesMapping.Direction.SENT) {
            R.string.files_to_device
        } else {
            R.string.files_from_device
        },
        row.peerLabel,
    )
    val size = row.sizeBytes?.let { humanBytes(it) }
    val subtitle = if (size == null) directionLine else "$directionLine · $size"
    val statusLabel = stringResource(statusString(row.status))
    val openDescription = stringResource(R.string.files_action_open_description, row.displayName)

    // "pasture-map.pdf, Received from Fedora, 2.4 MB" — one sentence, with
    // the direction and the state in words. An icon and an accent colour say
    // the same thing for people who can see them, and neither is the only
    // place either fact appears.
    val description = if (size == null) {
        stringResource(R.string.files_row_description_no_size, row.displayName, "$statusLabel, $directionLine")
    } else {
        stringResource(R.string.files_row_description, row.displayName, "$statusLabel, $directionLine", size)
    }

    AnyFlowTransferCard(
        modifier = modifier,
        filename = row.displayName,
        subtitle = subtitle,
        status = statusTone(row.status),
        statusLabel = statusLabel,
        rowDescription = description,
        fraction = row.progressPercent?.let { it / 100f },
        percentLabel = row.progressPercent?.let { "$it%" },
        icon = if (row.direction == FilesMapping.Direction.SENT) {
            R.drawable.ic_send
        } else {
            R.drawable.ic_download
        },
        accent = if (row.direction == FilesMapping.Direction.SENT) {
            colors.accentBlue
        } else {
            colors.accentViolet
        },
        cancelLabel = stringResource(R.string.action_cancel),
        cancelDescription = stringResource(
            R.string.files_action_cancel_description,
            row.displayName,
        ),
        onCancel = onCancel?.takeIf { row.isActive },
        // Drawn only when opening is structurally possible. A button that
        // exists in order to explain why it does not work is worse than no
        // button: the row's status already says what happened, and the
        // failures that can only be discovered by trying are reported when
        // they are actually discovered.
        action = if (onOpen != null && row.openAction == OpenAction.Available) {
            {
                AnyFlowTextButton(
                    text = stringResource(R.string.files_action_open),
                    onClick = onOpen,
                    // Every row's button says "Open". Which file it opens is
                    // the part a screen reader has no other way to know.
                    modifier = Modifier.semantics {
                        contentDescription = openDescription
                    },
                )
            }
        } else {
            null
        },
    )
}

/**
 * The localised word for a status.
 *
 * A `when` over the enum rather than a name stored on it: a resource id
 * belongs to the UI layer, and [FilesMapping] must stay a plain Kotlin object
 * that a JVM test can call without an Android context.
 */
@StringRes
internal fun statusString(status: FilesMapping.FileStatus): Int = when (status) {
    FilesMapping.FileStatus.WAITING -> R.string.files_status_waiting
    FilesMapping.FileStatus.SENDING -> R.string.files_status_sending
    FilesMapping.FileStatus.RECEIVING -> R.string.files_status_receiving
    FilesMapping.FileStatus.SENT -> R.string.files_status_sent
    FilesMapping.FileStatus.RECEIVED -> R.string.files_status_received
    FilesMapping.FileStatus.DECLINED -> R.string.files_status_declined
    FilesMapping.FileStatus.CANCELLED -> R.string.files_status_cancelled
    FilesMapping.FileStatus.TIMED_OUT -> R.string.files_status_timed_out
    FilesMapping.FileStatus.DISCONNECTED -> R.string.files_status_disconnected
    FilesMapping.FileStatus.FAILED -> R.string.files_status_failed
}

/**
 * The colour a status wears.
 *
 * Only the colour: the word beside it comes from [statusString], so a person
 * who cannot separate the tones still reads the difference between "Declined"
 * and "Failed". A decline and a cancellation are not errors and are not
 * painted as ones.
 */
internal fun statusTone(status: FilesMapping.FileStatus): AnyFlowStatus = when (status) {
    FilesMapping.FileStatus.WAITING -> AnyFlowStatus.Connecting
    FilesMapping.FileStatus.SENDING, FilesMapping.FileStatus.RECEIVING ->
        AnyFlowStatus.Transferring
    FilesMapping.FileStatus.SENT, FilesMapping.FileStatus.RECEIVED -> AnyFlowStatus.Success
    FilesMapping.FileStatus.DECLINED, FilesMapping.FileStatus.CANCELLED ->
        AnyFlowStatus.Disconnected
    FilesMapping.FileStatus.TIMED_OUT, FilesMapping.FileStatus.DISCONNECTED ->
        AnyFlowStatus.Disconnected
    FilesMapping.FileStatus.FAILED -> AnyFlowStatus.Error
}
