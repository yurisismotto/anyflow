package io.github.yurisismotto.omnibridge.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
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
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Icon
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import io.github.yurisismotto.omnibridge.R
import io.github.yurisismotto.omnibridge.capability.NotificationsCapability
import androidx.annotation.StringRes
import io.github.yurisismotto.omnibridge.notifications.LockPolicy
import io.github.yurisismotto.omnibridge.notifications.NotificationApp
import io.github.yurisismotto.omnibridge.notifications.NotificationApps
import io.github.yurisismotto.omnibridge.notifications.NotificationReadiness
import io.github.yurisismotto.omnibridge.ui.components.OmniBridgeCapabilityRow
import io.github.yurisismotto.omnibridge.ui.components.OmniBridgeCard
import io.github.yurisismotto.omnibridge.ui.components.OmniBridgeSecondaryButton
import io.github.yurisismotto.omnibridge.ui.components.OmniBridgeSectionLabel
import io.github.yurisismotto.omnibridge.ui.components.OmniBridgeSecurityNotice
import io.github.yurisismotto.omnibridge.ui.components.OmniBridgeStatusBadge
import io.github.yurisismotto.omnibridge.ui.components.NoticeTone
import io.github.yurisismotto.omnibridge.ui.theme.OmniBridgeIconSize
import io.github.yurisismotto.omnibridge.ui.theme.OmniBridgeRadius
import io.github.yurisismotto.omnibridge.ui.theme.OmniBridgeSpacing
import io.github.yurisismotto.omnibridge.ui.theme.OmniBridgeTheme
import io.github.yurisismotto.omnibridge.ui.theme.OmniBridgeType
import io.github.yurisismotto.omnibridge.ui.theme.MinTouchTarget

/**
 * One computer's notification consent.
 *
 * ## Three gates, shown as three things
 *
 * The screen is laid out in the order the permissions actually apply, and it
 * never collapses them:
 *
 * 1. **Share notifications with this computer** — OmniBridge's own per-peer
 *    grant, written to the same trust store the clipboard and files switches
 *    write. Off until a person turns it on.
 * 2. **Notification access** — Android's, for the whole device, granted in
 *    Settings and nowhere else. Its row says plainly that allowing it shares
 *    nothing by itself.
 * 3. **Apps** — deny by default, and the reason the two gates above still
 *    share nothing on their own.
 *
 * A single switch would be simpler and would be a lie. N2 measured a real
 * state in which this phone had granted, the computer had not, and the honest
 * symptom was `roles=0`; the status line at the top of this screen is what
 * makes that state explainable instead of looking like a bug.
 *
 * ## What is deliberately not here
 *
 * No list of notifications, past or present, and no screen that says one is
 * coming. No "restore full content when unlocked" setting — it cannot exist
 * without keeping a title and a body in memory across the lock, which is a
 * history by another name. No dismissal *history*: the dismissal switch at the
 * bottom of this screen shows two counters for the current connection and
 * nothing that could name what was dismissed.
 *
 * And nothing wider than a dismissal. There is no control here for notification
 * actions, replies, opening an app or clearing everything, because
 * `DismissRequest` has no field that could carry any of them — the absence is
 * in the schema, not in this file.
 */
