package io.github.yurisismotto.anyflow.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import io.github.yurisismotto.anyflow.AnyFlowApp
import io.github.yurisismotto.anyflow.R
import io.github.yurisismotto.anyflow.capability.BatteryCapability
import io.github.yurisismotto.anyflow.capability.ClipboardCapability
import io.github.yurisismotto.anyflow.capability.FilesCapability
import io.github.yurisismotto.anyflow.store.TrustStore
import io.github.yurisismotto.anyflow.ui.components.AnyFlowCard
import io.github.yurisismotto.anyflow.ui.components.AnyFlowDeviceCard
import io.github.yurisismotto.anyflow.ui.components.AnyFlowEmptyState
import io.github.yurisismotto.anyflow.ui.components.AnyFlowQuickAction
import io.github.yurisismotto.anyflow.ui.components.AnyFlowSecondaryButton
import io.github.yurisismotto.anyflow.ui.components.AnyFlowSectionLabel
import io.github.yurisismotto.anyflow.ui.components.AnyFlowSecurityNotice
import io.github.yurisismotto.anyflow.ui.components.AnyFlowStatusBadge
import io.github.yurisismotto.anyflow.ui.theme.AnyFlowIconSize
import io.github.yurisismotto.anyflow.ui.theme.AnyFlowSpacing
import io.github.yurisismotto.anyflow.ui.theme.AnyFlowStatus
import io.github.yurisismotto.anyflow.ui.theme.AnyFlowTheme
import io.github.yurisismotto.anyflow.ui.theme.AnyFlowType

/**
 * Home: the computers this phone knows, and what can be done with them now.
 *
 * Anything waiting on a decision — an incoming file, a held clip — is placed
 * above the device list, because it is the only thing on this screen with a
 * deadline.
 */
@Composable
fun DevicesScreen(
    state: MainUiState,
    actions: MainActions,
    onOpenPeer: (String) -> Unit,
    onSendClipboard: (String) -> Unit,
    modifier: Modifier = Modifier,
) {
    val colors = AnyFlowTheme.colors
    LazyColumn(
        modifier = modifier
            .fillMaxSize()
            .padding(horizontal = AnyFlowSpacing.md),
        verticalArrangement = Arrangement.spacedBy(AnyFlowSpacing.sm),
        contentPadding = androidx.compose.foundation.layout.PaddingValues(
            bottom = AnyFlowSpacing.xxl,
        ),
    ) {
        // --- things waiting on the person -------------------------------
        items(state.offers, key = { UiMapping.offerKey(it.transferId) }) { offer ->
            IncomingOfferCard(
                offer = offer,
                onRespond = { accept -> actions.onRespondToOffer(offer.transferId, accept) },
            )
        }
        items(state.pendingClips, key = { UiMapping.clipKey(it.peer.toHex()) }) { clip ->
            PendingClipCard(
                clip = clip,
                onApply = { actions.onApplyClip(clip.peer) },
                onDismiss = { actions.onDismissClip(clip.peer) },
            )
        }

        // --- devices -----------------------------------------------------
        item {
            Row(verticalAlignment = Alignment.CenterVertically) {
                AnyFlowSectionLabel("Devices")
                Spacer(Modifier.weight(1f))
                IconButton(onClick = actions.onPair) {
                    Icon(
                        painter = painterResource(R.drawable.ic_add),
                        contentDescription = "Pair a new device",
                        tint = colors.accentBlue,
                        modifier = Modifier.size(AnyFlowIconSize.large),
                    )
                }
            }
        }

        if (state.peers.isEmpty()) {
            item {
                AnyFlowEmptyState(
                    title = "Connect your first device",
                    subtitle = "Run `anyflow pair` on your computer, then scan the code " +
                        "it shows. Nothing leaves your network.",
                    actionLabel = "Pair device",
                    onAction = actions.onPair,
                )
            }
        } else {
            items(state.peers, key = { UiMapping.peerKey(it.fingerprint.toHex()) }) { peer ->
                val connected = state.isConnected(peer)
                AnyFlowDeviceCard(
                    name = peer.deviceName,
                    platform = "Desktop · Linux",
                    status = state.statusFor(peer),
                    deviceIcon = R.drawable.ic_device_desktop,
                    batteryPercent = if (connected) state.remoteBatteryPercent else null,
                    onClick = { onOpenPeer(peer.fingerprint.toHex()) },
                    footer = { CapabilityChips(peer) },
                )
            }
        }

        // --- quick actions ------------------------------------------------
        if (state.peers.isNotEmpty()) {
            val target = state.peers.firstOrNull { state.isConnected(it) } ?: state.peers.first()
            val connected = state.isConnected(target)
            item { AnyFlowSectionLabel("Quick actions") }
            item {
                Row(
                    Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.spacedBy(AnyFlowSpacing.xs),
                ) {
                    AnyFlowQuickAction(
                        label = "Send clipboard",
                        icon = R.drawable.ic_clipboard,
                        accent = colors.accentTeal,
                        // Disabled rather than hidden: the reason is one tap
                        // away on the device screen, and a row that reflows
                        // whenever a session drops is worse than a grey tile.
                        enabled = connected && target.allows(ClipboardCapability.ID) &&
                            target.clipboardPolicy.allowSend,
                        onClick = { onSendClipboard(target.fingerprint.toHex()) },
                        modifier = Modifier.weight(1f),
                    )
                    AnyFlowQuickAction(
                        label = "Send files",
                        icon = R.drawable.ic_files,
                        accent = colors.accentBlue,
                        enabled = connected && target.allows(FilesCapability.ID),
                        onClick = { actions.onPickFileFor(target.fingerprint) },
                        modifier = Modifier.weight(1f),
                    )
                    AnyFlowQuickAction(
                        label = "Pair device",
                        icon = R.drawable.ic_qr,
                        accent = colors.accentViolet,
                        onClick = actions.onPair,
                        modifier = Modifier.weight(1f),
                    )
                    AnyFlowQuickAction(
                        label = "Device settings",
                        icon = R.drawable.ic_settings,
                        accent = colors.textSecondary,
                        onClick = { onOpenPeer(target.fingerprint.toHex()) },
                        modifier = Modifier.weight(1f),
                    )
                }
            }

            // --- connection ------------------------------------------------
            item { AnyFlowSectionLabel("Connection") }
            item { ConnectionCard(state, actions) }
        }

        item {
            AnyFlowSecurityNotice(
                title = "Local network only",
                body = "AnyFlow talks straight to your computer over TLS 1.3, pinned to " +
                    "the key you approved when pairing. No account, no cloud, no relay.",
            )
        }
    }
}

