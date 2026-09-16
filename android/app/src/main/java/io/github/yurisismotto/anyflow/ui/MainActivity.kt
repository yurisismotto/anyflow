package io.github.yurisismotto.anyflow.ui

import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.Surface
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.core.content.ContextCompat
import androidx.lifecycle.lifecycleScope
import com.journeyapps.barcodescanner.ScanContract
import com.journeyapps.barcodescanner.ScanOptions
import io.github.yurisismotto.anyflow.AnyFlowApp
import io.github.yurisismotto.anyflow.capability.BatteryCapability
import io.github.yurisismotto.anyflow.capability.ClipboardCapability
import io.github.yurisismotto.anyflow.capability.FilesCapability
import io.github.yurisismotto.anyflow.capability.NotificationsCapability
import io.github.yurisismotto.anyflow.clipboard.ClipboardNotifications
import io.github.yurisismotto.anyflow.clipboard.ClipboardPolicy
import io.github.yurisismotto.anyflow.clipboard.ClipboardSendFailed
import io.github.yurisismotto.anyflow.clipboard.ClipboardSync
import io.github.yurisismotto.anyflow.clipboard.SystemClipboard
import io.github.yurisismotto.anyflow.identity.Fingerprint
import io.github.yurisismotto.anyflow.notifications.InstalledApps
import io.github.yurisismotto.anyflow.notifications.NotificationAccess
import io.github.yurisismotto.anyflow.pairing.QrPayload
import io.github.yurisismotto.anyflow.service.ConnectionService
import io.github.yurisismotto.anyflow.store.TrustStore
import io.github.yurisismotto.anyflow.ui.theme.AnyFlowTheme
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/**
 * Identity, pairing, connection status, transfers and clipboard.
 *
 * ## Why the clipboard is read here and nowhere else
 *
 * Android refuses `getPrimaryClip` to an app without input focus. An Activity
 * that the person is looking at has it; a service, a tile or a broadcast
 * receiver does not. So every clipboard *read* in this app originates from a
 * button on this screen — which is also the honest place for it, because
 * sending a clipboard is a decision, not a background sync.
 */
class MainActivity : ComponentActivity() {

    private val app: AnyFlowApp get() = application as AnyFlowApp

    private val scanLauncher = registerForActivityResult(ScanContract()) { result ->
        val contents = result.contents ?: return@registerForActivityResult
        val payload = QrPayload.parse(contents)
        if (payload == null) {
            // Never echo the scanned text back to the screen: it may contain
            // a pairing token, and it is attacker-supplied either way.
            showError("That QR code is not a AnyFlow pairing code.")
            return@registerForActivityResult
        }
        lifecycleScope.launch {
            app.pair(payload)
                // The computer that was just paired becomes the target. It is
                // the only reading of "scan this code" that is not a guess,
                // and it is what makes pairing a second desktop work: without
                // it the new peer would be trusted and unreachable behind
                // whichever entry the store happened to hold first.
                .onSuccess { peer ->
                    ConnectionService.start(this@MainActivity, peer.fingerprint)
                }
                .onFailure { showError(it.message ?: "Pairing failed.") }
        }
    }

    /** The peer a file picked from the system picker should be offered to. */
    private var pendingFileTarget: Fingerprint? = null

    private val filePicker =
        registerForActivityResult(ActivityResultContracts.OpenDocument()) { uri: Uri? ->
            val peer = pendingFileTarget
            pendingFileTarget = null
            if (uri == null || peer == null) return@registerForActivityResult
            lifecycleScope.launch {
                app.files.offer(peer, uri)
                    // Same mapping as the Sharesheet: a platform exception
                    // from a content provider must not put the URI on screen.
                    .onFailure { showError(UiMapping.sendFailureMessage(it)) }
            }
        }

    private val notificationPermission =
        registerForActivityResult(ActivityResultContracts.RequestPermission()) { }

    private val cameraPermission =
        registerForActivityResult(ActivityResultContracts.RequestPermission()) { granted ->
            if (granted) launchScanner() else showError("Camera access is needed to scan the code.")
        }

    /** Set when a send needs the sensitive-clip confirmation. */
    private var sensitivePrompt by mutableStateOf<SensitivePrompt?>(null)

