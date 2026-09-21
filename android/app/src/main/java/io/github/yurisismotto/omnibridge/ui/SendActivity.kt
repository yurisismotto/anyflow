package io.github.yurisismotto.omnibridge.ui

import android.content.Intent
import android.net.Uri
import android.os.Build
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.selection.selectable
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.dp
import androidx.lifecycle.lifecycleScope
import io.github.yurisismotto.omnibridge.OmniBridgeApp
import io.github.yurisismotto.omnibridge.capability.ClipboardCapability
import io.github.yurisismotto.omnibridge.clipboard.ClipboardLimits
import io.github.yurisismotto.omnibridge.clipboard.ClipboardText
import io.github.yurisismotto.omnibridge.files.SharedFile
import io.github.yurisismotto.omnibridge.service.ConnectionService
import io.github.yurisismotto.omnibridge.store.PeerTarget
import io.github.yurisismotto.omnibridge.store.TrustStore
import kotlinx.coroutines.launch

/**
 * The Sharesheet entry point: Gallery/Files/Browser → Share → OmniBridge.
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
 *
 * ## Where the share goes
 *
 * It used to go to `trustStore.peers().firstOrNull()` — the other half of the
 * certified U2 §39.17 defect, and the half with a data-routing consequence: a
 * file shared from another app was delivered to whichever computer the trust
 * store happened to list first, not the one the person was using. With an
 * offline desktop in that slot the share simply failed; with a *second* live
 * desktop it would have succeeded against the wrong machine.
 *
 * The destination is now resolved by [PeerTarget] over the peers that can
 * actually receive this kind of share, and the rule is the app's existing one
 * for ambiguity: one eligible computer is used, several are offered as a
 * choice, none is stated plainly. Nothing here is decided by list position.
 */
class SendActivity : ComponentActivity() {

    private val app: OmniBridgeApp get() = application as OmniBridgeApp

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        val uri = extractSharedUri(intent)
        val sharedText = extractSharedText(intent)

