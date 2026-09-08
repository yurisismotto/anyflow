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
import io.github.yurisismotto.anyflow.capability.ClipboardCapability
import io.github.yurisismotto.anyflow.clipboard.ClipboardText
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
 *
 * ## Shared text goes to `clipboard.v1`, not `files.v1`
 *
 * A `text/plain` share carries its text in `EXTRA_TEXT`, in the intent
 * itself. Two consequences, and both are worth stating:
 *
 *  * it never touches the system clipboard, so the Android 10 focus
 *    restriction on `getPrimaryClip` does not apply — this is the one path
 *    where text can reach a computer without the person first copying it;
 *  * an intent carries no `EXTRA_IS_SENSITIVE`, so shared text is sent
 *    without the sensitive hint. That is honest rather than convenient: we
 *    do not know, and guessing from the content would be exactly the
 *    heuristic-password-detector this project refuses to build.
 *
 * This complements the Send clipboard button; it does not replace it.
 */
class SendActivity : ComponentActivity() {

    private val app: AnyFlowApp get() = application as AnyFlowApp

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        val uri = extractSharedUri(intent)
        val sharedText = extractSharedText(intent)
        val peer = runCatching { app.trustStore.peers().firstOrNull() }.getOrNull()

        setContent {
            MaterialTheme {
                Surface {
                    if (sharedText != null && uri == null) {
                        SendTextScreen(
                            text = sharedText,
                            peer = peer,
                            onSend = { target, text, onOutcome -> startTextSend(target, text, onOutcome) },
                            onClose = { finish() },
                        )
                    } else {
                        SendScreen(
                            app = app,
                            uri = uri,
                            peer = peer,
                            onSend = { target, file, onOutcome -> startSend(target, file, onOutcome) },
                            onClose = { finish() },
                        )
                    }
                }
            }
        }
    }

    /**
     * Sends shared text as a clipboard update.
     *
     * `sensitive = false`: an `ACTION_SEND` intent carries no sensitivity
     * hint, and inferring one from the text would be a heuristic dressed up
     * as a security control.
     */
    private fun startTextSend(
        peer: TrustStore.TrustedPeer,
        text: ClipboardText,
        onOutcome: (UiMapping.SendAttempt) -> Unit,
    ) {
        ConnectionService.start(this)
        lifecycleScope.launch {
            app.clipboard.sendText(peer.fingerprint, text, sensitive = false)
                .onSuccess {
                    android.widget.Toast.makeText(
                        this@SendActivity,
                        "Sent ${'$'}{it} bytes to ${'$'}{peer.deviceName}.",
                        android.widget.Toast.LENGTH_SHORT,
                    ).show()
                    onOutcome(UiMapping.SendAttempt.Sent)
                    finish()
                }
                .onFailure { failure ->
                    // The toast says what happened; the button has to come
                    // back, or this screen has the same dead end issue #12
                    // described, one function along.
                    val outcome = UiMapping.SendAttempt.Failed(
                        failure.message ?: "Could not send that text.",
                    )
                    android.widget.Toast.makeText(
                        this@SendActivity,
                        outcome.message,
                        android.widget.Toast.LENGTH_LONG,
                    ).show()
                    onOutcome(outcome)
                }
        }
    }

    /**
     * The text of a `text/plain` share, if there is usable text.
     *
     * Validated here rather than at send time so the screen can say *why*
     * something cannot be sent before offering a button that would fail.
     */
    private fun extractSharedText(intent: Intent?): ClipboardText? {
        if (intent?.action != Intent.ACTION_SEND) return null
        val raw = intent.getCharSequenceExtra(Intent.EXTRA_TEXT) ?: return null
        return ClipboardText.validate(raw).getOrNull()
    }

    /**
     * Offers the shared file, and reports the outcome back to the screen.
     *
     * The `Result` used to be discarded (issue #12). Every early refusal
     * inside `offer` — not connected, not granted, too many at once, a name
     * the sanitizer will not pass, a file that cannot be read — returns
     * before a transfer row exists, so nothing appeared in `files.visible`
     * and the button sat on "Sending…" until the person killed the screen.
     * The offer had not been made and never would be; only the UI thought
     * otherwise.
     *
     * [onOutcome] therefore runs on every path, and the states it can carry
     * are terminal in both directions.
     *
     * It is safe against the Activity going away underneath it: this runs in
     * `lifecycleScope`, which is cancelled at `onDestroy`, so the
     * continuation after `offer` does not resume into a dead composition. No
     * cancellation is invented here either — a cancelled scope means the
     * screen is gone, and there is nothing left to tell.
     */
    private fun startSend(
        peer: TrustStore.TrustedPeer,
        uri: Uri,
        onOutcome: (UiMapping.SendAttempt) -> Unit,
    ) {
        // The connection is what carries the offer, so make sure there is one.
        ConnectionService.start(this)
        lifecycleScope.launch {
            // Nothing about the file is logged here: the message is the
            // capability's own words, and the throwable is never printed.
            onOutcome(UiMapping.sendOutcome(app.files.offer(peer.fingerprint, uri)))
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
    onSend: (TrustStore.TrustedPeer, Uri, (UiMapping.SendAttempt) -> Unit) -> Unit,
    onClose: () -> Unit,
) {
    val transfers by app.files.visible.collectAsState()
    var attempt by remember { mutableStateOf<UiMapping.SendAttempt>(UiMapping.SendAttempt.Idle) }

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
                    // The failure is stated above the button rather than in a
                    // toast: a toast on a Sharesheet is gone before the person
                    // has finished reading it, and the button beneath it is
                    // the retry.
                    (attempt as? UiMapping.SendAttempt.Failed)?.let {
                        Text(it.message, color = MaterialTheme.colorScheme.error)
                    }
                    Button(
                        enabled = attempt.canSend,
                        onClick = {
                            attempt = UiMapping.SendAttempt.Sending
                            onSend(peer, uri) { outcome -> attempt = outcome }
                        },
                    ) { Text(UiMapping.sendButtonLabel(attempt)) }
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

/**
 * The text half of the Sharesheet: "Share → AnyFlow" from a browser or a
 * notes app.
 *
 * The text is *not* rendered. Showing a preview would put whatever was shared
 * — which may well be a password someone selected in a manager — on a screen
 * that is also visible over the shoulder, and it buys nothing: the person
 * just selected it and knows what it is. The size is shown instead.
 */
@Composable
private fun SendTextScreen(
    text: ClipboardText,
    peer: TrustStore.TrustedPeer?,
    onSend: (TrustStore.TrustedPeer, ClipboardText, (UiMapping.SendAttempt) -> Unit) -> Unit,
    onClose: () -> Unit,
) {
    var attempt by remember { mutableStateOf<UiMapping.SendAttempt>(UiMapping.SendAttempt.Idle) }

    Column(
        modifier = Modifier.padding(16.dp).fillMaxWidth(),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Text("Send text with AnyFlow", style = MaterialTheme.typography.headlineSmall)

        when {
            peer == null ->
                Text("No computer paired yet. Open AnyFlow and scan the pairing code first.")

            !peer.allows(ClipboardCapability.ID) ->
                Text(
                    "Clipboard sharing is off for ${'$'}{peer.deviceName}. Turn on " +
                        "\"Share clipboard with this computer\" in AnyFlow first.",
                )

            !peer.clipboardPolicy.allowSend ->
                Text("Sending your clipboard to ${'$'}{peer.deviceName} is turned off.")

            else -> {
                Card {
                    Column(Modifier.padding(12.dp), Arrangement.spacedBy(4.dp)) {
                        Text(
                            "${'$'}{text.byteLength} bytes of text",
                            style = MaterialTheme.typography.titleMedium,
                        )
                        Text("to ${'$'}{peer.deviceName}")
                        Text(
                            "Fingerprint ${'$'}{peer.fingerprint.toDisplayShort()}",
                            style = MaterialTheme.typography.bodySmall,
                        )
                    }
                }
                (attempt as? UiMapping.SendAttempt.Failed)?.let {
                    Text(it.message, color = MaterialTheme.colorScheme.error)
                }
                Button(
                    enabled = attempt.canSend,
                    onClick = {
                        attempt = UiMapping.SendAttempt.Sending
                        onSend(peer, text) { outcome -> attempt = outcome }
                    },
                ) { Text(UiMapping.sendButtonLabel(attempt)) }
            }
        }

        Button(onClick = onClose) { Text("Close") }
    }
}