    private data class SensitivePrompt(
        val peer: Fingerprint,
        val computerName: String,
        val bytes: Int,
    )

    /**
     * A clip the notification asked us to offer, if the Activity was started
     * from one.
     */
    private var requestedClip by mutableStateOf<Fingerprint?>(null)

    /**
     * Android's notification access, as last read from the platform.
     *
     * Held here rather than inside a screen because it has to be re-read on
     * [onResume]: the permission is granted in Android's own settings, and
     * the only thing this app learns by being resumed is that the person came
     * back — **not** that they said yes. A consent screen that assumed
     * otherwise would claim to be working while reading nothing.
     */
    private var notificationAccess by mutableStateOf(false)

    /**
     * Whether a second profile exists on this device.
     *
     * Decides only whether the work-profile switch is offered or explained as
     * inapplicable. Read once: a work profile is not created while an app is
     * in the foreground.
     */
    private val hasWorkProfile: Boolean by lazy {
        runCatching {
            (getSystemService(android.os.UserManager::class.java)?.userProfiles?.size ?: 1) > 1
        }.getOrDefault(false)
    }

    /** The app-picker's source of applications. Binder calls, off this thread. */
    private val installedApps by lazy { InstalledApps(this) }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            notificationPermission.launch(Manifest.permission.POST_NOTIFICATIONS)
        }
        handleIntent(intent)

        setContent {
            AnyFlowTheme {
                Surface(
                    modifier = Modifier.fillMaxSize(),
                    color = AnyFlowTheme.colors.background,
                ) {
                    AnyFlowShell(state = rememberMainUiState(), actions = rememberMainActions())

                    sensitivePrompt?.let { prompt ->
                        SensitiveClipDialog(
                            computerName = prompt.computerName,
                            bytes = prompt.bytes,
                            onConfirm = {
                                sensitivePrompt = null
                                sendClipboard(prompt.peer, confirmedSensitive = true)
                            },
                            onDismiss = { sensitivePrompt = null },
                        )
                    }
                }
            }
        }
    }

    /**
     * Re-reads the permission state Android owns.
     *
     * Called on every resume, which covers the one path that matters: the
     * person went to Settings and came back. It is also the path that covers
     * a *revocation* made there, which is why it re-reads rather than only
     * checking when it expects a grant.
     */
    override fun onResume() {
        super.onResume()
        notificationAccess = NotificationAccess.isGranted(this)
    }

    /** Collects every observable the UI draws from into one snapshot. */
    @Composable
    private fun rememberMainUiState(): MainUiState {
        // Observed, not read once. An earlier release read the peer list during
        // composition and never again, so a grant changed anywhere else stayed
        // invisible until the screen was recreated.
        val connection by app.connectionState.collectAsState()
        val peers by app.trustStore.peersFlow.collectAsState()
        // Observed for the same reason the peer list is: the connection
        // service and the share sheet both write the choice, so a value read
        // once here would go stale the moment either of them re-pointed the
        // link.
        val selectedPeerHex by app.trustStore.selectedPeerFlow.collectAsState()
        val offers by app.files.pendingOffers.collectAsState()
        val transfers by app.files.visible.collectAsState()
        val pendingClips by app.clipboard.pendingClips.collectAsState()
        val outcomes by app.clipboard.lastOutcome.collectAsState()
        val notifications by app.notifications.status.collectAsState()
        // The capability republishes on every bind, unbind and revocation, so
        // a permission taken away while this screen is open is picked up here
        // as well as on the next resume.
        LaunchedEffect(notifications) {
            notificationAccess = NotificationAccess.isGranted(this@MainActivity)
        }
        return MainUiState(
            ownDeviceName = app.trustStore.deviceName,
            ownFingerprint = app.identity.fingerprint.toDisplayShort(),
            keyBackingDescription = if (app.identity.isStrongBoxBacked) {
                "Key stored in a secure element"
            } else {
                "Key stored in the hardware-backed keystore"
            },
            connection = connection,
            peers = peers,
            selectedPeerHex = selectedPeerHex,
            offers = offers,
            transfers = transfers,
            pendingClips = pendingClips,
            clipboardOutcomes = outcomes,
            remoteBatteryPercent = app.battery.remoteReading()?.percentage,
            notificationAccessGranted = notificationAccess,
            notifications = notifications,
            hasWorkProfile = hasWorkProfile,
        )
    }

    @Composable
    private fun rememberMainActions(): MainActions = MainActions(
        onPair = ::requestScan,
        // The fingerprint of the row that was tapped, carried into the start
        // intent. This is the seam the certified defect fell through: the
        // action used to take nothing, and the service picked a peer itself.
        onConnect = { peer -> ConnectionService.start(this, peer) },
        onDisconnect = { ConnectionService.stop(this) },
        onForget = { peer ->
            // `removePeer` drops the choice with the computer, so the next
            // connection cannot be aimed at something no longer trusted.
            app.trustStore.removePeer(peer.fingerprint)
            ConnectionService.stop(this)
        },
        onSetFilesGrant = { peer, granted ->
            app.trustStore.setGrant(peer.fingerprint, FilesCapability.ID, granted)
        },
        onSetClipboardGrant = { peer, granted ->
            app.trustStore.setGrant(peer.fingerprint, ClipboardCapability.ID, granted)
        },
        onSetBatteryGrant = { peer, granted ->
            app.trustStore.setGrant(peer.fingerprint, BatteryCapability.ID, granted)
        },
        onSetClipboardPolicy = { peer, policy ->
            app.trustStore.setClipboardPolicy(peer.fingerprint, policy)
        },
        onSetNotificationsGrant = { peer, granted ->
            // The canonical grant, in the canonical place. Withdrawing it here
            // makes `notificationPolicyFor` answer DENIED on the very next
            // notification, narrows the announced role on the session that is
            // already up, and lets the listener unbind when no eligible peer
            // is left — none of which needs a second permission database.
            app.trustStore.setGrant(peer.fingerprint, NotificationsCapability.ID, granted)
            app.notifications.policyChanged()
        },
        onSetNotificationPolicy = { peer, policy ->
            app.trustStore.setNotificationPolicy(peer.fingerprint, policy)
            // Re-evaluates the listener binding: turning mirroring off for the
            // last eligible computer releases the listener rather than leaving
            // it bound and reading.
            app.notifications.policyChanged()
        },
        onOpenNotificationAccess = {
            runCatching { startActivity(NotificationAccess.settingsIntent(this)) }
                .onFailure { showError("Could not open Android's notification settings.") }
        },
        loadNotificationApps = { peer ->
            installedApps.load(
                allowed = peer.notificationPolicy.allowedApps,
                // Package names from the shade, and only names. Empty when the
                // listener is not bound, which is the ordinary idle state.
                notifying = withContext(Dispatchers.IO) { app.notifications.activePackages() },
            )
        },
        loadAppIcon = { packageName -> installedApps.iconFor(packageName) },
        onSendClipboard = { peer -> sendClipboard(peer) },
        onApplyClip = ::applyClip,
        onDismissClip = { peer ->
            lifecycleScope.launch { app.clipboard.dismissPending(peer) }
            ClipboardNotifications(this).clear(peer)
        },
        onRespondToOffer = { transferId, accept -> app.files.respondToOffer(transferId, accept) },
        onCancelTransfer = { transferId -> app.files.cancel(transferId) },
        onPickFileFor = { peer ->
            pendingFileTarget = peer
            // ACTION_OPEN_DOCUMENT, exactly like the Sharesheet path: the URI
            // arrives with a temporary read grant for that one item, which is
            // why no storage permission is declared anywhere in the manifest.
            filePicker.launch(arrayOf("*/*"))
        },
        readClipboardPreview = ::readClipboardPreview,
    )

    /**
     * Reads the clipboard so the send screen can show what is about to leave.
     *
     * A clip the source app marked sensitive comes back with its text
     * withheld: `EXTRA_IS_SENSITIVE` exists so that a preview does not put a
     * password on screen, and the size is enough to decide with. Nothing here
     * is logged or stored.
     */
    private fun readClipboardPreview(): ClipboardPreview? =
        SystemClipboard(this).read().fold(
            onSuccess = { clip ->
                ClipboardPreview(
                    text = if (clip.sensitive) null else clip.text.text,
                    bytes = clip.text.byteLength,
                    sensitive = clip.sensitive,
                )
            },
            onFailure = { null },
        )

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        handleIntent(intent)
    }

    private fun handleIntent(intent: Intent?) {
        when (intent?.action) {
            ClipboardNotifications.ACTION_APPLY_CLIP -> {
                val hex = intent.getStringExtra(ClipboardNotifications.EXTRA_PEER_FINGERPRINT)
                    ?: return
                requestedClip = Fingerprint.fromHex(hex)
            }

            ACTION_SEND_CLIPBOARD -> sendClipboardFromShortcut()
        }
    }

    /**
     * The Quick Settings tile asked for a send.
     *
     * The clipboard is read here rather than in the tile because this is
     * where the app has input focus, which is what Android requires. The
     * target is resolved without guessing: with exactly one eligible computer
     * the send starts, and with none or several the screen simply opens so
     * the person picks. Sending a password to the wrong computer because
     * something chose for them is not a failure mode worth having — the same
     * rule the CLI follows for an ambiguous device prefix.
     */
    private fun sendClipboardFromShortcut() {
        val eligible = runCatching {
            app.trustStore.peers().filter {
                it.allows(ClipboardCapability.ID) && it.clipboardPolicy.allowSend
            }
        }.getOrDefault(emptyList())

        when (eligible.size) {
            0 -> showError("No computer is set up to receive your clipboard.")
            // `single`, not `first`: the branch is already guarded by the size,
            // and spelling it this way means no peer-selection call site in the
            // app can be read as "whichever one is first".
            1 -> sendClipboard(eligible.single().fingerprint)
            else -> showError("Choose which computer to send the clipboard to.")
        }
    }

    /**
     * Reads the clipboard and sends it.
     *
     * This runs with the Activity in the foreground, which is the only state
     * in which Android permits the read. A clip the platform marked sensitive
     * comes back as [ClipboardSync.SendFailure.NeedsConfirmation] and is not
     * sent until the person answers the dialog.
     */
    private fun sendClipboard(peer: Fingerprint, confirmedSensitive: Boolean = false) {
        lifecycleScope.launch {
            val name = app.trustStore.peer(peer)?.deviceName ?: "the computer"
            app.clipboard.sendCurrentClipboard(peer, confirmedSensitive)
                .onSuccess { bytes -> showError("Sent $bytes bytes to $name.") }
                .onFailure { failure ->
                    when (val reason = (failure as? ClipboardSendFailed)?.failure) {
                        is ClipboardSync.SendFailure.NeedsConfirmation ->
                            sensitivePrompt = SensitivePrompt(peer, name, reason.bytes)
                        // Every other failure carries a message that names the
                        // cause without naming the content.
                        else -> showError(failure.message ?: "Could not send the clipboard.")
                    }
                }
        }
    }

    private fun applyClip(peer: Fingerprint) {
        lifecycleScope.launch {
            app.clipboard.applyPending(peer)
                .onSuccess { bytes ->
                    ClipboardNotifications(this@MainActivity).clear(peer)
                    showError("Copied $bytes bytes to your clipboard.")
                }
                .onFailure { showError(it.message ?: "Could not copy that clipboard.") }
        }
    }

    private fun requestScan() {
        val granted = ContextCompat.checkSelfPermission(this, Manifest.permission.CAMERA) ==
            PackageManager.PERMISSION_GRANTED
        if (granted) launchScanner() else cameraPermission.launch(Manifest.permission.CAMERA)
    }

    private fun launchScanner() {
        scanLauncher.launch(
            ScanOptions()
                .setDesiredBarcodeFormats(ScanOptions.QR_CODE)
                .setPrompt("Point at the QR code shown by `anyflow pair`")
                .setBeepEnabled(false),
        )
    }

    /**
     * One-line feedback.
     *
     * Callers must never pass clipboard text: a toast is on screen and in the
     * accessibility event stream. Every call site here passes a count and a
     * device name.
     */
    private fun showError(message: String) {
        android.widget.Toast.makeText(this, message, android.widget.Toast.LENGTH_LONG).show()
    }

    companion object {
        /** Asks this screen to send the clipboard as soon as it has focus. */
        const val ACTION_SEND_CLIPBOARD = "io.github.yurisismotto.anyflow.SEND_CLIPBOARD"
    }
}