@Composable
fun NotificationSettingsScreen(
    state: MainUiState,
    actions: MainActions,
    fingerprintHex: String,
    onOpenAppPicker: (String) -> Unit,
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

    val policy = peer.notificationPolicy
    val granted = peer.allows(NotificationsCapability.ID)
    val gates = state.notificationGates(peer)
    val readiness = gates.readiness()

    // Loaded once per visit, off the main thread, so the applications row can
    // show a real denominator and a real "new apps" count rather than a guess.
    // It is a hundred binder calls; doing it during composition would drop
    // frames on the screen a person is reading a permission on.
    var apps by remember(fingerprintHex) { mutableStateOf(emptyList<NotificationApp>()) }
    LaunchedEffect(fingerprintHex, granted, policy.allowedApps) {
        apps = if (granted) actions.loadNotificationApps(peer) else emptyList()
    }
    val newApps = NotificationApps.newlyInstalled(apps, policy.knownApps)

    Column(
        modifier = modifier
            .fillMaxSize()
            .verticalScroll(rememberScrollState())
            .padding(horizontal = OmniBridgeSpacing.md),
        verticalArrangement = Arrangement.spacedBy(OmniBridgeSpacing.sm),
    ) {
        Spacer(Modifier.height(OmniBridgeSpacing.xs))

        // --- what is actually happening, right now ------------------------
        OmniBridgeCard {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Icon(
                    painter = painterResource(R.drawable.ic_notifications),
                    contentDescription = null,
                    tint = colors.accentViolet,
                    modifier = Modifier.size(OmniBridgeIconSize.large),
                )
                Spacer(Modifier.width(OmniBridgeSpacing.sm))
                Text(
                    peer.deviceName,
                    style = OmniBridgeType.subtitle,
                    color = colors.textPrimary,
                    modifier = Modifier.weight(1f),
                )
                OmniBridgeStatusBadge(
                    status = NotificationUiMapping.status(readiness),
                    label = stringResource(NotificationUiMapping.statusLabel(readiness)),
                )
            }
            Text(
                stringResource(NotificationUiMapping.statusDetail(readiness)),
                style = OmniBridgeType.caption,
                color = colors.textSecondary,
            )
        }

        // --- gate 2: this computer ----------------------------------------
        OmniBridgeSectionLabel(stringResource(R.string.notif_screen_title))
        OmniBridgeCard {
            OmniBridgeCapabilityRow(
                title = stringResource(R.string.notif_share_title),
                description = stringResource(R.string.notif_share_description),
                icon = R.drawable.ic_notifications,
                accent = colors.accentViolet,
                checked = granted,
                onCheckedChange = { actions.onSetNotificationsGrant(peer, it) },
            )
        }

        if (granted) {
            // --- gate 1: Android ------------------------------------------
            OmniBridgeCard {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text(
                        stringResource(R.string.notif_access_title),
                        style = OmniBridgeType.body,
                        color = colors.textPrimary,
                        modifier = Modifier.weight(1f).semantics { heading() },
                    )
                    OmniBridgeStatusBadge(
                        status = if (gates.osAccessGranted) {
                            NotificationUiMapping.status(NotificationReadiness.READY)
                        } else {
                            NotificationUiMapping.status(
                                NotificationReadiness.NEEDS_ANDROID_ACCESS,
                            )
                        },
                        label = stringResource(
                            if (gates.osAccessGranted) {
                                R.string.notif_access_allowed
                            } else {
                                R.string.notif_access_not_allowed
                            },
                        ),
                    )
                }
                Text(
                    stringResource(R.string.notif_access_explainer),
                    style = OmniBridgeType.caption,
                    color = colors.textSecondary,
                )
                Spacer(Modifier.height(OmniBridgeSpacing.xxs))
                // Offered whether or not it is granted: this is also the way
                // back to Android's own screen to take it away.
                OmniBridgeSecondaryButton(
                    text = stringResource(R.string.notif_access_open_settings),
                    icon = R.drawable.ic_settings,
                    onClick = actions.onOpenNotificationAccess,
                )
                if (gates.osAccessGranted) {
                    Text(
                        stringResource(R.string.notif_access_revoke_hint),
                        style = OmniBridgeType.caption,
                        color = colors.textMuted,
                    )
                }
            }

            // --- gate 3: which applications -------------------------------
            AppsRow(
                allowedCount = policy.allowedApps.size,
                // The last count the picker actually showed, so the row never
                // invents a denominator before the list has been read.
                totalCount = maxOf(apps.size, policy.knownApps.size),
                newCount = newApps.size,
                onClick = { onOpenAppPicker(fingerprintHex) },
            )

            // --- everything else the source decides -----------------------
            OmniBridgeCard {
                OmniBridgeCapabilityRow(
                    title = stringResource(R.string.notif_mirror_title),
                    description = stringResource(R.string.notif_mirror_description),
                    icon = R.drawable.ic_send,
                    accent = colors.accentTeal,
                    checked = policy.allowMirror,
                    onCheckedChange = {
                        actions.onSetNotificationPolicy(peer, policy.copy(allowMirror = it))
                    },
                )
                OmniBridgeCapabilityRow(
                    title = stringResource(R.string.notif_ongoing_title),
                    description = stringResource(R.string.notif_ongoing_description),
                    icon = R.drawable.ic_activity,
                    accent = colors.accentBlue,
                    checked = policy.includeOngoing,
                    onCheckedChange = {
                        actions.onSetNotificationPolicy(peer, policy.copy(includeOngoing = it))
                    },
                )
                if (state.hasWorkProfile) {
                    OmniBridgeCapabilityRow(
                        title = stringResource(R.string.notif_work_title),
                        description = stringResource(R.string.notif_work_description),
                        icon = R.drawable.ic_shield,
                        accent = colors.accentAmber,
                        checked = policy.includeWorkProfile,
                        onCheckedChange = {
                            actions.onSetNotificationPolicy(
                                peer,
                                policy.copy(includeWorkProfile = it),
                            )
                        },
                    )
                } else {
                    Text(
                        stringResource(R.string.notif_work_title) + " — " +
                            stringResource(R.string.notif_work_absent),
                        style = OmniBridgeType.caption,
                        color = colors.textMuted,
                    )
                }
            }

            // --- the lock policy ------------------------------------------
            OmniBridgeSectionLabel(stringResource(R.string.notif_locked_title))
            OmniBridgeCard {
                Column(Modifier.selectableGroup()) {
                    for (option in LockPolicy.entries) {
                        LockPolicyOption(
                            option = option,
                            selected = policy.whenSourceLocked == option,
                            onSelect = {
                                actions.onSetNotificationPolicy(
                                    peer,
                                    policy.copy(whenSourceLocked = option),
                                )
                            },
                        )
                    }
                }
                Text(
                    stringResource(R.string.notif_locked_not_retroactive),
                    style = OmniBridgeType.caption,
                    color = colors.textMuted,
                )
            }

            // --- dismissal: the one control that acts on this device -------
            //
            // Last on the screen, and deliberately so. Everything above it is
            // about what leaves this phone; this is the only setting that lets
            // a computer change something here, and it reads as the different
            // kind of decision it is.
            OmniBridgeCard {
                OmniBridgeCapabilityRow(
                    title = stringResource(R.string.notif_dismiss_title),
                    description = stringResource(R.string.notif_dismiss_description),
                    icon = R.drawable.ic_close,
                    accent = colors.accentAmber,
                    checked = policy.allowDismissSync,
                    onCheckedChange = {
                        actions.onSetNotificationPolicy(
                            peer,
                            // Exactly one field. The grant, the app list, the
                            // mirroring switch and the lock policy are all
                            // reached through their own `copy` calls elsewhere
                            // on this screen, and this one cannot touch them.
                            policy.copy(allowDismissSync = it),
                        )
                    },
                )
                // The switch says on or off; this says whether "on" is
                // currently doing anything, and what to do when it is not.
                // They are two different facts and a switch alone cannot carry
                // both — which is the whole reason the state is a type.
                val dismissState = gates.dismissReadiness()
                Text(
                    stringResource(NotificationUiMapping.dismissDetail(dismissState)),
                    style = OmniBridgeType.caption,
                    color = if (dismissState.needsAttention) {
                        colors.accentAmber
                    } else {
                        colors.textSecondary
                    },
                )
                val peerStatus = state.notifications.peers[fingerprintHex]
                if (peerStatus != null && peerStatus.dismissRequests > 0) {
                    // Counts, on this connection, from memory. There is no
                    // list of dismissals anywhere and no field that could hold
                    // one — a dismiss event journal is what §26 forbids.
                    Text(
                        stringResource(
                            R.string.notif_dismiss_counts,
                            peerStatus.dismissRequests,
                            peerStatus.dismissesPerformed,
                        ),
                        style = OmniBridgeType.caption,
                        color = colors.textMuted,
                    )
                }
            }
        }

        OmniBridgeSecurityNotice(
            title = stringResource(R.string.notif_privacy_title),
            body = stringResource(R.string.notif_privacy_body),
            tone = NoticeTone.Info,
            icon = R.drawable.ic_shield_check,
        )

        NotificationDiagnostics(state = state, fingerprintHex = fingerprintHex)
        Spacer(Modifier.height(OmniBridgeSpacing.xxl))
    }
}

