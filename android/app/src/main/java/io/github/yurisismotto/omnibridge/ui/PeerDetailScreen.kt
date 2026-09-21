package io.github.yurisismotto.omnibridge.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import io.github.yurisismotto.omnibridge.R
import io.github.yurisismotto.omnibridge.capability.BatteryCapability
import io.github.yurisismotto.omnibridge.capability.ClipboardCapability
import io.github.yurisismotto.omnibridge.capability.FilesCapability
import io.github.yurisismotto.omnibridge.capability.NotificationsCapability
import io.github.yurisismotto.omnibridge.clipboard.ClipboardCapabilities
import io.github.yurisismotto.omnibridge.notifications.NotificationReadiness
import io.github.yurisismotto.omnibridge.ui.components.OmniBridgeCapabilityRow
import io.github.yurisismotto.omnibridge.ui.components.OmniBridgeCard
import io.github.yurisismotto.omnibridge.ui.components.OmniBridgeDestructiveButton
import io.github.yurisismotto.omnibridge.ui.components.OmniBridgeFingerprint
import io.github.yurisismotto.omnibridge.ui.components.OmniBridgePrimaryButton
import io.github.yurisismotto.omnibridge.ui.components.OmniBridgeSectionLabel
import io.github.yurisismotto.omnibridge.ui.components.OmniBridgeSecurityNotice
import io.github.yurisismotto.omnibridge.ui.components.OmniBridgeStatusBadge
import io.github.yurisismotto.omnibridge.ui.components.NoticeTone
import io.github.yurisismotto.omnibridge.ui.theme.OmniBridgeIconSize
import io.github.yurisismotto.omnibridge.ui.theme.OmniBridgeRadius
import io.github.yurisismotto.omnibridge.ui.theme.OmniBridgeSpacing
import io.github.yurisismotto.omnibridge.ui.theme.OmniBridgeStatus
import io.github.yurisismotto.omnibridge.ui.theme.OmniBridgeTheme
import io.github.yurisismotto.omnibridge.ui.theme.OmniBridgeType
import io.github.yurisismotto.omnibridge.ui.theme.MinTouchTarget

/**
 * One computer: who it is, what it may do, and how to stop trusting it.
 *
 * The three groups are kept apart deliberately, because collapsing them is
 * how a person comes to believe sync is running when it is not:
 *
 *  * **Permissions** — whether a capability is granted at all;
 *  * **Automation** — how much happens without being asked;
 *  * **Security** — identity, and withdrawal of trust.
 */
