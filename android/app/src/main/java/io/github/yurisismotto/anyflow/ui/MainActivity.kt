package io.github.yurisismotto.anyflow.ui

import android.Manifest
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
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.core.content.ContextCompat
import androidx.lifecycle.lifecycleScope
import com.journeyapps.barcodescanner.ScanContract
import com.journeyapps.barcodescanner.ScanOptions
import io.github.yurisismotto.anyflow.AnyFlowApp
import io.github.yurisismotto.anyflow.pairing.QrPayload
import io.github.yurisismotto.anyflow.service.ConnectionService
import io.github.yurisismotto.anyflow.store.TrustStore
import kotlinx.coroutines.launch

/**
 * The whole UI for this Sprint: identity, pairing, connection status.
 *
 * Compose, kept minimal on purpose — the foundation is the protocol, and a
 * larger UI now would be guesswork about features that do not exist yet.
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

    private var error: String? = null

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            notificationPermission.launch(Manifest.permission.POST_NOTIFICATIONS)
        }

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
                    )
                }
            }
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

    private fun showError(message: String) {
        error = message
        android.widget.Toast.makeText(this, message, android.widget.Toast.LENGTH_LONG).show()
    }
}

@Composable
private fun MainScreen(
    app: AnyFlowApp,
    onPair: () -> Unit,
    onConnect: () -> Unit,
    onDisconnect: () -> Unit,
    onForget: (TrustStore.TrustedPeer) -> Unit,
) {
    val state by app.connectionState.collectAsState()
    val peers = app.trustStore.peers()

    Scaffold { padding ->
        Column(
            modifier = Modifier.padding(padding).padding(16.dp).fillMaxSize(),
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

            Card {
                Column(Modifier.padding(12.dp), Arrangement.spacedBy(4.dp)) {
                    Text("Status", style = MaterialTheme.typography.titleMedium)
                    Text(
                        when (val s = state) {
                            is AnyFlowApp.ConnectionState.Idle -> "Not connected"
                            is AnyFlowApp.ConnectionState.Connecting -> "Connecting…"
                            is AnyFlowApp.ConnectionState.Connected ->
                                "Connected to ${s.deviceName} (${s.fingerprintShort})"
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
                    Card {
                        Column(Modifier.padding(12.dp), Arrangement.spacedBy(4.dp)) {
                            Text(peer.deviceName, style = MaterialTheme.typography.titleMedium)
                            Text("Fingerprint ${peer.fingerprint.toDisplayShort()}")
                            Text("Allowed: ${peer.grantedCapabilities.joinToString(", ")}")
                            Button(onClick = { onForget(peer) }) { Text("Forget this computer") }
                        }
                    }
                }
                Button(onClick = onConnect) { Text("Connect") }
                Button(onClick = onDisconnect) { Text("Disconnect") }
            }
        }
    }
}