/**
 * The applications row: a count, and the way to change it.
 *
 * The count is the honest headline. "0 of 97" is not an error state and is not
 * drawn as one — it is what granting without finishing the picker leaves
 * behind, and in that state precisely nothing leaves the phone.
 */
@Composable
private fun AppsRow(
    allowedCount: Int,
    totalCount: Int,
    newCount: Int,
    onClick: () -> Unit,
) {
    val colors = OmniBridgeTheme.colors
    val label = stringResource(R.string.notif_apps_title)
    OmniBridgeCard(
        modifier = Modifier
            .clip(RoundedCornerShape(OmniBridgeRadius.large))
            .clickable(role = Role.Button, onClick = onClick)
            .semantics { contentDescription = label },
    ) {
        Row(
            verticalAlignment = Alignment.CenterVertically,
            modifier = Modifier.heightIn(min = MinTouchTarget),
        ) {
            Icon(
                painter = painterResource(R.drawable.ic_peers),
                contentDescription = null,
                tint = colors.accentBlue,
                modifier = Modifier.size(OmniBridgeIconSize.large),
            )
            Spacer(Modifier.width(OmniBridgeSpacing.sm))
            Column(Modifier.weight(1f)) {
                Text(label, style = OmniBridgeType.body, color = colors.textPrimary)
                Text(
                    if (allowedCount == 0 || totalCount == 0) {
                        stringResource(R.string.notif_apps_none)
                    } else {
                        stringResource(R.string.notif_apps_count, allowedCount, totalCount)
                    },
                    style = OmniBridgeType.caption,
                    color = colors.textSecondary,
                )
                if (newCount > 0) {
                    Text(
                        pluralStringResource(R.plurals.notif_apps_new, newCount, newCount),
                        style = OmniBridgeType.caption,
                        color = colors.accentAmber,
                    )
                }
            }
            Icon(
                painter = painterResource(R.drawable.ic_chevron_right),
                contentDescription = null,
                tint = colors.textMuted,
                modifier = Modifier.size(OmniBridgeIconSize.large),
            )
        }
    }
}

