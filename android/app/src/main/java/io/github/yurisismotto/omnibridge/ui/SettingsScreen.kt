package io.github.yurisismotto.omnibridge.ui

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
import io.github.yurisismotto.omnibridge.R
import io.github.yurisismotto.omnibridge.ui.components.OmniBridgeBrandMark
import io.github.yurisismotto.omnibridge.ui.components.OmniBridgeCard
import io.github.yurisismotto.omnibridge.ui.components.OmniBridgeFingerprint
import io.github.yurisismotto.omnibridge.ui.components.OmniBridgeSectionLabel
import io.github.yurisismotto.omnibridge.ui.components.OmniBridgeSecurityNotice
import io.github.yurisismotto.omnibridge.ui.theme.OmniBridgeSpacing
import io.github.yurisismotto.omnibridge.ui.theme.OmniBridgeTheme
import io.github.yurisismotto.omnibridge.ui.theme.OmniBridgeType

/**
 * This device's own identity, and what OmniBridge is.
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
    val colors = OmniBridgeTheme.colors
    Column(
        modifier = modifier
            .fillMaxSize()
            .verticalScroll(rememberScrollState())
            .padding(horizontal = OmniBridgeSpacing.md),
        verticalArrangement = Arrangement.spacedBy(OmniBridgeSpacing.sm),
    ) {
        OmniBridgeSectionLabel("This device")
        OmniBridgeCard {
            Text(state.ownDeviceName, style = OmniBridgeType.subtitle, color = colors.textPrimary)
            Spacer(Modifier.height(OmniBridgeSpacing.xxs))
            Text("Fingerprint", style = OmniBridgeType.label, color = colors.textSecondary)
            OmniBridgeFingerprint(state.ownFingerprint)
            Spacer(Modifier.height(OmniBridgeSpacing.xxs))
            Text(
                state.keyBackingDescription,
                style = OmniBridgeType.caption,
                color = colors.textSecondary,
            )
        }

        OmniBridgeSectionLabel("Paired computers")
        OmniBridgeCard {
            if (state.peers.isEmpty()) {
                Text(
                    "None yet.",
                    style = OmniBridgeType.body,
                    color = colors.textSecondary,
                )
            } else {
                state.peers.forEach { peer ->
                    Text(peer.deviceName, style = OmniBridgeType.body, color = colors.textPrimary)
                    OmniBridgeFingerprint(
                        peer.fingerprint.toDisplayShort(),
                        color = colors.textSecondary,
                    )
                    Spacer(Modifier.height(OmniBridgeSpacing.xs))
                }
            }
        }

        OmniBridgeSectionLabel("Privacy")
        OmniBridgeSecurityNotice(
            title = "Nothing leaves your network",
            body = "OmniBridge has no account, no cloud service and no analytics. " +
                "Clipboard text is never written to disk, and transfers are not logged.",
            icon = R.drawable.ic_shield_check,
        )

        Spacer(Modifier.height(OmniBridgeSpacing.lg))
        Column(
            Modifier.fillMaxWidth(),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(OmniBridgeSpacing.xs),
        ) {
            OmniBridgeBrandMark(contentDescription = "OmniBridge")
            Text("OmniBridge", style = OmniBridgeType.subtitle, color = colors.textPrimary)
            Text(
                "One bridge. Any device.",
                style = OmniBridgeType.caption,
                color = colors.textSecondary,
                textAlign = TextAlign.Center,
            )
        }
        Spacer(Modifier.height(OmniBridgeSpacing.xxl))
    }
}
