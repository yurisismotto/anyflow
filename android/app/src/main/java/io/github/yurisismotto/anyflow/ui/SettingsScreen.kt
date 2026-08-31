package io.github.yurisismotto.anyflow.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextAlign
import io.github.yurisismotto.anyflow.R
import io.github.yurisismotto.anyflow.ui.components.AnyFlowBrandMark
import io.github.yurisismotto.anyflow.ui.components.AnyFlowCard
import io.github.yurisismotto.anyflow.ui.components.AnyFlowFingerprint
import io.github.yurisismotto.anyflow.ui.components.AnyFlowSectionLabel
import io.github.yurisismotto.anyflow.ui.components.AnyFlowSecurityNotice
import io.github.yurisismotto.anyflow.ui.theme.AnyFlowSpacing
import io.github.yurisismotto.anyflow.ui.theme.AnyFlowTheme
import io.github.yurisismotto.anyflow.ui.theme.AnyFlowType

/**
 * This device's own identity, and what AnyFlow is.
 *
 * The identity block is not decoration: the fingerprint here is what the
 * person reads aloud, or compares on screen, while pairing from the other
 * side.
 */
@Composable
fun SettingsScreen(
    state: MainUiState,
    actions: MainActions,
    modifier: Modifier = Modifier,
) {
    val colors = AnyFlowTheme.colors
    Column(
        modifier = modifier
            .fillMaxSize()
            .verticalScroll(rememberScrollState())
            .padding(horizontal = AnyFlowSpacing.md),
        verticalArrangement = Arrangement.spacedBy(AnyFlowSpacing.sm),
    ) {
        AnyFlowSectionLabel("This device")
        AnyFlowCard {
            Text(state.ownDeviceName, style = AnyFlowType.subtitle, color = colors.textPrimary)
            Spacer(Modifier.height(AnyFlowSpacing.xxs))
            Text("Fingerprint", style = AnyFlowType.label, color = colors.textSecondary)
            AnyFlowFingerprint(state.ownFingerprint)
            Spacer(Modifier.height(AnyFlowSpacing.xxs))
            Text(
                state.keyBackingDescription,
                style = AnyFlowType.caption,
                color = colors.textSecondary,
            )
        }

        AnyFlowSectionLabel("Paired computers")
        AnyFlowCard {
            if (state.peers.isEmpty()) {
                Text(
                    "None yet.",
                    style = AnyFlowType.body,
                    color = colors.textSecondary,
                )
            } else {
                state.peers.forEach { peer ->
                    Text(peer.deviceName, style = AnyFlowType.body, color = colors.textPrimary)
                    AnyFlowFingerprint(
                        peer.fingerprint.toDisplayShort(),
                        color = colors.textSecondary,
                    )
                    Spacer(Modifier.height(AnyFlowSpacing.xs))
                }
            }
        }

        AnyFlowSectionLabel("Privacy")
        AnyFlowSecurityNotice(
            title = "Nothing leaves your network",
            body = "AnyFlow has no account, no cloud service and no analytics. " +
                "Clipboard text is never written to disk, and transfers are not logged.",
            icon = R.drawable.ic_shield_check,
        )

        Spacer(Modifier.height(AnyFlowSpacing.lg))
        Column(
            Modifier.fillMaxWidth(),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(AnyFlowSpacing.xs),
        ) {
            AnyFlowBrandMark(contentDescription = "AnyFlow")
            Text("AnyFlow", style = AnyFlowType.subtitle, color = colors.textPrimary)
            Text(
                "One flow. Any device.",
                style = AnyFlowType.caption,
                color = colors.textSecondary,
                textAlign = TextAlign.Center,
            )
        }
        Spacer(Modifier.height(AnyFlowSpacing.xxl))
    }
}