/**
 * What this computer is allowed to do, as a row of small chips.
 *
 * Grants only — never a live state. A chip says "Clipboard" because the
 * capability is granted, not because something is syncing right now.
 */
@Composable
private fun CapabilityChips(peer: TrustStore.TrustedPeer) {
    val colors = AnyFlowTheme.colors
    Row(horizontalArrangement = Arrangement.spacedBy(AnyFlowSpacing.md)) {
        Chip("Clipboard", R.drawable.ic_clipboard, peer.allows(ClipboardCapability.ID), colors.accentTeal)
        Chip("Files", R.drawable.ic_files, peer.allows(FilesCapability.ID), colors.accentBlue)
        Chip("Battery", R.drawable.ic_battery, peer.allows(BatteryCapability.ID), colors.accentViolet)
    }
}

@Composable
private fun Chip(label: String, icon: Int, granted: Boolean, accent: Color) {
    val colors = AnyFlowTheme.colors
    Row(
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(AnyFlowSpacing.xxs),
        // Never colour alone: the chip states its grant in words for anyone
        // who cannot tell the tinted icon from the grey one.
        modifier = Modifier.semantics(mergeDescendants = true) {
            contentDescription = "$label, ${if (granted) "allowed" else "not allowed"}"
        },
    ) {
        Icon(
            painter = painterResource(icon),
            contentDescription = null,
            tint = if (granted) accent else colors.disabled,
            modifier = Modifier.size(AnyFlowIconSize.small),
        )
        Text(
            label,
            style = AnyFlowType.caption,
            color = if (granted) colors.textSecondary else colors.disabled,
        )
    }
}

/** Connect / disconnect, and what the link is doing right now. */
@Composable
private fun ConnectionCard(state: MainUiState, actions: MainActions) {
    val colors = AnyFlowTheme.colors
    AnyFlowCard {
        val status = when (state.connection) {
            is AnyFlowApp.ConnectionState.Connected -> AnyFlowStatus.Connected
            is AnyFlowApp.ConnectionState.Connecting -> AnyFlowStatus.Connecting
            is AnyFlowApp.ConnectionState.Retrying -> AnyFlowStatus.Connecting
            is AnyFlowApp.ConnectionState.Error -> AnyFlowStatus.Error
            is AnyFlowApp.ConnectionState.Idle -> AnyFlowStatus.Disconnected
        }
        val detail = when (val s = state.connection) {
            is AnyFlowApp.ConnectionState.Connected -> "Connected to ${s.deviceName}"
            is AnyFlowApp.ConnectionState.Connecting -> "Connecting…"
            is AnyFlowApp.ConnectionState.Retrying ->
                "Reconnecting in ${s.inSeconds}s · ${s.reason}"
            is AnyFlowApp.ConnectionState.Error -> s.message
            is AnyFlowApp.ConnectionState.Idle -> "Not connected"
        }
        AnyFlowStatusBadge(status, label = status.label)
        Text(detail, style = AnyFlowType.body, color = colors.textSecondary)
        Row(horizontalArrangement = Arrangement.spacedBy(AnyFlowSpacing.xs)) {
            AnyFlowSecondaryButton(
                text = "Connect",
                onClick = actions.onConnect,
                enabled = state.connection !is AnyFlowApp.ConnectionState.Connected,
            )
            AnyFlowSecondaryButton(
                text = "Disconnect",
                onClick = actions.onDisconnect,
                enabled = state.connection is AnyFlowApp.ConnectionState.Connected,
            )
        }
    }
}

/** Maps a peer to the status vocabulary. See [UiMapping] for the rules. */
fun MainUiState.statusFor(peer: TrustStore.TrustedPeer): AnyFlowStatus =
    UiMapping.statusFor(connection, isConnected(peer))
