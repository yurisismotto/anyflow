package io.github.yurisismotto.omnibridge.ui

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
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.Image
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Checkbox
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.painter.BitmapPainter
import androidx.compose.ui.graphics.painter.Painter
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.unit.dp
import androidx.core.graphics.drawable.toBitmap
import io.github.yurisismotto.omnibridge.R
import io.github.yurisismotto.omnibridge.capability.NotificationsCapability
import io.github.yurisismotto.omnibridge.notifications.NotificationApp
import io.github.yurisismotto.omnibridge.notifications.NotificationApps
import io.github.yurisismotto.omnibridge.ui.components.OmniBridgeCard
import io.github.yurisismotto.omnibridge.ui.components.OmniBridgeSecondaryButton
import io.github.yurisismotto.omnibridge.ui.components.OmniBridgeSecurityNotice
import io.github.yurisismotto.omnibridge.ui.components.NoticeTone
import io.github.yurisismotto.omnibridge.ui.theme.OmniBridgeIconSize
import io.github.yurisismotto.omnibridge.ui.theme.OmniBridgeSpacing
import io.github.yurisismotto.omnibridge.ui.theme.OmniBridgeTheme
import io.github.yurisismotto.omnibridge.ui.theme.OmniBridgeType
import io.github.yurisismotto.omnibridge.ui.theme.MinTouchTarget

/**
 * Which applications one computer may receive. The deny-by-default picker.
 *
 * ## Nothing is chosen for you
 *
 * The list starts with whatever is stored, which on a computer that has just
 * been granted is **nothing**. There is no pre-selection, no "recommended"
 * set, no category rule and no heuristic that turns anything on — the design
 * forbids all of them (ADR-0015 §5), and a guess dressed as a security
 * control is worse than an honest boundary because the person trusts it.
 *
 * **Select all** exists and is one tap away, because a feature nobody can
 * switch on fully is a feature people work around. It is never automatic, and
 * it asks first, because it is the one action here that can share a hundred
 * applications at once.
 *
 * ## Where the rows come from
 *
 * [NotificationApps.build] merges three sources — applications with a launcher
 * entry, applications currently in the notification shade, and applications
 * already chosen — and drops OmniBridge's own package. See
 * [io.github.yurisismotto.omnibridge.notifications.InstalledApps] for why none of
 * that needs `QUERY_ALL_PACKAGES`.
 *
 * ## Icons
 *
 * Loaded from the local package manager, one per visible row, and never
 * transmitted: `notifications.v1` has no field that could carry an image and
 * none was added for this. A missing icon is a blank tile, never an error.
 */