        setContent {
            MaterialTheme {
                Surface {
                    // Observed, not read once in `onCreate`. Choosing a
                    // destination below writes through the trust store, so a
                    // snapshot taken before the choice would leave this screen
                    // showing the computer the person just changed away from.
                    val peers by app.trustStore.peersFlow.collectAsState()
                    val selectedHex by app.trustStore.selectedPeerFlow.collectAsState()
                    if (sharedText != null && uri == null) {
                        SendTextScreen(
                            text = sharedText,
                            peers = peers,
                            selectedHex = selectedHex,
                            onChoose = ::chooseDestination,
                            onSend = { target, text, onOutcome ->
                                startTextSend(target, text, onOutcome)
                            },
                            onClose = { finish() },
                        )
                    } else {
                        SendScreen(
                            app = app,
                            uri = uri,
                            peers = peers,
                            selectedHex = selectedHex,
                            onChoose = ::chooseDestination,
                            onSend = { target, file, onOutcome ->
                                startSend(target, file, onOutcome)
                            },
                            onClose = { finish() },
                        )
                    }
                }
            }
        }
    }

    /**
     * Points the app at the computer chosen for this share.
     *
     * Choosing a destination and connecting to it are the same act: a transfer
     * and a clipboard update both travel over the authenticated session, so a
     * share aimed at a computer the app is not connected to has nowhere to go.
     * Starting the service here — rather than at the moment Send is pressed —
     * also gives the session time to come up while the person is still reading
     * the screen.
     *
     * The choice is written through [ConnectionService], which is the single
     * writer: it persists it, ends a session belonging to a different computer
     * and wakes the retry loop, and refuses a fingerprint that is not trusted.
     */
    private fun chooseDestination(peer: TrustStore.TrustedPeer) {
        ConnectionService.start(this, peer.fingerprint)
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
        // Named, not implied: the session must be pointed at the computer this
        // send is for, or the offer would travel over somebody else's.
        ConnectionService.start(this, peer.fingerprint)
        lifecycleScope.launch {
            app.clipboard.sendText(peer.fingerprint, text, sensitive = false)
                .onSuccess { receipt ->
                    // Waits for the computer's verdict rather than announcing
                    // one. `sendText` returning means the frame is on the
                    // session; it has never meant the text arrived, and this
                    // screen said "Sent" on the strength of it (GitHub #8).
                    // `ClipboardDelivery.describe` is the single vocabulary,
                    // including the honest "delivery not confirmed".
                    val delivery = receipt.awaitVerdict(ClipboardLimits.VERDICT_TIMEOUT_MS)
                    android.widget.Toast.makeText(
                        this@SendActivity,
                        delivery.describe(peer.deviceName),
                        android.widget.Toast.LENGTH_LONG,
                    ).show()
                    // The screen closes either way: this is a modal over
                    // someone else's app and the message is the report. What
                    // changed is that the message is now true.
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
        // The connection is what carries the offer, so make sure there is one —
        // and that it is pointed at the computer this offer names.
        ConnectionService.start(this, peer.fingerprint)
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
    app: OmniBridgeApp,
    uri: Uri?,
    peers: List<TrustStore.TrustedPeer>,
    selectedHex: String?,
    onChoose: (TrustStore.TrustedPeer) -> Unit,
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

    // A computer without the `files.v1` grant is not a destination, however
    // well paired it is. Narrowing here rather than letting `offer` refuse
    // later means the screen never offers a Send that cannot work.
    //
    // Re-derived on every recomposition, which is what makes a Retry respect
    // a grant withdrawn since the attempt it is retrying: the destination is
    // recomputed at the moment of the tap, never carried over.
    val eligible = UiMapping.fileDestinations(peers)
    val destination = PeerTarget.resolve(eligible, selectedHex)
    val peer = destination.peerOrNull()

    Column(
        modifier = Modifier.padding(16.dp).fillMaxWidth(),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Text("Send with OmniBridge", style = MaterialTheme.typography.headlineSmall)

        when {
            uri == null ->
                Text("Nothing to send: that share did not include a file.")

            name == null ->
                // The sanitizer refused it. Saying so is better than inventing
                // a name for a file whose own name was unusable.
                Text("That file's name cannot be sent safely. Rename it and try again.")

            peers.isEmpty() ->
                Text("No computer paired yet. Open OmniBridge and scan the pairing code first.")

            eligible.isEmpty() ->
                // Paired but not permitted, which is a different sentence and a
                // different fix. Saying "nothing is paired" here would send
                // someone to the QR scanner for a grant they already own.
                Text(
                    "No paired computer is allowed to receive files. Turn on " +
                        "\"Receive files\" for one of them in OmniBridge first.",
                )

            peer == null -> {
                // Several eligible computers and none chosen — the case the
                // old code answered with `first()`, silently. There is no Send
                // button until the person names a destination.
                Text("Which computer should receive ${'$'}name?")
                DestinationPicker(eligible, selectedHex, onChoose)
            }

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

                // Still offered when there is more than one candidate, even
                // though one is already chosen: a preselected destination is a
                // convenience, and it must stay visibly changeable rather than
                // become the same silent routing under a nicer name.
                if (eligible.size > 1) {
                    Text("Send to", style = MaterialTheme.typography.bodySmall)
                    DestinationPicker(eligible, peer.fingerprint.toHex(), onChoose)
                }

                // UX-DEBT-01: this screen follows the transfer *it* started,
                // by the id `offer` returned, and never a transfer that
                // merely shares a display name. See `UiMapping.sendSurface`.
                val surface = UiMapping.sendSurface(attempt, transfers)

                // Re-read from the live state rather than from the captured
                // composition, so two taps landing before a recomposition
                // cannot both start an attempt.
                val start = {
                    val now = UiMapping.sendSurface(attempt, transfers)
                    if (UiMapping.canStartSend(attempt, now)) {
                        attempt = UiMapping.SendAttempt.Sending
                        onSend(peer, uri) { outcome -> attempt = outcome }
                    }
                }

                when (surface) {
                    is UiMapping.SendSurface.Offer -> {
                        // The failure is stated above the button rather than
                        // in a toast: a toast on a Sharesheet is gone before
                        // the person has finished reading it, and the button
                        // beneath it is the retry.
                        (attempt as? UiMapping.SendAttempt.Failed)?.let {
                            Text(it.message, color = MaterialTheme.colorScheme.error)
                        }
                        Button(
                            enabled = UiMapping.canStartSend(attempt, surface),
                            onClick = start,
                        ) { Text(UiMapping.sendButtonLabel(attempt)) }
                    }

                    is UiMapping.SendSurface.InFlight ->
                        // Null for the instant between `offer` returning and
                        // its row arriving. No button either way: an offer is
                        // already out.
                        surface.transfer?.let { TransferRow(it) }

                    is UiMapping.SendSurface.Ended -> {
                        TransferRow(surface.transfer)
                        // The whole of UX-DEBT-01. A declined, failed,
                        // cancelled or completed attempt is history; this
                        // starts a *new* one, with a new transfer id, through
                        // the same path the first attempt took. Never
                        // automatic, and never in the background.
                        Button(onClick = start) {
                            Text(UiMapping.retryButtonLabel(surface.transfer.state))
                        }
                    }
                }
            }
        }

        Button(onClick = onClose) { Text("Close") }
    }
}

/**
 * Which computer receives this share.
 *
 * A radio group rather than a dialog: the destination is part of what the
 * person is confirming, so it belongs on the screen beside the file, not
 * behind a second tap. Each row states the fingerprint as well as the name,
 * because two desktops can quite reasonably be called the same thing and only
 * one of them is the pinned identity being connected to.
 *
 * Choosing does not grant anything and does not pair anything. Every peer
 * listed is already trusted, already holds the capability grant this share
 * needs, and is still authenticated against its pin when the session opens.
 */
@Composable
private fun DestinationPicker(
    candidates: List<TrustStore.TrustedPeer>,
    chosenHex: String?,
    onChoose: (TrustStore.TrustedPeer) -> Unit,
) {
    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
        for (candidate in candidates) {
            val chosen = candidate.fingerprint.toHex() == chosenHex
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    // One selectable row, so a screen reader announces the
                    // name, the fingerprint and the selected state together
                    // rather than reading a bare radio button.
                    .selectable(
                        selected = chosen,
                        role = Role.RadioButton,
                        onClick = { onChoose(candidate) },
                    )
                    .padding(vertical = 4.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                RadioButton(selected = chosen, onClick = null)
                Column {
                    Text(candidate.deviceName, style = MaterialTheme.typography.bodyMedium)
                    Text(
                        candidate.fingerprint.toDisplayShort(),
                        style = MaterialTheme.typography.bodySmall,
                    )
                }
            }
        }
    }
}

