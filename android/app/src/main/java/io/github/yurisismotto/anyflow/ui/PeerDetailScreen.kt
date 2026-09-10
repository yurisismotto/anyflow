package io.github.yurisismotto.anyflow.ui

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
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import io.github.yurisismotto.anyflow.R
import io.github.yurisismotto.anyflow.capability.BatteryCapability
import io.github.yurisismotto.anyflow.capability.ClipboardCapability
import io.github.yurisismotto.anyflow.capability.FilesCapability
import io.github.yurisismotto.anyflow.capability.NotificationsCapability
import io.github.yurisismotto.anyflow.clipboard.ClipboardCapabilities
import io.github.yurisismotto.anyflow.notifications.NotificationReadiness
import io.github.yurisismotto.anyflow.ui.components.AnyFlowCapabilityRow
import io.github.yurisismotto.anyflow.ui.components.AnyFlowCard
import io.github.yurisismotto.anyflow.ui.components.AnyFlowDestructiveButton
import io.github.yurisismotto.anyflow.ui.components.AnyFlowFingerprint
import io.github.yurisismotto.anyflow.ui.components.AnyFlowPrimaryButton
import io.github.yurisismotto.anyflow.ui.components.AnyFlowSectionLabel
import io.github.yurisismotto.anyflow.ui.components.AnyFlowSecurityNotice
import io.github.yurisismotto.anyflow.ui.components.AnyFlowStatusBadge
import io.github.yurisismotto.anyflow.ui.components.NoticeTone
import io.github.yurisismotto.anyflow.ui.theme.AnyFlowIconSize
import io.github.yurisismotto.anyflow.ui.theme.AnyFlowRadius
import io.github.yurisismotto.anyflow.ui.theme.AnyFlowSpacing
import io.github.yurisismotto.anyflow.ui.theme.AnyFlowTheme
import io.github.yurisismotto.anyflow.ui.theme.AnyFlowType
import io.github.yurisismotto.anyflow.ui.theme.MinTouchTarget

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
    val colors = AnyFlowTheme.colors
    val peer = state.peerByHex(fingerprintHex)
    if (peer == null) {
        // The peer was forgotten while this screen was open.
        Column(modifier.fillMaxSize().padding(AnyFlowSpacing.md)) {
            Text("This device is no longer paired.", style = AnyFlowType.body, color = colors.textSecondary)
        }
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
            .padding(horizontal = AnyFlowSpacing.md),
        verticalArrangement = Arrangement.spacedBy(AnyFlowSpacing.sm),
    ) {
        // --- identity ----------------------------------------------------
        Column(
            Modifier.fillMaxSize().padding(vertical = AnyFlowSpacing.md),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(AnyFlowSpacing.xs),
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
                    modifier = Modifier.size(AnyFlowIconSize.xlarge),
                )
            }
            Text(peer.deviceName, style = AnyFlowType.title, color = colors.textPrimary)
            AnyFlowStatusBadge(state.statusFor(peer))
            Text("Desktop · Linux", style = AnyFlowType.body, color = colors.textSecondary)
        }

        // --- permissions --------------------------------------------------
        AnyFlowSectionLabel("Permissions")
        AnyFlowCard {
            Text(
                "Choose what ${peer.deviceName} can do with this device. " +
                    "Nothing is granted automatically.",
                style = AnyFlowType.caption,
                color = colors.textSecondary,
            )
            AnyFlowCapabilityRow(
                title = "Clipboard",
                description = "Send and receive clipboard text",
                icon = R.drawable.ic_clipboard,
                accent = colors.accentTeal,
                checked = clipboardGranted,
                onCheckedChange = { actions.onSetClipboardGrant(peer, it) },
            )
            AnyFlowCapabilityRow(
                title = "Files",
                description = "Offer and receive files",
                icon = R.drawable.ic_files,
                accent = colors.accentBlue,
                checked = peer.allows(FilesCapability.ID),
                onCheckedChange = { actions.onSetFilesGrant(peer, it) },
            )
            AnyFlowCapabilityRow(
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
            AnyFlowNotificationsEntry(
                readiness = notificationReadiness,
                granted = notificationsGranted,
                appCount = peer.notificationPolicy.allowedApps.size,
                onClick = { onOpenNotifications(peer.fingerprint.toHex()) },
            )
        }

        // --- clipboard direction and automation ----------------------------
        AnyFlowSectionLabel("Clipboard")
        AnyFlowCard {
            if (!clipboardGranted) {
                Text(
                    "Clipboard is off. ${peer.deviceName} cannot send or receive " +
                        "clipboard text.",
                    style = AnyFlowType.body,
                    color = colors.textSecondary,
                )
            } else {
                AnyFlowCapabilityRow(
                    title = "Receive clipboard",
                    description = "Accept clipboard text from ${peer.deviceName}",
                    icon = R.drawable.ic_receive,
                    accent = colors.accentTeal,
                    checked = policy.allowReceive,
                    onCheckedChange = {
                        actions.onSetClipboardPolicy(peer, policy.copy(allowReceive = it))
                    },
                )
                AnyFlowCapabilityRow(
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
                AnyFlowCapabilityRow(
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
                    Spacer(Modifier.height(AnyFlowSpacing.xxs))
                    AnyFlowSecurityNotice(
                        title = "Automatic sending is not possible on Android",
                        body = ClipboardCapabilities.AUTO_SEND_REASON,
                        tone = NoticeTone.Info,
                        icon = R.drawable.ic_shield,
                    )
                }

                Spacer(Modifier.height(AnyFlowSpacing.xxs))
                AnyFlowPrimaryButton(
                    text = "Send clipboard",
                    icon = R.drawable.ic_send,
                    enabled = connected && policy.allowSend,
                    onClick = { onSendClipboard(peer.fingerprint.toHex()) },
                )
                if (!connected) {
                    Text(
                        "Connect to ${peer.deviceName} to send your clipboard.",
                        style = AnyFlowType.caption,
                        color = colors.textMuted,
                    )
                }
            }
        }

        // --- security -----------------------------------------------------
        AnyFlowSectionLabel("Security")
        AnyFlowCard {
            Text("Device fingerprint", style = AnyFlowType.label, color = colors.textSecondary)
            // Never abbreviated for balance. This is the string a person
            // compares against the computer's screen, and it is the whole
            // reason pairing is safe.
            AnyFlowFingerprint(peer.fingerprint.toDisplayShort())
            Spacer(Modifier.height(AnyFlowSpacing.xxs))
            Text(
                "Paired directly over your local network and pinned to this key. " +
                    "A different key cannot impersonate it.",
                style = AnyFlowType.caption,
                color = colors.textSecondary,
            )
        }

        // Set apart from everything else, and never beside a confirming
        // button: forgetting a device destroys the pairing and costs a QR
        // scan to undo.
        Spacer(Modifier.height(AnyFlowSpacing.md))
        AnyFlowDestructiveButton(
            text = "Forget this device",
            icon = R.drawable.ic_trash,
            onClick = {
                actions.onForget(peer)
                onBack()
            },
        )
        Spacer(Modifier.height(AnyFlowSpacing.xxl))
    }
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
private fun AnyFlowNotificationsEntry(
    readiness: NotificationReadiness,
    granted: Boolean,
    appCount: Int,
    onClick: () -> Unit,
) {
    val colors = AnyFlowTheme.colors
    val title = stringResource(R.string.notif_capability_title)
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .heightIn(min = MinTouchTarget)
            .clip(RoundedCornerShape(AnyFlowRadius.medium))
            .clickable(role = Role.Button, onClick = onClick)
            .semantics(mergeDescendants = true) { contentDescription = title }
            .padding(vertical = AnyFlowSpacing.xs),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(
            painter = painterResource(R.drawable.ic_notifications),
            contentDescription = null,
            tint = colors.accentViolet,
            modifier = Modifier.size(AnyFlowIconSize.large),
        )
        Spacer(Modifier.width(AnyFlowSpacing.sm))
        Column(Modifier.weight(1f)) {
            Text(title, style = AnyFlowType.body, color = colors.textPrimary)
            Text(
                if (granted && appCount > 0) {
                    stringResource(R.string.notif_apps_count, appCount, appCount)
                } else {
                    stringResource(R.string.notif_capability_description)
                },
                style = AnyFlowType.caption,
                color = colors.textSecondary,
            )
        }
        AnyFlowStatusBadge(
            status = NotificationUiMapping.status(readiness),
            label = stringResource(NotificationUiMapping.statusLabel(readiness)),
            showIcon = false,
        )
        Spacer(Modifier.width(AnyFlowSpacing.xxs))
        Icon(
            painter = painterResource(R.drawable.ic_chevron_right),
            contentDescription = null,
            tint = colors.textMuted,
            modifier = Modifier.size(AnyFlowIconSize.large),
        )
    }
}