@Composable
fun PeerDetailScreen(
    state: MainUiState,
    actions: MainActions,
    fingerprintHex: String,
    onBack: () -> Unit,
    onSendClipboard: (String) -> Unit,
    onOpenNotifications: (String) -> Unit,
    modifier: Modifier = Modifier,
) {
    val colors = OmniBridgeTheme.colors
    val peer = state.peerByHex(fingerprintHex)
    if (peer == null) {
        // The peer was forgotten while this screen was open.
        Column(modifier.fillMaxSize().padding(OmniBridgeSpacing.md)) {
            Text("This device is no longer paired.", style = OmniBridgeType.body, color = colors.textSecondary)
        }
        return
    }

    // A revoked computer gets its own screen, not the ordinary one with
    // switches greyed out. There is nothing here to configure: the grants are
    // gone, the policies are back at their defaults, and the only thing left
    // to decide is whether the row stays on the list.
    if (peer.revoked) {
        RevokedPeerDetail(peer = peer, actions = actions, onBack = onBack, modifier = modifier)
        return
    }

    val connected = state.isConnected(peer)
    val policy = peer.clipboardPolicy
    val clipboardGranted = peer.allows(ClipboardCapability.ID)
    val notificationsGranted = peer.allows(NotificationsCapability.ID)
    val notificationReadiness = state.notificationGates(peer).readiness()

    Column(
        modifier = modifier
            .fillMaxSize()
            .verticalScroll(rememberScrollState())
            .padding(horizontal = OmniBridgeSpacing.md),
        verticalArrangement = Arrangement.spacedBy(OmniBridgeSpacing.sm),
    ) {
        // --- identity ----------------------------------------------------
        Column(
            Modifier.fillMaxSize().padding(vertical = OmniBridgeSpacing.md),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(OmniBridgeSpacing.xs),
        ) {
            Box(
                Modifier
                    .size(72.dp)
                    .background(colors.accentBlue.copy(alpha = 0.10f), CircleShape),
                contentAlignment = Alignment.Center,
            ) {
                Icon(
                    painter = painterResource(R.drawable.ic_device_desktop),
                    contentDescription = null,
                    tint = colors.accentBlue,
                    modifier = Modifier.size(OmniBridgeIconSize.xlarge),
                )
            }
            Text(peer.deviceName, style = OmniBridgeType.title, color = colors.textPrimary)
            OmniBridgeStatusBadge(state.statusFor(peer))
            Text("Desktop · Linux", style = OmniBridgeType.body, color = colors.textSecondary)
        }

        // --- permissions --------------------------------------------------
        OmniBridgeSectionLabel("Permissions")
        OmniBridgeCard {
            Text(
                "Choose what ${peer.deviceName} can do with this device. " +
                    "Nothing is granted automatically.",
                style = OmniBridgeType.caption,
                color = colors.textSecondary,
            )
            OmniBridgeCapabilityRow(
                title = "Clipboard",
                description = "Send and receive clipboard text",
                icon = R.drawable.ic_clipboard,
                accent = colors.accentTeal,
                checked = clipboardGranted,
                onCheckedChange = { actions.onSetClipboardGrant(peer, it) },
            )
            OmniBridgeCapabilityRow(
                title = "Files",
                description = "Offer and receive files",
                icon = R.drawable.ic_files,
                accent = colors.accentBlue,
                checked = peer.allows(FilesCapability.ID),
                onCheckedChange = { actions.onSetFilesGrant(peer, it) },
            )
            OmniBridgeCapabilityRow(
                title = "Battery",
                description = "Share this device's battery level",
                icon = R.drawable.ic_battery,
                accent = colors.accentViolet,
                checked = peer.allows(BatteryCapability.ID),
                onCheckedChange = { actions.onSetBatteryGrant(peer, it) },
            )
            // `notifications.v1` is not a switch here, and that is deliberate.
            // The other three grants are one decision each; this one is three
            // — Android's own notification access, this computer's grant, and
            // which applications — and they fail separately. A switch on this
            // card would have to claim one of them stood for all three.
            OmniBridgeNotificationsEntry(
                readiness = notificationReadiness,
                granted = notificationsGranted,
                appCount = peer.notificationPolicy.allowedApps.size,
                onClick = { onOpenNotifications(peer.fingerprint.toHex()) },
            )
        }

        // --- clipboard direction and automation ----------------------------
        OmniBridgeSectionLabel("Clipboard")
        OmniBridgeCard {
            if (!clipboardGranted) {
                Text(
                    "Clipboard is off. ${peer.deviceName} cannot send or receive " +
                        "clipboard text.",
                    style = OmniBridgeType.body,
                    color = colors.textSecondary,
                )
            } else {
                OmniBridgeCapabilityRow(
                    title = "Receive clipboard",
                    description = "Accept clipboard text from ${peer.deviceName}",
                    icon = R.drawable.ic_receive,
                    accent = colors.accentTeal,
                    checked = policy.allowReceive,
                    onCheckedChange = {
                        actions.onSetClipboardPolicy(peer, policy.copy(allowReceive = it))
                    },
                )
                OmniBridgeCapabilityRow(
                    title = "Apply automatically",
                    description = if (policy.mayAutoReceive()) {
                        "Received text replaces your clipboard as it arrives"
                    } else {
                        "Received text waits in a notification until you tap Copy"
                    },
                    icon = R.drawable.ic_download,
                    accent = colors.accentTeal,
                    checked = policy.autoReceive,
                    enabled = policy.allowReceive,
                    onCheckedChange = {
                        actions.onSetClipboardPolicy(peer, policy.copy(autoReceive = it))
                    },
                )
                OmniBridgeCapabilityRow(
                    title = "Send clipboard",
                    description = "Allow sending this device's clipboard",
                    icon = R.drawable.ic_send,
                    accent = colors.accentBlue,
                    checked = policy.allowSend,
                    onCheckedChange = {
                        actions.onSetClipboardPolicy(peer, policy.copy(allowSend = it))
                    },
                )

                // No auto-send toggle, and the reason is stated rather than
                // hidden. Offering a switch that cannot work would be worse
                // than not offering one: the person would turn it on and
                // quietly get nothing.
                if (!ClipboardCapabilities.AUTO_SEND_SUPPORTED) {
                    Spacer(Modifier.height(OmniBridgeSpacing.xxs))
                    OmniBridgeSecurityNotice(
                        title = "Automatic sending is not possible on Android",
                        body = ClipboardCapabilities.AUTO_SEND_REASON,
                        tone = NoticeTone.Info,
                        icon = R.drawable.ic_shield,
                    )
                }

                Spacer(Modifier.height(OmniBridgeSpacing.xxs))
                val clipboardGate = state.clipboardSendGate(peer)
                OmniBridgePrimaryButton(
                    text = "Send clipboard",
                    icon = R.drawable.ic_send,
                    enabled = clipboardGate.ready,
                    onClick = { onSendClipboard(peer.fingerprint.toHex()) },
                )
                // The reason the button is grey, in the one place a person is
                // looking when they wonder. It used to say "Connect to X" for
                // every cause, including a session that was up and simply had
                // not negotiated the clipboard.
                clipboardGate.reasonOrNull?.let { reason ->
                    Text(
                        reason,
                        style = OmniBridgeType.caption,
                        color = colors.textMuted,
                    )
                }
            }
        }

        // --- security -----------------------------------------------------
        OmniBridgeSectionLabel("Security")
        OmniBridgeCard {
            Text("Device fingerprint", style = OmniBridgeType.label, color = colors.textSecondary)
            // Never abbreviated for balance. This is the string a person
            // compares against the computer's screen, and it is the whole
            // reason pairing is safe.
            OmniBridgeFingerprint(peer.fingerprint.toDisplayShort())
            Spacer(Modifier.height(OmniBridgeSpacing.xxs))
            Text(
                "Paired directly over your local network and pinned to this key. " +
                    "A different key cannot impersonate it.",
                style = OmniBridgeType.caption,
                color = colors.textSecondary,
            )
        }

        // Set apart from everything else, and never beside a confirming
        // button: revoking a device ends the pairing and costs a QR scan to
        // undo.
        Spacer(Modifier.height(OmniBridgeSpacing.md))
        var confirmRevoke by remember { mutableStateOf(false) }
        OmniBridgeDestructiveButton(
            text = stringResource(R.string.device_revoke_action),
            icon = R.drawable.ic_shield_off,
            onClick = { confirmRevoke = true },
        )
        if (confirmRevoke) {
            ConfirmDialog(
                title = stringResource(R.string.device_revoke_confirm_title, peer.deviceName),
                body = stringResource(R.string.device_revoke_confirm_body),
                confirmLabel = stringResource(R.string.device_revoke_confirm_button),
                onDismiss = { confirmRevoke = false },
                onConfirm = {
                    confirmRevoke = false
                    actions.onRevoke(peer)
                    onBack()
                },
            )
        }
        Spacer(Modifier.height(OmniBridgeSpacing.xxl))
    }
}