@Composable
fun AppPickerScreen(
    state: MainUiState,
    actions: MainActions,
    fingerprintHex: String,
    modifier: Modifier = Modifier,
) {
    val colors = OmniBridgeTheme.colors
    val peer = state.peerByHex(fingerprintHex)
    if (peer == null || !peer.allows(NotificationsCapability.ID)) {
        // Either the computer was forgotten while this screen was open, or its
        // grant was withdrawn elsewhere. Both mean the same thing here: there
        // is nothing to choose applications for.
        Column(modifier.fillMaxSize().padding(OmniBridgeSpacing.md)) {
            Text(
                stringResource(R.string.notif_detail_sharing_off),
                style = OmniBridgeType.body,
                color = colors.textSecondary,
            )
        }
        return
    }

    val policy = peer.notificationPolicy
    var query by rememberSaveable(fingerprintHex) { mutableStateOf("") }
    var loading by remember(fingerprintHex) { mutableStateOf(true) }
    var apps by remember(fingerprintHex) { mutableStateOf(emptyList<NotificationApp>()) }
    var confirmSelectAll by remember(fingerprintHex) { mutableStateOf(false) }

    // Reloaded when the selection changes so the rows track the stored policy
    // rather than a local copy — the trust store stays the single authority,
    // exactly as it is for every other switch in the app.
    LaunchedEffect(fingerprintHex, policy.allowedApps) {
        apps = actions.loadNotificationApps(peer)
        loading = false
    }

    // The baseline for "N new apps are not being shared" is recorded when the
    // list is actually shown, which is the only moment we can honestly claim
    // the person has seen it. Recorded once the load finishes, and only when
    // the set has changed, so it does not rewrite the trust store on every
    // recomposition.
    LaunchedEffect(apps) {
        if (apps.isEmpty()) return@LaunchedEffect
        val seen = apps.map { it.packageName }.toSet()
        if (seen != policy.knownApps) {
            actions.onSetNotificationPolicy(peer, policy.copy(knownApps = seen))
        }
    }

    val visible = remember(apps, query) { NotificationApps.search(apps, query) }
    val chosen = policy.allowedApps.size

    Column(
        modifier = modifier.fillMaxSize().padding(horizontal = OmniBridgeSpacing.md),
        verticalArrangement = Arrangement.spacedBy(OmniBridgeSpacing.sm),
    ) {
        Spacer(Modifier.height(OmniBridgeSpacing.xs))
        Text(
            stringResource(R.string.notif_picker_intro),
            style = OmniBridgeType.caption,
            color = colors.textSecondary,
        )

        OutlinedTextField(
            value = query,
            onValueChange = { query = it },
            singleLine = true,
            modifier = Modifier.fillMaxWidth(),
            label = { Text(stringResource(R.string.notif_picker_search)) },
            leadingIcon = {
                Icon(
                    painter = painterResource(R.drawable.ic_search),
                    contentDescription = null,
                    modifier = Modifier.size(OmniBridgeIconSize.medium),
                )
            },
            trailingIcon = {
                if (query.isNotEmpty()) {
                    IconButton(onClick = { query = "" }) {
                        Icon(
                            painter = painterResource(R.drawable.ic_close),
                            contentDescription =
                                stringResource(R.string.notif_picker_search_clear),
                            modifier = Modifier.size(OmniBridgeIconSize.medium),
                        )
                    }
                }
            },
            keyboardOptions = KeyboardOptions(imeAction = ImeAction.Search),
        )

        Row(
            horizontalArrangement = Arrangement.spacedBy(OmniBridgeSpacing.xs),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(
                if (chosen == 0) {
                    stringResource(R.string.notif_apps_none)
                } else {
                    stringResource(R.string.notif_apps_count, chosen, apps.size)
                },
                style = OmniBridgeType.label,
                color = colors.textSecondary,
                modifier = Modifier.weight(1f),
            )
            OmniBridgeSecondaryButton(
                text = stringResource(R.string.notif_picker_select_all),
                // Deliberate, and never a side effect of anything else.
                onClick = { confirmSelectAll = true },
                enabled = apps.isNotEmpty() && chosen < apps.size,
            )
            OmniBridgeSecondaryButton(
                text = stringResource(R.string.notif_picker_clear_all),
                onClick = {
                    actions.onSetNotificationPolicy(peer, policy.copy(allowedApps = emptySet()))
                },
                enabled = chosen > 0,
            )
        }

        when {
            loading -> Box(
                Modifier.fillMaxSize(),
                contentAlignment = Alignment.TopCenter,
            ) { CircularProgressIndicator(Modifier.padding(OmniBridgeSpacing.xl)) }

            apps.isEmpty() -> OmniBridgeSecurityNotice(
                title = stringResource(R.string.notif_picker_empty),
                tone = NoticeTone.Caution,
                icon = R.drawable.ic_warning,
            )

            visible.isEmpty() -> Text(
                stringResource(R.string.notif_picker_no_results),
                style = OmniBridgeType.body,
                color = colors.textSecondary,
            )

            else -> LazyColumn(
                verticalArrangement = Arrangement.spacedBy(OmniBridgeSpacing.xxs),
            ) {
                items(visible, key = { it.packageName }) { app ->
                    AppRow(
                        app = app,
                        loadIcon = actions.loadAppIcon,
                        onToggle = { wanted ->
                            val next = policy.allowedApps.toMutableSet()
                            if (wanted) next += app.packageName else next -= app.packageName
                            actions.onSetNotificationPolicy(
                                peer,
                                policy.copy(allowedApps = next),
                            )
                        },
                    )
                }
                item { Spacer(Modifier.height(OmniBridgeSpacing.xxl)) }
            }
        }
    }

    if (confirmSelectAll) {
        SelectAllDialog(
            count = apps.size,
            onConfirm = {
                confirmSelectAll = false
                actions.onSetNotificationPolicy(
                    peer,
                    policy.copy(allowedApps = apps.map { it.packageName }.toSet()),
                )
            },
            onDismiss = { confirmSelectAll = false },
        )
    }
}