/**
 * The text half of the Sharesheet: "Share → OmniBridge" from a browser or a
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
    peers: List<TrustStore.TrustedPeer>,
    selectedHex: String?,
    onChoose: (TrustStore.TrustedPeer) -> Unit,
    onSend: (TrustStore.TrustedPeer, ClipboardText, (UiMapping.SendAttempt) -> Unit) -> Unit,
    onClose: () -> Unit,
) {
    var attempt by remember { mutableStateOf<UiMapping.SendAttempt>(UiMapping.SendAttempt.Idle) }

    // Both gates, not one: the grant says this computer may speak clipboard at
    // all, the policy says in which direction. A peer failing either is not a
    // destination, and collapsing them is how a screen comes to offer a Send
    // that the capability will refuse.
    val eligible = peers.filter {
        it.allows(ClipboardCapability.ID) && it.clipboardPolicy.allowSend
    }
    val destination = PeerTarget.resolve(eligible, selectedHex)
    val peer = destination.peerOrNull()

    Column(
        modifier = Modifier.padding(16.dp).fillMaxWidth(),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Text("Send text with OmniBridge", style = MaterialTheme.typography.headlineSmall)

        when {
            peers.isEmpty() ->
                Text("No computer paired yet. Open OmniBridge and scan the pairing code first.")

            eligible.isEmpty() ->
                Text(
                    "No paired computer is set up to receive your clipboard. Turn on " +
                        "\"Share clipboard with this computer\" in OmniBridge first.",
                )

            peer == null -> {
                Text("Which computer should receive this text?")
                DestinationPicker(eligible, selectedHex, onChoose)
            }

            else -> {
                if (eligible.size > 1) {
                    Text("Send to", style = MaterialTheme.typography.bodySmall)
                    DestinationPicker(eligible, peer.fingerprint.toHex(), onChoose)
                }
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