/**
 * One revoked computer: who it was, and the one thing left to do about it.
 *
 * Deliberately not the ordinary screen with everything disabled. A row of
 * greyed-out switches invites the question "can I turn these back on?", and
 * the answer — pair it again from a fresh code — is not something a switch
 * can say. What is here instead is the state in words, the fingerprint that
 * is still pinned against it, and "Remove from list".
 */
@Composable
private fun RevokedPeerDetail(
    peer: io.github.yurisismotto.omnibridge.store.TrustStore.TrustedPeer,
    actions: MainActions,
    onBack: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val colors = OmniBridgeTheme.colors
    var confirmRemove by remember { mutableStateOf(false) }

    Column(
        modifier = modifier
            .fillMaxSize()
            .verticalScroll(rememberScrollState())
            .padding(horizontal = OmniBridgeSpacing.md),
        verticalArrangement = Arrangement.spacedBy(OmniBridgeSpacing.sm),
    ) {
        Column(
            Modifier.fillMaxSize().padding(vertical = OmniBridgeSpacing.md),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(OmniBridgeSpacing.xs),
        ) {
            Box(
                Modifier
                    .size(72.dp)
                    .background(colors.accentRed.copy(alpha = 0.10f), CircleShape),
                contentAlignment = Alignment.Center,
            ) {
                Icon(
                    painter = painterResource(R.drawable.ic_shield_off),
                    contentDescription = null,
                    tint = colors.accentRed,
                    modifier = Modifier.size(OmniBridgeIconSize.xlarge),
                )
            }
            Text(peer.deviceName, style = OmniBridgeType.title, color = colors.textPrimary)
            OmniBridgeStatusBadge(OmniBridgeStatus.Revoked)
            Text(
                stringResource(R.string.device_revoked_explanation),
                style = OmniBridgeType.body,
                color = colors.textSecondary,
            )
        }

        OmniBridgeSectionLabel("Security")
        OmniBridgeCard {
            Text("Device fingerprint", style = OmniBridgeType.label, color = colors.textSecondary)
            OmniBridgeFingerprint(peer.fingerprint.toDisplayShort())
            Spacer(Modifier.height(OmniBridgeSpacing.xxs))
            Text(
                stringResource(R.string.device_revoked_pin_note),
                style = OmniBridgeType.caption,
                color = colors.textSecondary,
            )
        }

        Spacer(Modifier.height(OmniBridgeSpacing.md))
        OmniBridgeDestructiveButton(
            text = stringResource(R.string.device_remove_from_list_action),
            icon = R.drawable.ic_trash,
            onClick = { confirmRemove = true },
        )
        if (confirmRemove) {
            ConfirmDialog(
                title = stringResource(
                    R.string.device_remove_from_list_confirm_title,
                    peer.deviceName,
                ),
                body = stringResource(R.string.device_remove_from_list_confirm_body),
                confirmLabel = stringResource(R.string.device_remove_from_list_confirm_button),
                onDismiss = { confirmRemove = false },
                onConfirm = {
                    confirmRemove = false
                    actions.onRemoveFromList(peer)
                    onBack()
                },
            )
        }
        Spacer(Modifier.height(OmniBridgeSpacing.xxl))
    }
}

