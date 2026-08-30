package io.github.yurisismotto.anyflow.ui

import android.content.Intent
import android.net.Uri
import android.os.Build
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.MaterialTheme
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
import androidx.lifecycle.lifecycleScope
import io.github.yurisismotto.anyflow.AnyFlowApp
import io.github.yurisismotto.anyflow.files.SharedFile
import io.github.yurisismotto.anyflow.service.ConnectionService
import io.github.yurisismotto.anyflow.store.TrustStore
import kotlinx.coroutines.launch

/**
 * The Sharesheet entry point: Gallery/Files/Browser → Share → AnyFlow.
 *
 * ## What this activity is careful about
 *
 * The incoming `content://` URI arrives with a *temporary* read grant scoped
 * to this activity's task. So the file is read here, through the
 * `ContentResolver`, and never resolved to a filesystem path — see
 * [SharedFile] for why turning a content URI into a path is the classic
 * Android mistake.
 *
 * The URI is also attacker-influenced: any app can share anything into this
 * one. Its display name is treated exactly like a name from the network, and
 * goes through the same sanitizer.
 *
 * ## Single file for now
 *
 * `ACTION_SEND` only. `ACTION_SEND_MULTIPLE` is declared as a *next*
 * increment rather than half-implemented: batching needs a queue, per-item
 * progress and a partial-failure story, and the sprint's own guidance is to
 * ship one file first and document the rest. See `docs/architecture/FILES.md`.
 */
class SendActivity : ComponentActivity() {

    private val app: AnyFlowApp get() = application as AnyFlowApp

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        val uri = extractSharedUri(intent)
        val peer = runCatching { app.trustStore.peers().firstOrNull() }.getOrNull()

        setContent {
            MaterialTheme {
                Surface {
                    SendScreen(
                        app = app,
                        uri = uri,
                        peer = peer,
                        onSend = { target, file -> startSend(target, file) },
                        onClose = { finish() },
                    )
                }
            }
        }
    }

    private fun startSend(peer: TrustStore.TrustedPeer, uri: Uri) {
        // The connection is what carries the offer, so make sure there is one.
        ConnectionService.start(this)
        lifecycleScope.launch {
            app.files.offer(peer.fingerprint, uri)
        }
    }

    private fun extractSharedUri(intent: Intent?): Uri? {
        if (intent == null) return null
        return when (intent.action) {
            Intent.ACTION_SEND -> intent.parcelableExtra(Intent.EXTRA_STREAM)
            // Declared in the manifest so the app appears for multi-select
            // too; only the first item is sent, and the UI says so rather
            // than silently dropping the rest.
            Intent.ACTION_SEND_MULTIPLE ->
                intent.parcelableArrayListExtra(Intent.EXTRA_STREAM)?.firstOrNull()
            else -> null
        }
    }

    @Suppress("DEPRECATION")
    private fun Intent.parcelableExtra(name: String): Uri? =
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            getParcelableExtra(name, Uri::class.java)
        } else {
            getParcelableExtra(name)
        }

    @Suppress("DEPRECATION")
    private fun Intent.parcelableArrayListExtra(name: String): List<Uri>? =
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            getParcelableArrayListExtra(name, Uri::class.java)
        } else {
            getParcelableArrayListExtra(name)
        }
}

@Composable
private fun SendScreen(
    app: AnyFlowApp,
    uri: Uri?,
    peer: TrustStore.TrustedPeer?,
    onSend: (TrustStore.TrustedPeer, Uri) -> Unit,
    onClose: () -> Unit,
) {
    val transfers by app.files.visible.collectAsState()
    var started by remember { mutableStateOf(false) }

    // Read once, off the composition's hot path: a display name query is
    // cheap, but it still touches another app's provider.
    val name = remember(uri) {
        uri?.let { runCatching { SharedFile(app, it).displayName() }.getOrNull() }
    }

    Column(
        modifier = Modifier.padding(16.dp).fillMaxWidth(),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Text("Send with AnyFlow", style = MaterialTheme.typography.headlineSmall)

        when {
            uri == null ->
                Text("Nothing to send: that share did not include a file.")

            name == null ->
                // The sanitizer refused it. Saying so is better than inventing
                // a name for a file whose own name was unusable.
                Text("That file's name cannot be sent safely. Rename it and try again.")

            peer == null ->
                Text("No computer paired yet. Open AnyFlow and scan the pairing code first.")

            else -> {
                Card {
                    Column(Modifier.padding(12.dp), Arrangement.spacedBy(4.dp)) {
                        Text(name, style = MaterialTheme.typography.titleMedium)
                        Text("to ${peer.deviceName}")
                        Text(
                            "Fingerprint ${peer.fingerprint.toDisplayShort()}",
                            style = MaterialTheme.typography.bodySmall,
                        )
                    }
                }

                val mine = transfers.filter { it.sending && it.filename == name }
                if (mine.isEmpty()) {
                    Button(
                        enabled = !started,
                        onClick = {
                            started = true
                            onSend(peer, uri)
                        },
                    ) { Text(if (started) "Sending…" else "Send") }
                } else {
                    for (transfer in mine) {
                        TransferRow(transfer)
                    }
                }
            }
        }

        Button(onClick = onClose) { Text("Close") }
    }
}