/** One lock-policy choice: what it is called, and what it actually does. */
@Composable
private fun LockPolicyOption(
    option: LockPolicy,
    selected: Boolean,
    onSelect: () -> Unit,
) {
    val colors = OmniBridgeTheme.colors
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .heightIn(min = MinTouchTarget)
            // `selectable` on the row, not on the button: the whole row is the
            // target, and the radio itself is marked as decoration so a screen
            // reader announces one control rather than two.
            .selectable(selected = selected, role = Role.RadioButton, onClick = onSelect)
            .padding(vertical = OmniBridgeSpacing.xs),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        // The row above carries the role, the label and the selected state, so
        // the glyph is decoration. Left with semantics of its own it announces
        // a second, contextless "not checked" beside a row that is checked.
        RadioButton(
            selected = selected,
            onClick = null,
            modifier = Modifier.clearAndSetSemantics {},
        )
        Spacer(Modifier.width(OmniBridgeSpacing.sm))
        Column(Modifier.weight(1f)) {
            Text(
                stringResource(NotificationUiMapping.lockPolicyLabel(option)),
                style = OmniBridgeType.body,
                color = colors.textPrimary,
            )
            Text(
                stringResource(NotificationUiMapping.lockPolicyDetail(option)),
                style = OmniBridgeType.caption,
                color = colors.textSecondary,
            )
        }
    }
}

