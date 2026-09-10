package io.github.yurisismotto.anyflow

import io.github.yurisismotto.anyflow.AnyFlowApp
import io.github.yurisismotto.anyflow.identity.Fingerprint
import io.github.yurisismotto.anyflow.notifications.NotificationApp
import io.github.yurisismotto.anyflow.notifications.NotificationPolicy
import io.github.yurisismotto.anyflow.notifications.NotificationSource
import io.github.yurisismotto.anyflow.store.TrustStore
import io.github.yurisismotto.anyflow.ui.MainActions
import io.github.yurisismotto.anyflow.ui.MainUiState

/**
 * The state and actions the consent screens are driven with, in one place.
 *
 * Every screen under test is a pure function of [MainUiState], which is what
 * makes these tests deterministic: no daemon, no session, no package manager
 * and no permission dialog. What is exercised is the thing the wave is
 * actually accountable for — whether a switch is off by default, whether a
 * status can claim to be mirroring while a gate is shut, and whether anything
 * shares an application nobody named.
 */
object NotificationUiFixtures {

    const val PEER_HEX = "7e637b4e937b77320123456789abcdef0123456789abcdef0123456789abcdef"
    const val OWN_PACKAGE = "io.github.yurisismotto.anyflow"

    fun fingerprint(): Fingerprint = Fingerprint.fromHex(PEER_HEX)!!

    fun peer(
        granted: Boolean = false,
        policy: NotificationPolicy = NotificationPolicy(),
        otherGrants: Set<String> = setOf("battery.v1", "files.v1"),
    ): TrustStore.TrustedPeer = TrustStore.TrustedPeer(
        deviceId = "fedora-desktop",
        deviceName = "fedora",
        fingerprint = fingerprint(),
        pairedAtUnix = 1_700_000_000,
        grantedCapabilities = if (granted) otherGrants + "notifications.v1" else otherGrants,
        addresses = emptyList(),
        notificationPolicy = policy,
    )

    /** A session in which this phone is sourcing and the computer is a sink. */
    fun sourcingStatus(peerIsSink: Boolean = true): NotificationSource.Status =
        NotificationSource.Status(
            listenerConnected = true,
            accessGranted = true,
            secretAvailable = true,
            sessions = 1,
            peers = mapOf(
                PEER_HEX to NotificationSource.PeerStatus(
                    localIsSource = true,
                    localEpoch = 1,
                    peerIsSink = peerIsSink,
                    peerEpoch = 2,
                ),
            ),
        )

    /** No session, and therefore no bound listener. The idle state. */
    fun idleStatus(accessGranted: Boolean = true): NotificationSource.Status =
        NotificationSource.Status(
            listenerConnected = false,
            accessGranted = accessGranted,
            secretAvailable = true,
        )

    fun state(
        peer: TrustStore.TrustedPeer,
        accessGranted: Boolean = true,
        status: NotificationSource.Status = sourcingStatus(),
        hasWorkProfile: Boolean = false,
    ): MainUiState = MainUiState(
        ownDeviceName = "Tablet",
        ownFingerprint = "0000 0000 0000 0000",
        keyBackingDescription = "Key stored in the hardware-backed keystore",
        connection = AnyFlowApp.ConnectionState.Connected("fedora", "7E63 7B4E 937B 7732"),
        peers = listOf(peer),
        offers = emptyList(),
        transfers = emptyList(),
        pendingClips = emptyList(),
        clipboardOutcomes = emptyMap(),
        remoteBatteryPercent = null,
        notificationAccessGranted = accessGranted,
        notifications = status,
        hasWorkProfile = hasWorkProfile,
    )

    /** Records everything a screen asked for, so a test can assert on it. */
    class Recorder {
        val grants = mutableListOf<Boolean>()
        val policies = mutableListOf<NotificationPolicy>()
        var settingsOpened = 0
        var appsLoaded = 0

        /** What the picker is handed. Deliberately not the real device's. */
        var apps: List<NotificationApp> = emptyList()

        fun actions(): MainActions = MainActions(
            onPair = {},
            onConnect = {},
            onDisconnect = {},
            onForget = {},
            onSetFilesGrant = { _, _ -> },
            onSetClipboardGrant = { _, _ -> },
            onSetBatteryGrant = { _, _ -> },
            onSetClipboardPolicy = { _, _ -> },
            onSetNotificationsGrant = { _, granted -> grants += granted },
            onSetNotificationPolicy = { _, policy -> policies += policy },
            onOpenNotificationAccess = { settingsOpened += 1 },
            loadNotificationApps = {
                appsLoaded += 1
                apps
            },
            loadAppIcon = { null },
            onSendClipboard = {},
            onApplyClip = {},
            onDismissClip = {},
            onRespondToOffer = { _, _ -> },
            onCancelTransfer = {},
            onPickFileFor = {},
            readClipboardPreview = { null },
        )
    }

    fun app(
        packageName: String,
        label: String,
        allowed: Boolean = false,
        notifying: Boolean = false,
    ) = NotificationApp(
        packageName = packageName,
        label = label,
        launchable = true,
        notifying = notifying,
        allowed = allowed,
    )
}
