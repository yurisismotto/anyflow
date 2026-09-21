package io.github.yurisismotto.omnibridge.ui

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
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import io.github.yurisismotto.omnibridge.OmniBridgeApp
import io.github.yurisismotto.omnibridge.R
import io.github.yurisismotto.omnibridge.capability.BatteryCapability
import io.github.yurisismotto.omnibridge.capability.ClipboardCapability
import io.github.yurisismotto.omnibridge.capability.FilesCapability
import io.github.yurisismotto.omnibridge.store.TrustStore
import io.github.yurisismotto.omnibridge.ui.components.OmniBridgeCard
import io.github.yurisismotto.omnibridge.ui.components.OmniBridgeDeviceCard
import io.github.yurisismotto.omnibridge.ui.components.OmniBridgeEmptyState
import io.github.yurisismotto.omnibridge.ui.components.OmniBridgeQuickAction
import io.github.yurisismotto.omnibridge.ui.components.OmniBridgeSecondaryButton
import io.github.yurisismotto.omnibridge.ui.components.OmniBridgeSectionLabel
import io.github.yurisismotto.omnibridge.ui.components.OmniBridgeSecurityNotice
import io.github.yurisismotto.omnibridge.ui.components.OmniBridgeStatusBadge
import io.github.yurisismotto.omnibridge.ui.theme.OmniBridgeIconSize
import io.github.yurisismotto.omnibridge.ui.theme.OmniBridgeSpacing
import io.github.yurisismotto.omnibridge.ui.theme.OmniBridgeStatus
import io.github.yurisismotto.omnibridge.ui.theme.OmniBridgeTheme
import io.github.yurisismotto.omnibridge.ui.theme.OmniBridgeType

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
    val colors = OmniBridgeTheme.colors
    LazyColumn(
        modifier = modifier
            .fillMaxSize()
            .padding(horizontal = OmniBridgeSpacing.md),
        verticalArrangement = Arrangement.spacedBy(OmniBridgeSpacing.sm),
        contentPadding = androidx.compose.foundation.layout.PaddingValues(
            bottom = OmniBridgeSpacing.xxl,
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
                OmniBridgeSectionLabel("Devices")
                Spacer(Modifier.weight(1f))
                IconButton(onClick = actions.onPair) {
                    Icon(
                        painter = painterResource(R.drawable.ic_add),
                        contentDescription = "Pair a new device",
                        tint = colors.accentBlue,
                        modifier = Modifier.size(OmniBridgeIconSize.large),
                    )
                }
            }
        }

        if (state.listedPeers.isEmpty()) {
            item {
                OmniBridgeEmptyState(
                    title = "Connect your first device",
                    subtitle = "Run `omnibridge pair` on your computer, then scan the code " +
                        "it shows. Nothing leaves your network.",
                    actionLabel = "Pair device",
                    onAction = actions.onPair,
                )
            }
        } else {
            items(state.listedPeers, key = { UiMapping.peerKey(it.fingerprint.toHex()) }) { peer ->
                val connected = !peer.revoked && state.isConnected(peer)
                OmniBridgeDeviceCard(
                    name = peer.deviceName,
                    platform = "Desktop · Linux",
                    // A revoked computer says so in a word, in the same badge
                    // every other state uses. Never a colour on its own.
                    status = if (peer.revoked) OmniBridgeStatus.Revoked else state.statusFor(peer),
                    deviceIcon = R.drawable.ic_device_desktop,
                    batteryPercent = if (connected) state.remoteBatteryPercent else null,
                    onClick = { onOpenPeer(peer.fingerprint.toHex()) },
                    // Name and state announced together, in words, for anyone
                    // who cannot see the red badge.
                    stateDescription = if (peer.revoked) {
                        UiMapping.deviceStateDescription(peer)
                    } else {
                        null
                    },
                    footer = {
                        Column(verticalArrangement = Arrangement.spacedBy(OmniBridgeSpacing.sm)) {
                            if (peer.revoked) {
                                // No chips and no Connect. A revoked row is
                                // not a destination, and offering the button
                                // would be offering something that cannot
                                // work. `UiMapping.deviceActions` is where
                                // that rule is stated and tested.
                                Text(
                                    stringResource(R.string.device_revoked_explanation),
                                    style = OmniBridgeType.caption,
                                    color = colors.textSecondary,
                                )
                            } else {
                                CapabilityChips(peer)
                                DeviceConnectAction(peer, state, actions)
                            }
                        }
                    },
                )
            }
        }

        // --- quick actions ------------------------------------------------
        // `peers`, not `listedPeers`: a revoked row is on the list above but
        // is not a destination, so it must not be what makes the quick
        // actions appear either.
        if (state.peers.isNotEmpty()) {
            // The computer the person chose, never `peers.first()`. With
            // several trusted and none chosen there is no target, the tiles
            // are disabled and the line below says why — which is the honest
            // answer, and the one the old code replaced with a guess.
            val target = state.targetPeer
            val connected = target != null && state.isConnected(target)
            item { OmniBridgeSectionLabel("Quick actions") }
            if (target == null) {
                item {
                    Text(
                        "Choose a device above to send to.",
                        style = OmniBridgeType.body,
                        color = colors.textSecondary,
                    )
                }
            }
            item {
                Row(
                    Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.spacedBy(OmniBridgeSpacing.xs),
                ) {
                    OmniBridgeQuickAction(
                        label = "Send clipboard",
                        icon = R.drawable.ic_clipboard,
                        accent = colors.accentTeal,
                        // Disabled rather than hidden: the reason is one tap
                        // away on the device screen, and a row that reflows
                        // whenever a session drops is worse than a grey tile.
                        //
                        // The condition is `UiMapping`'s, not this screen's.
                        // It used to be written inline here and differently on
                        // two other screens, and none of the three asked the
                        // question that mattered — whether the live session
                        // negotiated `clipboard.v1` at all (GitHub #8).
                        enabled = target != null && state.clipboardSendGate(target).ready,
                        onClick = { target?.let { onSendClipboard(it.fingerprint.toHex()) } },
                        modifier = Modifier.weight(1f),
                    )
                    OmniBridgeQuickAction(
                        label = "Send files",
                        icon = R.drawable.ic_files,
                        accent = colors.accentBlue,
                        enabled = connected && target.allows(FilesCapability.ID),
                        onClick = { target?.let { actions.onPickFileFor(it.fingerprint) } },
                        modifier = Modifier.weight(1f),
                    )
                    OmniBridgeQuickAction(
                        label = "Pair device",
                        icon = R.drawable.ic_qr,
                        accent = colors.accentViolet,
                        onClick = actions.onPair,
                        modifier = Modifier.weight(1f),
                    )
                    OmniBridgeQuickAction(
                        label = "Device settings",
                        icon = R.drawable.ic_settings,
                        accent = colors.textSecondary,
                        enabled = target != null,
                        onClick = { target?.let { onOpenPeer(it.fingerprint.toHex()) } },
                        modifier = Modifier.weight(1f),
                    )
                }
            }

            // --- connection ------------------------------------------------
            item { OmniBridgeSectionLabel("Connection") }
            item { ConnectionCard(state, actions) }
        }

        item {
            OmniBridgeSecurityNotice(
                title = "Local network only",
                body = "OmniBridge talks straight to your computer over TLS 1.3, pinned to " +
                    "the key you approved when pairing. No account, no cloud, no relay.",
            )
        }
    }
}

