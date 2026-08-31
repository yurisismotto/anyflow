package io.github.yurisismotto.anyflow.ui

import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.NavigationBarItemDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import io.github.yurisismotto.anyflow.R
import io.github.yurisismotto.anyflow.ui.components.AnyFlowGradientMark
import io.github.yurisismotto.anyflow.ui.theme.AnyFlowIconSize
import io.github.yurisismotto.anyflow.ui.theme.AnyFlowMotion
import io.github.yurisismotto.anyflow.ui.theme.AnyFlowSpacing
import io.github.yurisismotto.anyflow.ui.theme.AnyFlowTheme
import io.github.yurisismotto.anyflow.ui.theme.AnyFlowType
import io.github.yurisismotto.anyflow.ui.theme.motionDuration

/**
 * The app shell: title bar, bottom tabs, and one screen at a time.
 *
 * Tab state and the back stack are [rememberSaveable], so a rotation lands
 * back on the same screen rather than bouncing to the home tab.
 */
@Composable
fun AnyFlowShell(
    state: MainUiState,
    actions: MainActions,
    modifier: Modifier = Modifier,
) {
    var screen by rememberSaveable(stateSaver = ScreenSaver) {
        mutableStateOf<Screen>(Screen.Devices)
    }
    // One level of back is all the hierarchy has: a detail returns to its tab.
    val atRoot = screen is Screen.Devices || screen is Screen.Activity || screen is Screen.Settings
    BackHandler(enabled = !atRoot) { screen = Screen.Devices }

    val colors = AnyFlowTheme.colors
    // Read here rather than inside transitionSpec: that lambda is not a
    // composable scope, so the token has to be resolved before it.
    val crossfadeMs = motionDuration(AnyFlowMotion.FAST_MS)
    Scaffold(
        modifier = modifier.fillMaxSize(),
        containerColor = colors.background,
        topBar = { ShellTopBar(screen, onBack = { screen = Screen.Devices }) },
        bottomBar = {
            ShellBottomBar(
                current = screen.tab,
                onSelect = { tab ->
                    screen = when (tab) {
                        Tab.Devices -> Screen.Devices
                        Tab.Activity -> Screen.Activity
                        Tab.Settings -> Screen.Settings
                    }
                },
            )
        },
    ) { padding ->
        AnimatedContent(
            targetState = screen,
            transitionSpec = {
                fadeIn(tween(crossfadeMs)) togetherWith fadeOut(tween(crossfadeMs))
            },
            label = "screen",
        ) { current ->
            // On a tablet a full-width single column reads as a stretched
            // phone. Capping the measure and centring it keeps line lengths
            // readable and the cards from becoming empty bands.
            Box(
                Modifier
                    .padding(padding)
                    .fillMaxSize(),
                contentAlignment = Alignment.TopCenter,
            ) {
              Box(Modifier.widthIn(max = ContentMaxWidth)) {
                when (current) {
                    Screen.Devices -> DevicesScreen(
                        state = state,
                        actions = actions,
                        onOpenPeer = { screen = Screen.PeerDetail(it) },
                        onSendClipboard = { screen = Screen.SendClipboard(it) },
                    )

                    Screen.Activity -> ActivityScreen(state = state, actions = actions)

                    Screen.Settings -> SettingsScreen(state = state, actions = actions)

                    is Screen.PeerDetail -> PeerDetailScreen(
                        state = state,
                        actions = actions,
                        fingerprintHex = current.fingerprintHex,
                        onBack = { screen = Screen.Devices },
                        onSendClipboard = { screen = Screen.SendClipboard(it) },
                    )

                    is Screen.SendClipboard -> SendClipboardScreen(
                        state = state,
                        actions = actions,
                        fingerprintHex = current.fingerprintHex,
                        onBack = { screen = Screen.Devices },
                    )
                }
              }
            }
        }
    }
}

/**
 * The widest the content column is allowed to get.
 *
 * Phones never reach it; tablets and unfolded foldables do, and without it a
 * device card becomes a 900 dp band with a name floating at one end.
 */
private val ContentMaxWidth = 640.dp

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun ShellTopBar(screen: Screen, onBack: () -> Unit) {
    val colors = AnyFlowTheme.colors
    val title = when (screen) {
        Screen.Devices -> "AnyFlow"
        Screen.Activity -> "Activity"
        Screen.Settings -> "Settings"
        is Screen.PeerDetail -> "Device"
        is Screen.SendClipboard -> "Send clipboard"
    }
    val showsBack = screen !is Screen.Devices && screen !is Screen.Activity &&
        screen !is Screen.Settings

    TopAppBar(
        title = {
            Row(verticalAlignment = Alignment.CenterVertically) {
                // The mark rides beside the wordmark on the home screen only;
                // elsewhere the title names the screen, which is more useful
                // than repeating the brand on every view.
                if (screen is Screen.Devices) {
                    AnyFlowGradientMark(size = AnyFlowIconSize.large)
                    Spacer(Modifier.width(AnyFlowSpacing.xs))
                }
                Text(title, style = AnyFlowType.heading, color = colors.textPrimary)
            }
        },
        navigationIcon = {
            if (showsBack) {
                IconButton(onClick = onBack) {
                    Icon(
                        painter = painterResource(R.drawable.ic_arrow_back),
                        contentDescription = "Back",
                        tint = colors.textPrimary,
                        modifier = Modifier.size(AnyFlowIconSize.large),
                    )
                }
            }
        },
        colors = TopAppBarDefaults.topAppBarColors(
            containerColor = colors.background,
            titleContentColor = colors.textPrimary,
        ),
    )
}

@Composable
private fun ShellBottomBar(current: Tab, onSelect: (Tab) -> Unit) {
    val colors = AnyFlowTheme.colors
    NavigationBar(containerColor = colors.surface, tonalElevation = 0.dp) {
        Tab.entries.forEach { tab ->
            val selected = tab == current
            NavigationBarItem(
                selected = selected,
                onClick = { onSelect(tab) },
                icon = {
                    Icon(
                        painter = painterResource(tab.icon),
                        // The label below is always shown, so the icon would
                        // otherwise be announced twice.
                        contentDescription = null,
                        modifier = Modifier.size(AnyFlowIconSize.large),
                    )
                },
                label = { Text(tab.label, style = AnyFlowType.caption) },
                alwaysShowLabel = true,
                modifier = Modifier.semantics { contentDescription = tab.label },
                colors = NavigationBarItemDefaults.colors(
                    selectedIconColor = colors.accentTeal,
                    selectedTextColor = colors.accentTeal,
                    indicatorColor = colors.accentTeal.copy(alpha = 0.12f),
                    unselectedIconColor = colors.textMuted,
                    unselectedTextColor = colors.textMuted,
                ),
            )
        }
    }
}