/**
 * One application.
 *
 * The package name is shown under the label rather than hidden, because two
 * applications can carry the same label and the person choosing which of them
 * may read their messages is entitled to tell them apart.
 */
@Composable
private fun AppRow(
    app: NotificationApp,
    loadIcon: suspend (String) -> android.graphics.drawable.Drawable?,
    onToggle: (Boolean) -> Unit,
) {
    val colors = OmniBridgeTheme.colors
    val shared = stringResource(R.string.notif_picker_shared)
    val notShared = stringResource(R.string.notif_picker_not_shared)
    OmniBridgeCard(contentPadding = OmniBridgeSpacing.sm) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .heightIn(min = MinTouchTarget)
                .semantics(mergeDescendants = true) {
                    contentDescription = app.label
                    // The row's meaning is "shared" or "not shared", and it is
                    // announced as a word: a checkbox whose only signal was a
                    // tick would be unreadable to anyone who cannot see it.
                    stateDescription = if (app.allowed) shared else notShared
                },
            verticalAlignment = Alignment.CenterVertically,
        ) {
            AppIcon(app.packageName, loadIcon)
            Spacer(Modifier.width(OmniBridgeSpacing.sm))
            Column(Modifier.weight(1f)) {
                Text(app.label, style = OmniBridgeType.body, color = colors.textPrimary)
                Text(
                    app.packageName,
                    style = OmniBridgeType.caption,
                    color = colors.textMuted,
                )
                if (app.notifying) {
                    Text(
                        stringResource(R.string.notif_picker_notifying),
                        style = OmniBridgeType.caption,
                        color = colors.accentBlue,
                    )
                }
            }
            Checkbox(
                checked = app.allowed,
                onCheckedChange = onToggle,
                // The row above already carries the label and the state.
                modifier = Modifier.semantics { contentDescription = app.label },
            )
        }
    }
}

/**
 * The application's own icon, or a neutral placeholder.
 *
 * Rasterised once at a fixed size on a background thread and held only for as
 * long as the row is composed. Nothing is written to disk: the platform's own
 * icon cache sits behind `getApplicationIcon` and is the only cache involved,
 * and a picker is not a reason to start keeping copies of other applications'
 * artwork.
 */
@Composable
private fun AppIcon(
    packageName: String,
    loadIcon: suspend (String) -> android.graphics.drawable.Drawable?,
) {
    val painter by produceState<Painter?>(initialValue = null, packageName) {
        value = runCatching {
            loadIcon(packageName)?.toBitmap(ICON_PX, ICON_PX)?.asImageBitmap()?.let(::BitmapPainter)
        }.getOrNull()
    }
    val current = painter
    if (current == null) {
        // A missing icon is a blank tile, never an error: an application whose
        // artwork cannot be loaded is still one a person may want to share.
        Icon(
            painter = painterResource(R.drawable.ic_files),
            contentDescription = null,
            tint = OmniBridgeTheme.colors.textMuted,
            modifier = Modifier.size(ICON_DP),
        )
    } else {
        Image(
            painter = current,
            // The label beside it names the application; announcing the icon
            // as well would say it twice.
            contentDescription = null,
            modifier = Modifier.size(ICON_DP),
        )
    }
}

private const val ICON_PX = 96
private val ICON_DP = 36.dp

/**
 * "Select all" asks first.
 *
 * It is the one control on this screen that can share a hundred applications
 * in a single tap, so it is confirmed with the number in the sentence. It is
 * never reached except by pressing it, and nothing else on this screen — not
 * granting, not opening the picker, not searching — can reach it.
 */
@Composable
private fun SelectAllDialog(count: Int, onConfirm: () -> Unit, onDismiss: () -> Unit) {
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(stringResource(R.string.notif_picker_select_all)) },
        text = { Text(stringResource(R.string.notif_picker_confirm_select_all, count)) },
        confirmButton = {
            TextButton(onClick = onConfirm) {
                Text(stringResource(R.string.notif_picker_select_all))
            }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) {
                Text(stringResource(android.R.string.cancel))
            }
        },
    )
}