/**
 * Connect to *this* computer, or disconnect from it.
 *
 * ## Why a per-row action exists at all
 *
 * The certified U2 §39.17 defect had two halves. The service picking
 * `peers().firstOrNull()` was one. The other was that there was no way to say
 * otherwise: the only Connect button on this screen took no argument, so with
 * two trusted desktops the app offered no means of choosing between them — the
 * report's words were "there is no in-app way to choose which desktop to use".
 * Fixing the routing without adding a chooser would have left the second
 * desktop just as unreachable, only for a tidier reason.
 *
 * The button therefore carries `peer.fingerprint`, the pinned identity, into
 * [MainActions.onConnect]. It is the same value the TLS handshake is checked
 * against, so what was tapped and what is authenticated cannot drift apart.
 */
@Composable
private fun DeviceConnectAction(
    peer: TrustStore.TrustedPeer,
    state: MainUiState,
    actions: MainActions,
) {
    val connected = state.isConnected(peer)
    val targeted = state.isTarget(peer)
    Row(
        Modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.spacedBy(OmniBridgeSpacing.xs),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (connected) {
            OmniBridgeSecondaryButton(text = "Disconnect", onClick = actions.onDisconnect)
        } else {
            OmniBridgeSecondaryButton(
                text = "Connect",
                // Enabled even when this peer is already the target: a second
                // press is how a person retries, and the service start is
                // idempotent. What it must never be is a press that connects
                // to a different computer.
                onClick = { actions.onConnect(peer.fingerprint) },
            )
        }
        // Stated in words, not by a highlight: which computer the quick
        // actions and the share sheet will use is the fact this whole fix is
        // about, and a colour would leave it unreadable to a screen reader.
        if (targeted && !connected) {
            Text(
                "Selected",
                style = OmniBridgeType.caption,
                color = OmniBridgeTheme.colors.textSecondary,
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
    val colors = OmniBridgeTheme.colors
    Row(horizontalArrangement = Arrangement.spacedBy(OmniBridgeSpacing.md)) {
        Chip("Clipboard", R.drawable.ic_clipboard, peer.allows(ClipboardCapability.ID), colors.accentTeal)
        Chip("Files", R.drawable.ic_files, peer.allows(FilesCapability.ID), colors.accentBlue)
        Chip("Battery", R.drawable.ic_battery, peer.allows(BatteryCapability.ID), colors.accentViolet)
    }
}

@Composable
private fun Chip(label: String, icon: Int, granted: Boolean, accent: Color) {
    val colors = OmniBridgeTheme.colors
    Row(
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(OmniBridgeSpacing.xxs),
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
            modifier = Modifier.size(OmniBridgeIconSize.small),
        )
        Text(
            label,
            style = OmniBridgeType.caption,
            color = if (granted) colors.textSecondary else colors.disabled,
        )
    }
}

/** Connect / disconnect, and what the link is doing right now. */
@Composable
private fun ConnectionCard(state: MainUiState, actions: MainActions) {
    val colors = OmniBridgeTheme.colors
    OmniBridgeCard {
        val status = when (state.connection) {
            is OmniBridgeApp.ConnectionState.Connected -> OmniBridgeStatus.Connected
            is OmniBridgeApp.ConnectionState.Connecting -> OmniBridgeStatus.Connecting
            is OmniBridgeApp.ConnectionState.Retrying -> OmniBridgeStatus.Connecting
            is OmniBridgeApp.ConnectionState.Error -> OmniBridgeStatus.Error
            is OmniBridgeApp.ConnectionState.Idle -> OmniBridgeStatus.Disconnected
        }
        val detail = when (val s = state.connection) {
            is OmniBridgeApp.ConnectionState.Connected -> "Connected to ${s.deviceName}"
            is OmniBridgeApp.ConnectionState.Connecting -> "Connecting…"
            is OmniBridgeApp.ConnectionState.Retrying ->
                "Reconnecting in ${s.inSeconds}s · ${s.reason}"
            is OmniBridgeApp.ConnectionState.Error -> s.message
            is OmniBridgeApp.ConnectionState.Idle -> "Not connected"
        }
        OmniBridgeStatusBadge(status, label = status.label)
        Text(detail, style = OmniBridgeType.body, color = colors.textSecondary)
        if (state.mustChooseTarget) {
            // The one case the old code answered with `first()`. Saying it is
            // the whole point: a disabled button with a reason beats a button
            // that works and dials the wrong desktop.
            Text(
                "Several devices are paired. Use Connect on the one you want.",
                style = OmniBridgeType.body,
                color = colors.textSecondary,
            )
        }
        Row(horizontalArrangement = Arrangement.spacedBy(OmniBridgeSpacing.xs)) {
            val target = state.targetPeer
            OmniBridgeSecondaryButton(
                text = "Connect",
                onClick = { target?.let { actions.onConnect(it.fingerprint) } },
                enabled = target != null &&
                    state.connection !is OmniBridgeApp.ConnectionState.Connected,
            )
            OmniBridgeSecondaryButton(
                text = "Disconnect",
                onClick = actions.onDisconnect,
                enabled = state.connection is OmniBridgeApp.ConnectionState.Connected,
            )
        }
    }
}

/** Maps a peer to the status vocabulary. See [UiMapping] for the rules. */
fun MainUiState.statusFor(peer: TrustStore.TrustedPeer): OmniBridgeStatus =
    UiMapping.statusFor(connection, isConnected(peer), targeted = isTarget(peer))
