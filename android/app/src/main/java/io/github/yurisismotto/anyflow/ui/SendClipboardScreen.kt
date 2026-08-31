package io.github.yurisismotto.anyflow.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
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
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import io.github.yurisismotto.anyflow.R
import io.github.yurisismotto.anyflow.ui.components.AnyFlowCard
import io.github.yurisismotto.anyflow.ui.components.AnyFlowPrimaryButton
import io.github.yurisismotto.anyflow.ui.components.AnyFlowSecurityNotice
import io.github.yurisismotto.anyflow.ui.components.AnyFlowStatusBadge
import io.github.yurisismotto.anyflow.ui.components.AnyFlowTextButton
import io.github.yurisismotto.anyflow.ui.components.NoticeTone
import io.github.yurisismotto.anyflow.ui.components.decorative
import io.github.yurisismotto.anyflow.ui.theme.AnyFlowGradient
import io.github.yurisismotto.anyflow.ui.theme.AnyFlowIconSize
import io.github.yurisismotto.anyflow.ui.theme.AnyFlowRadius
import io.github.yurisismotto.anyflow.ui.theme.AnyFlowSpacing
import io.github.yurisismotto.anyflow.ui.theme.AnyFlowTheme
import io.github.yurisismotto.anyflow.ui.theme.AnyFlowType

/**
 * The deliberate act of sending the clipboard to one computer.
 *
 * ## About the preview
 *
 * Ordinary text is shown, because seeing it is how a person notices they are
 * about to send the wrong thing. Text the source app marked sensitive is
 * **not** shown — `EXTRA_IS_SENSITIVE` exists precisely so that surfaces like
 * this one do not render a password into a shoulder-surfable box, and the
 * size and destination are enough to decide with.
 *
 * Neither case is logged or persisted. The preview lives in composition and
 * dies with the screen. See [ClipboardPreview].
 */
