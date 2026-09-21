package io.github.yurisismotto.omnibridge.ui

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
import io.github.yurisismotto.omnibridge.R
import io.github.yurisismotto.omnibridge.ui.components.OmniBridgeCard
import io.github.yurisismotto.omnibridge.ui.components.OmniBridgePrimaryButton
import io.github.yurisismotto.omnibridge.ui.components.OmniBridgeSecurityNotice
import io.github.yurisismotto.omnibridge.ui.components.OmniBridgeStatusBadge
import io.github.yurisismotto.omnibridge.ui.components.OmniBridgeTextButton
import io.github.yurisismotto.omnibridge.ui.components.NoticeTone
import io.github.yurisismotto.omnibridge.ui.components.decorative
import io.github.yurisismotto.omnibridge.ui.theme.OmniBridgeGradient
import io.github.yurisismotto.omnibridge.ui.theme.OmniBridgeIconSize
import io.github.yurisismotto.omnibridge.ui.theme.OmniBridgeRadius
import io.github.yurisismotto.omnibridge.ui.theme.OmniBridgeSpacing
import io.github.yurisismotto.omnibridge.ui.theme.OmniBridgeTheme
import io.github.yurisismotto.omnibridge.ui.theme.OmniBridgeType

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
    val colors = OmniBridgeTheme.colors
    val peer = state.peerByHex(fingerprintHex)
    if (peer == null) {
        Column(modifier.fillMaxSize().padding(OmniBridgeSpacing.md)) {
            Text(
                "This device is no longer paired.",
                style = OmniBridgeType.body,
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
            .padding(horizontal = OmniBridgeSpacing.md),
        verticalArrangement = Arrangement.spacedBy(OmniBridgeSpacing.sm),
    ) {
        FlowHero(peerName = peer.deviceName)

        Column(horizontalAlignment = Alignment.CenterHorizontally, modifier = Modifier.fillMaxWidth()) {
            OmniBridgeStatusBadge(
                state.statusFor(peer),
                label = if (connected) "Connected to ${peer.deviceName}" else peer.deviceName,
            )
            Text("Desktop · Linux", style = OmniBridgeType.caption, color = colors.textSecondary)
        }

        Spacer(Modifier.height(OmniBridgeSpacing.xs))

        when {
            preview == null -> OmniBridgeCard {
                Text("Clipboard", style = OmniBridgeType.label, color = colors.textSecondary)
                Text(
                    "There is nothing to send. Copy some text, then come back.",
                    style = OmniBridgeType.body,
                    color = colors.textSecondary,
                )
            }

            else -> {
                OmniBridgeCard {
                    Text(
                        "Clipboard preview",
                        style = OmniBridgeType.label,
                        color = colors.textSecondary,
                    )
                    Box(
                        Modifier
                            .fillMaxWidth()
                            .clip(RoundedCornerShape(OmniBridgeRadius.small))
                            .background(colors.surfaceSunken)
                            .padding(OmniBridgeSpacing.sm),
                    ) {
                        Text(
                            text = preview.text ?: "•".repeat(12),
                            style = OmniBridgeType.mono,
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
                        style = OmniBridgeType.caption,
                        color = colors.textMuted,
                    )
                }

                if (preview.sensitive) {
                    OmniBridgeSecurityNotice(
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

        Spacer(Modifier.height(OmniBridgeSpacing.xs))

        val gate = state.clipboardSendGate(peer)
        OmniBridgePrimaryButton(
            text = "Send to ${peer.deviceName}",
            icon = R.drawable.ic_send,
            // The same gate as every other Send clipboard affordance. This
            // screen used to ask only whether a link was up, which made it the
            // most permissive of the three and the one a Quick Settings press
            // is most likely to land on.
            enabled = gate.ready && preview != null,
            onClick = {
                actions.onSendClipboard(peer.fingerprint)
                onBack()
            },
        )
        gate.reasonOrNull?.let { reason ->
            Text(
                reason,
                style = OmniBridgeType.caption,
                color = colors.textMuted,
            )
        }
        Box(Modifier.fillMaxWidth(), contentAlignment = Alignment.Center) {
            OmniBridgeTextButton("Cancel", onBack)
        }

        // Deliberately not "end-to-end encrypted". The link is a direct,
        // mutually authenticated TLS 1.3 session pinned to the key approved
        // at pairing — which is what this says, in the project's own terms.
        // Reaching for a marketing phrase whose meaning does not exactly
        // match is how a security claim becomes untrue.
        Row(
            Modifier.fillMaxWidth().padding(vertical = OmniBridgeSpacing.sm),
            horizontalArrangement = Arrangement.Center,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Icon(
                painter = painterResource(R.drawable.ic_shield_check),
                contentDescription = null,
                tint = colors.textMuted,
                modifier = Modifier.size(OmniBridgeIconSize.small),
            )
            Spacer(Modifier.width(OmniBridgeSpacing.xxs))
            Text(
                "Direct connection · TLS 1.3, pinned · local network",
                style = OmniBridgeType.caption,
                color = colors.textMuted,
                textAlign = TextAlign.Center,
            )
        }
        Spacer(Modifier.height(OmniBridgeSpacing.xxl))
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
    val colors = OmniBridgeTheme.colors
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(vertical = OmniBridgeSpacing.lg)
            .decorative(),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.Center,
    ) {
        Icon(
            painter = painterResource(R.drawable.ic_clipboard),
            contentDescription = null,
            tint = colors.textPrimary,
            modifier = Modifier.size(OmniBridgeIconSize.hero),
        )
        Icon(
            painter = painterResource(R.drawable.ribbon_connection),
            contentDescription = null,
            tint = Color.Unspecified,
            modifier = Modifier
                .padding(horizontal = OmniBridgeSpacing.xs)
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
                modifier = Modifier.size(OmniBridgeIconSize.xlarge),
            )
        }
    }
}