/**
 * The confirmation in front of anything destructive.
 *
 * Material's own dialog, so it is focusable, dismissable and announced the way
 * every other dialog on the device is. Cancel is the dismiss action *and* the
 * one a tap outside gives, which is the platform convention and means the safe
 * answer is what a mistimed tap produces.
 */
@Composable
private fun ConfirmDialog(
    title: String,
    body: String,
    confirmLabel: String,
    onDismiss: () -> Unit,
    onConfirm: () -> Unit,
) {
    val colors = OmniBridgeTheme.colors
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(title, style = OmniBridgeType.subtitle) },
        text = { Text(body, style = OmniBridgeType.body, color = colors.textSecondary) },
        confirmButton = {
            TextButton(onClick = onConfirm) {
                Text(confirmLabel, color = colors.accentRed)
            }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) {
                Text(stringResource(R.string.action_cancel), color = colors.textSecondary)
            }
        },
    )
}

/**
 * The notifications row on the device card: a state, and a way in.
 *
 * It shows the *resolved* state rather than a grant flag, because "on" and
 * "actually mirroring" are not the same thing and the gap between them is
 * where every confusing report about this feature comes from. A computer that
 * has been granted but has no application chosen reads "No apps chosen", not
 * "On" — and it is sharing exactly nothing, which is what the words say.
 */
@Composable
private fun OmniBridgeNotificationsEntry(
    readiness: NotificationReadiness,
    granted: Boolean,
    appCount: Int,
    onClick: () -> Unit,
) {
    val colors = OmniBridgeTheme.colors
    val title = stringResource(R.string.notif_capability_title)
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .heightIn(min = MinTouchTarget)
            .clip(RoundedCornerShape(OmniBridgeRadius.medium))
            .clickable(role = Role.Button, onClick = onClick)
            .semantics(mergeDescendants = true) { contentDescription = title }
            .padding(vertical = OmniBridgeSpacing.xs),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(
            painter = painterResource(R.drawable.ic_notifications),
            contentDescription = null,
            tint = colors.accentViolet,
            modifier = Modifier.size(OmniBridgeIconSize.large),
        )
        Spacer(Modifier.width(OmniBridgeSpacing.sm))
        Column(Modifier.weight(1f)) {
            Text(title, style = OmniBridgeType.body, color = colors.textPrimary)
            Text(
                if (granted && appCount > 0) {
                    stringResource(R.string.notif_apps_count, appCount, appCount)
                } else {
                    stringResource(R.string.notif_capability_description)
                },
                style = OmniBridgeType.caption,
                color = colors.textSecondary,
            )
        }
        OmniBridgeStatusBadge(
            status = NotificationUiMapping.status(readiness),
            label = stringResource(NotificationUiMapping.statusLabel(readiness)),
            showIcon = false,
        )
        Spacer(Modifier.width(OmniBridgeSpacing.xxs))
        Icon(
            painter = painterResource(R.drawable.ic_chevron_right),
            contentDescription = null,
            tint = colors.textMuted,
            modifier = Modifier.size(OmniBridgeIconSize.large),
        )
    }
}