@Composable
fun SendClipboardScreen(
    state: MainUiState,
    actions: MainActions,
    fingerprintHex: String,
    onBack: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val colors = AnyFlowTheme.colors
    val peer = state.peerByHex(fingerprintHex)
    if (peer == null) {
        Column(modifier.fillMaxSize().padding(AnyFlowSpacing.md)) {
            Text(
                "This device is no longer paired.",
                style = AnyFlowType.body,
                color = colors.textSecondary,
            )
        }
        return
    }

    // Read once, when the screen opens. The Activity has focus here, which is
    // the only state in which Android permits a clipboard read at all.
    val preview = remember(fingerprintHex) { actions.readClipboardPreview() }
    val connected = state.isConnected(peer)

    Column(
        modifier = modifier
            .fillMaxSize()
            .verticalScroll(rememberScrollState())
            .padding(horizontal = AnyFlowSpacing.md),
        verticalArrangement = Arrangement.spacedBy(AnyFlowSpacing.sm),
    ) {
        FlowHero(peerName = peer.deviceName)

        Column(horizontalAlignment = Alignment.CenterHorizontally, modifier = Modifier.fillMaxWidth()) {
            AnyFlowStatusBadge(
                state.statusFor(peer),
                label = if (connected) "Connected to ${peer.deviceName}" else peer.deviceName,
            )
            Text("Desktop · Linux", style = AnyFlowType.caption, color = colors.textSecondary)
        }

        Spacer(Modifier.height(AnyFlowSpacing.xs))

        when {
            preview == null -> AnyFlowCard {
                Text("Clipboard", style = AnyFlowType.label, color = colors.textSecondary)
                Text(
                    "There is nothing to send. Copy some text, then come back.",
                    style = AnyFlowType.body,
                    color = colors.textSecondary,
                )
            }

            else -> {
                AnyFlowCard {
                    Text(
                        "Clipboard preview",
                        style = AnyFlowType.label,
                        color = colors.textSecondary,
                    )
                    Box(
                        Modifier
                            .fillMaxWidth()
                            .clip(RoundedCornerShape(AnyFlowRadius.small))
                            .background(colors.surfaceSunken)
                            .padding(AnyFlowSpacing.sm),
                    ) {
                        Text(
                            text = preview.text ?: "•".repeat(12),
                            style = AnyFlowType.mono,
                            color = if (preview.text != null) {
                                colors.textPrimary
                            } else {
                                colors.textMuted
                            },
                            maxLines = 4,
                            overflow = TextOverflow.Ellipsis,
                        )
                    }
                    Text(
                        "${preview.bytes} bytes",
                        style = AnyFlowType.caption,
                        color = colors.textMuted,
                    )
                }

                if (preview.sensitive) {
                    AnyFlowSecurityNotice(
                        title = "This clipboard is marked sensitive",
                        body = "The app you copied from marked this text as sensitive — " +
                            "a password, a recovery code, or similar. It is hidden here " +
                            "on purpose. Send it only if you meant to.",
                        tone = NoticeTone.Caution,
                        icon = R.drawable.ic_warning,
                    )
                }
            }
        }

        Spacer(Modifier.height(AnyFlowSpacing.xs))

        AnyFlowPrimaryButton(
            text = "Send to ${peer.deviceName}",
            icon = R.drawable.ic_send,
            enabled = connected && preview != null,
            onClick = {
                actions.onSendClipboard(peer.fingerprint)
                onBack()
            },
        )
        Box(Modifier.fillMaxWidth(), contentAlignment = Alignment.Center) {
            AnyFlowTextButton("Cancel", onBack)
        }

        // Deliberately not "end-to-end encrypted". The link is a direct,
        // mutually authenticated TLS 1.3 session pinned to the key approved
        // at pairing — which is what this says, in the project's own terms.
        // Reaching for a marketing phrase whose meaning does not exactly
        // match is how a security claim becomes untrue.
        Row(
            Modifier.fillMaxWidth().padding(vertical = AnyFlowSpacing.sm),
            horizontalArrangement = Arrangement.Center,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Icon(
                painter = painterResource(R.drawable.ic_shield_check),
                contentDescription = null,
                tint = colors.textMuted,
                modifier = Modifier.size(AnyFlowIconSize.small),
            )
            Spacer(Modifier.width(AnyFlowSpacing.xxs))
            Text(
                "Direct connection · TLS 1.3, pinned · local network",
                style = AnyFlowType.caption,
                color = colors.textMuted,
                textAlign = TextAlign.Center,
            )
        }
        Spacer(Modifier.height(AnyFlowSpacing.xxl))
    }
}

/**
 * Clipboard, ribbon, destination — the reference's transfer motif.
 *
 * Entirely decorative and hidden from assistive technology: the status badge
 * below it already says where the clip is going.
 */
@Composable
private fun FlowHero(peerName: String) {
    val colors = AnyFlowTheme.colors
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(vertical = AnyFlowSpacing.lg)
            .decorative(),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.Center,
    ) {
        Icon(
            painter = painterResource(R.drawable.ic_clipboard),
            contentDescription = null,
            tint = colors.textPrimary,
            modifier = Modifier.size(AnyFlowIconSize.hero),
        )
        Icon(
            painter = painterResource(R.drawable.ribbon_connection),
            contentDescription = null,
            tint = Color.Unspecified,
            modifier = Modifier
                .padding(horizontal = AnyFlowSpacing.xs)
                .width(120.dp)
                .height(44.dp),
        )
        Box(
            Modifier
                .size(56.dp)
                .background(colors.accentBlue.copy(alpha = 0.12f), CircleShape),
            contentAlignment = Alignment.Center,
        ) {
            Icon(
                painter = painterResource(R.drawable.ic_device_desktop),
                contentDescription = null,
                tint = colors.accentBlue,
                modifier = Modifier.size(AnyFlowIconSize.xlarge),
            )
        }
    }
}