/**
 * The protocol vocabulary, in the one place it belongs.
 *
 * `SOURCE`, `SINK` and an epoch mean nothing to most people and everything to
 * somebody working out why two devices disagree, so they are here — below the
 * fold, in a section labelled Details — and nowhere in the primary UI.
 */
@Composable
private fun NotificationDiagnostics(state: MainUiState, fingerprintHex: String) {
    val colors = OmniBridgeTheme.colors
    val status = state.notifications
    val peerStatus = status.peers[fingerprintHex]
    val peer = state.peerByHex(fingerprintHex)

    OmniBridgeSectionLabel(stringResource(R.string.notif_diagnostics_title))
    OmniBridgeCard {
        DiagnosticRow(
            stringResource(R.string.notif_diag_access),
            stringResource(
                if (status.accessGranted) R.string.notif_access_allowed
                else R.string.notif_access_not_allowed,
            ),
        )
        DiagnosticRow(
            stringResource(R.string.notif_diag_listener),
            stringResource(
                if (status.listenerConnected) R.string.notif_diag_listener_bound
                else R.string.notif_diag_listener_unbound,
            ),
        )
        DiagnosticRow(
            stringResource(R.string.notif_diag_grant),
            stringResource(
                if (peer?.allows(NotificationsCapability.ID) == true) {
                    R.string.notif_diag_granted
                } else {
                    R.string.notif_diag_not_granted
                },
            ),
        )
        if (peerStatus == null) {
            DiagnosticRow(
                stringResource(R.string.notif_diag_local_role),
                stringResource(R.string.notif_diag_no_session),
            )
        } else {
            DiagnosticRow(
                stringResource(R.string.notif_diag_local_role),
                roles(
                    peerStatus.localIsSource to R.string.notif_diag_role_source,
                    peerStatus.localIsDismissTarget to R.string.notif_diag_role_dismiss_target,
                ) + " · " + stringResource(
                    R.string.notif_diag_epoch,
                    peerStatus.localEpoch.toInt(),
                ),
            )
            DiagnosticRow(
                stringResource(R.string.notif_diag_peer_role),
                roles(
                    peerStatus.peerIsSink to R.string.notif_diag_role_sink,
                    peerStatus.peerIsDismissReporter to R.string.notif_diag_role_dismiss_reporter,
                ) + " · " + stringResource(R.string.notif_diag_epoch, peerStatus.peerEpoch),
            )
            DiagnosticRow(
                stringResource(R.string.notif_diag_dismiss),
                "${peerStatus.dismissRequests} asked · ${peerStatus.dismissesPerformed} done",
            )
        }
        Text(
            // Counts, and only counts: there is no field on the status object
            // that could hold a notification, so there is nothing else to show.
            "tracked=${status.trackedNotifications} sent=${status.emitted} " +
                "not mirrored=${status.dropped}",
            style = OmniBridgeType.caption,
            color = colors.textMuted,
        )
    }
}

/**
 * The protocol role names a side is currently announcing, or "no role".
 *
 * A set rather than a single name, because both ends now claim two. It lives
 * in Details and nowhere else: `SOURCE` and `DISMISS_TARGET` mean everything
 * to somebody working out why two devices disagree, and nothing to anybody
 * else.
 */
@Composable
private fun roles(vararg claims: Pair<Boolean, Int>): String {
    val present = claims.filter { it.first }.map { stringResource(it.second) }
    return if (present.isEmpty()) {
        stringResource(R.string.notif_diag_role_none)
    } else {
        present.joinToString(" + ")
    }
}

@Composable
private fun DiagnosticRow(label: String, value: String) {
    val colors = OmniBridgeTheme.colors
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
        Text(
            label,
            style = OmniBridgeType.caption,
            color = colors.textSecondary,
            modifier = Modifier.weight(1f),
        )
        Text(value, style = OmniBridgeType.label, color = colors.textPrimary)
    }
}
