package io.github.yurisismotto.anyflow.ui

import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.core.content.ContextCompat
import androidx.lifecycle.lifecycleScope
import com.journeyapps.barcodescanner.ScanContract
import com.journeyapps.barcodescanner.ScanOptions
import io.github.yurisismotto.anyflow.AnyFlowApp
import io.github.yurisismotto.anyflow.capability.ClipboardCapability
import io.github.yurisismotto.anyflow.capability.FilesCapability
import io.github.yurisismotto.anyflow.clipboard.ClipboardNotifications
import io.github.yurisismotto.anyflow.clipboard.ClipboardPolicy
import io.github.yurisismotto.anyflow.clipboard.ClipboardSendFailed
import io.github.yurisismotto.anyflow.clipboard.ClipboardSync
import io.github.yurisismotto.anyflow.identity.Fingerprint
import io.github.yurisismotto.anyflow.pairing.QrPayload
import io.github.yurisismotto.anyflow.service.ConnectionService
import io.github.yurisismotto.anyflow.store.TrustStore
import kotlinx.coroutines.launch

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
                .onSuccess { ConnectionService.start(this@MainActivity) }
                .onFailure { showError(it.message ?: "Pairing failed.") }
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

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            notificationPermission.launch(Manifest.permission.POST_NOTIFICATIONS)
        }
        handleIntent(intent)

        setContent {
            MaterialTheme {
                Surface(modifier = Modifier.fillMaxSize()) {
                    MainScreen(
                        app = app,
                        onPair = ::requestScan,
                        onConnect = { ConnectionService.start(this) },
                        onDisconnect = { ConnectionService.stop(this) },
                        onForget = { peer ->
                            app.trustStore.removePeer(peer.fingerprint)
                            ConnectionService.stop(this)
                        },
                        onSetFilesGrant = { peer, granted ->
                            app.trustStore.setGrant(peer.fingerprint, FilesCapability.ID, granted)
                        },
                        onSetClipboardGrant = { peer, granted ->
                            app.trustStore
                                .setGrant(peer.fingerprint, ClipboardCapability.ID, granted)
                        },
                        onSetClipboardPolicy = { peer, policy ->
                            app.trustStore.setClipboardPolicy(peer.fingerprint, policy)
                        },
                        onSendClipboard = ::sendClipboard,
                        onApplyClip = ::applyClip,
                        onDismissClip = { peer ->
                            lifecycleScope.launch { app.clipboard.dismissPending(peer) }
                            ClipboardNotifications(this).clear(peer)
                        },
                        pendingRequest = requestedClip,
                        onPendingRequestHandled = { requestedClip = null },
                    )

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
            1 -> sendClipboard(eligible.first().fingerprint)
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

@Composable
private fun MainScreen(
    app: AnyFlowApp,
    onPair: () -> Unit,
    onConnect: () -> Unit,
    onDisconnect: () -> Unit,
    onForget: (TrustStore.TrustedPeer) -> Unit,
    onSetFilesGrant: (TrustStore.TrustedPeer, Boolean) -> Unit,
    onSetClipboardGrant: (TrustStore.TrustedPeer, Boolean) -> Unit,
    onSetClipboardPolicy: (TrustStore.TrustedPeer, ClipboardPolicy) -> Unit,
    onSendClipboard: (Fingerprint, Boolean) -> Unit,
    onApplyClip: (Fingerprint) -> Unit,
    onDismissClip: (Fingerprint) -> Unit,
    pendingRequest: Fingerprint?,
    onPendingRequestHandled: () -> Unit,
) {
    val state by app.connectionState.collectAsState()
    // Observed, not read once. The previous release read the peer list during
    // composition and never again, so a grant changed anywhere else stayed
    // invisible until the screen was recreated.
    val peers by app.trustStore.peersFlow.collectAsState()
    val offers by app.files.pendingOffers.collectAsState()
    val transfers by app.files.visible.collectAsState()
    val pendingClips by app.clipboard.pendingClips.collectAsState()
    val clipboardOutcomes by app.clipboard.lastOutcome.collectAsState()

    val connectedPeer = (state as? AnyFlowApp.ConnectionState.Connected)?.fingerprintShort

    Scaffold { padding ->
        Column(
            modifier = Modifier
                .padding(padding)
                .padding(16.dp)
                .fillMaxSize()
                .verticalScroll(rememberScrollState()),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Text("AnyFlow", style = MaterialTheme.typography.headlineSmall)

            Card {
                Column(Modifier.padding(12.dp), Arrangement.spacedBy(4.dp)) {
                    Text("This device", style = MaterialTheme.typography.titleMedium)
                    Text(app.trustStore.deviceName)
                    // Shown so the user can compare it against the computer's
                    // screen during pairing.
                    Text("Fingerprint ${app.identity.fingerprint.toDisplayShort()}")
                    Text(
                        if (app.identity.isStrongBoxBacked) {
                            "Key stored in secure element"
                        } else {
                            "Key stored in hardware-backed keystore"
                        },
                        style = MaterialTheme.typography.bodySmall,
                    )
                }
            }

            // Anything waiting on the person comes first.
            for (offer in offers) {
                IncomingOfferCard(
                    offer = offer,
                    onRespond = { accept -> app.files.respondToOffer(offer.transferId, accept) },
                )
            }

            for (clip in pendingClips) {
                PendingClipCard(
                    clip = clip,
                    onApply = { onApplyClip(clip.peer) },
                    onDismiss = { onDismissClip(clip.peer) },
                )
            }

            // A notification tapped while a clip is already gone should not
            // leave the request hanging around forever.
            if (pendingRequest != null && pendingClips.none { it.peer == pendingRequest }) {
                onPendingRequestHandled()
            }

            for (transfer in transfers) {
                TransferRow(
                    transfer = transfer,
                    onCancel = { app.files.cancel(transfer.transferId) },
                )
            }

            Card {
                Column(Modifier.padding(12.dp), Arrangement.spacedBy(4.dp)) {
                    Text("Status", style = MaterialTheme.typography.titleMedium)
                    Text(
                        when (val s = state) {
                            is AnyFlowApp.ConnectionState.Idle -> "Not connected"
                            is AnyFlowApp.ConnectionState.Connecting -> "Connecting…"
                            is AnyFlowApp.ConnectionState.Connected ->
                                "Connected to ${s.deviceName} (${s.fingerprintShort})"
                            // Shown separately from Error so the user can see
                            // that the app is still working on it.
                            is AnyFlowApp.ConnectionState.Retrying ->
                                "Reconnecting in ${s.inSeconds}s (${s.reason})"
                            is AnyFlowApp.ConnectionState.Error -> "Error: ${s.message}"
                        },
                    )
                    app.battery.remoteReading()?.let { reading ->
                        Text("Computer battery: ${reading.percentage}%")
                    }
                }
            }

            if (peers.isEmpty()) {
                Text("No computer paired yet. Run `anyflow pair` on Fedora, then scan the code.")
                Button(onClick = onPair) { Text("Scan pairing code") }
            } else {
                for (peer in peers) {
                    val connected = connectedPeer == peer.fingerprint.toDisplayShort()
                    Card {
                        Column(Modifier.padding(12.dp), Arrangement.spacedBy(4.dp)) {
                            Text(peer.deviceName, style = MaterialTheme.typography.titleMedium)
                            Text("Fingerprint ${peer.fingerprint.toDisplayShort()}")

                            val filesAllowed = peer.allows(FilesCapability.ID)
                            OutlinedButton(onClick = { onSetFilesGrant(peer, !filesAllowed) }) {
                                Text(
                                    if (filesAllowed) {
                                        "Stop allowing file transfer"
                                    } else {
                                        "Allow file transfer"
                                    },
                                )
                            }

                            ClipboardSection(
                                peer = peer,
                                connected = connected,
                                lastOutcome = clipboardOutcomes[peer.fingerprint.toHex()],
                                onSetGrant = { granted -> onSetClipboardGrant(peer, granted) },
                                onSetPolicy = { policy -> onSetClipboardPolicy(peer, policy) },
                                onSendClipboard = { onSendClipboard(peer.fingerprint, false) },
                            )

                            OutlinedButton(onClick = { onForget(peer) }) {
                                Text("Forget this computer")
                            }
                        }
                    }
                }
                Button(onClick = onConnect) { Text("Connect") }
                OutlinedButton(onClick = onDisconnect) { Text("Disconnect") }
            }
        }
    }
}
