package io.github.yurisismotto.omnibridge.ui

import android.content.Intent
import android.net.Uri
import android.os.Build
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.annotation.DrawableRes
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.RadioButton
import androidx.compose.material3.RadioButtonDefaults
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
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.style.TextOverflow
import androidx.lifecycle.lifecycleScope
import io.github.yurisismotto.omnibridge.OmniBridgeApp
import io.github.yurisismotto.omnibridge.R
import io.github.yurisismotto.omnibridge.capability.ClipboardCapability
import io.github.yurisismotto.omnibridge.clipboard.ClipboardLimits
import io.github.yurisismotto.omnibridge.clipboard.ClipboardText
import io.github.yurisismotto.omnibridge.files.SharedFile
import io.github.yurisismotto.omnibridge.service.ConnectionService
import io.github.yurisismotto.omnibridge.store.PeerTarget
import io.github.yurisismotto.omnibridge.store.TrustStore
import io.github.yurisismotto.omnibridge.ui.components.ExchangeHero
import io.github.yurisismotto.omnibridge.ui.components.ExchangePayloadCard
import io.github.yurisismotto.omnibridge.ui.components.ExchangePayloadEmpty
import io.github.yurisismotto.omnibridge.ui.components.ExchangePeerStatus
import io.github.yurisismotto.omnibridge.ui.components.ExchangeSecurityFooter
import io.github.yurisismotto.omnibridge.ui.components.OmniBridgeIconTile
import io.github.yurisismotto.omnibridge.ui.components.OmniBridgePrimaryButton
import io.github.yurisismotto.omnibridge.ui.components.OmniBridgeTextButton
import io.github.yurisismotto.omnibridge.ui.components.deviceKindIcon
import io.github.yurisismotto.omnibridge.ui.components.exchangeAmbient
import io.github.yurisismotto.omnibridge.ui.components.omniBridgeContentColumn
import io.github.yurisismotto.omnibridge.ui.theme.MinTouchTarget
import io.github.yurisismotto.omnibridge.ui.theme.OmniBridgeSpacing
import io.github.yurisismotto.omnibridge.ui.theme.OmniBridgeStatus
import io.github.yurisismotto.omnibridge.ui.theme.OmniBridgeTheme
import io.github.yurisismotto.omnibridge.ui.theme.OmniBridgeType
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
            // The app's own theme, not a bare MaterialTheme. This screen is a
            // modal over somebody else's app and it used to look like one:
            // default Material colours, default Material cards, none of the
            // tokens the rest of OmniBridge is built from. It is an exchange
            // flow — the same act as Send clipboard with a different payload —
            // so it wears the same visual language and the same dark theme.
            OmniBridgeTheme {
                Surface(color = OmniBridgeTheme.colors.background) {
                    // Observed, not read once in `onCreate`. Choosing a
                    // destination below writes through the trust store, so a
                    // snapshot taken before the choice would leave this screen
                    // showing the computer the person just changed away from.
                    val peers by app.trustStore.peersFlow.collectAsState()
                    val selectedHex by app.trustStore.selectedPeerFlow.collectAsState()
                    if (sharedText != null && uri == null) {
                        SendTextScreen(
                            app = app,
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


/**
 * What the exchange hero and the peer pill need to know about the
 * destination, gathered once.
 *
 * Null where no destination is resolved yet — nothing paired, nothing
 * permitted, or several candidates and none chosen. The pill is then simply
 * absent rather than filled with a placeholder: a screen that says
 * "Connected to …" over no computer at all is the kind of decoration this
 * sprint is removing.
 */
private data class Destination(
    val peer: TrustStore.TrustedPeer,
    val status: OmniBridgeStatus,
    val identity: String,
    val kind: UiMapping.DeviceKind,
)

/**
 * The destination as the live session describes it right now.
 *
 * Every field comes from state: the name from the trust store, the status
 * from the same [UiMapping.statusFor] the Devices screen uses, and the
 * identity line and device glyph from the session that authenticated *this*
 * peer — never from a session with a different computer, and never from a
 * stored platform. See [UiMapping.peerDeviceKind].
 */
@Composable
private fun rememberDestination(
    peer: TrustStore.TrustedPeer?,
    session: OmniBridgeApp.LiveSession?,
    connection: OmniBridgeApp.ConnectionState,
): Destination? {
    if (peer == null) return null
    val connected = session?.peerHex == peer.fingerprint.toHex()
    return Destination(
        peer = peer,
        status = UiMapping.statusFor(connection, connected = connected),
        identity = UiMapping.peerIdentityLine(peer, session),
        kind = UiMapping.peerDeviceKind(peer, session),
    )
}

/**
 * The exchange-flow composition, as a Sharesheet modal.
 *
 * Title, hero, destination, payload, action, close, security — the same
 * order and the same pieces as [SendClipboardScreen], because it is the same
 * act. The shared pieces live in `components/Exchange.kt`; what is local here
 * is only the arrangement, and it is local to the two screens in this file
 * rather than promoted into a framework.
 *
 * The content column cap is applied here too. This screen is outside the app
 * shell, so it does not inherit the shell's cap, and without it the card
 * would stretch the full width of a tablet.
 */
@Composable
private fun SendExchangeScreen(
    title: String,
    @DrawableRes sourceIcon: Int,
    destination: Destination?,
    onClose: () -> Unit,
    content: @Composable ColumnScope.() -> Unit,
) {
    val colors = OmniBridgeTheme.colors
    // The wash goes on the window-width node and the column inside it, so the
    // ambient fades out by distance rather than being clipped to the column.
    Box(Modifier.fillMaxSize().exchangeAmbient()) {
        Column(
            modifier = Modifier
                .fillMaxSize()
                .verticalScroll(rememberScrollState())
                .omniBridgeContentColumn()
                .padding(horizontal = OmniBridgeSpacing.md, vertical = OmniBridgeSpacing.lg),
        ) {
            Text(title, style = OmniBridgeType.heading, color = colors.textPrimary)

            ExchangeHero(
                sourceIcon = sourceIcon,
                // A neutral device until something has actually said otherwise.
                destinationIcon = deviceKindIcon(destination?.kind ?: UiMapping.DeviceKind.Unknown),
            )

            if (destination != null) {
                Box(Modifier.fillMaxWidth(), contentAlignment = Alignment.Center) {
                    ExchangePeerStatus(
                        status = destination.status,
                        label = if (destination.status == OmniBridgeStatus.Connected) {
                            "Connected to ${destination.peer.deviceName}"
                        } else {
                            destination.peer.deviceName
                        },
                        identity = destination.identity,
                    )
                }
                Spacer(Modifier.height(OmniBridgeSpacing.lg))
            }

            content()

            Spacer(Modifier.height(OmniBridgeSpacing.xs))
            Box(Modifier.fillMaxWidth(), contentAlignment = Alignment.Center) {
                // "Close", not "Cancel". This screen can be left while an offer is
                // already travelling, and closing it cancels nothing — a button
                // that said otherwise would be claiming to stop a transfer it has
                // no way to stop.
                OmniBridgeTextButton("Close", onClose)
            }

            Spacer(Modifier.height(OmniBridgeSpacing.lg))
            ExchangeSecurityFooter()
            Spacer(Modifier.height(OmniBridgeSpacing.xxl))
        }
    }
}

/** A payload card holding nothing but a reason it is empty. */
@Composable
private fun EmptyPayload(
    title: String,
    @DrawableRes icon: Int,
    headline: String,
    body: String,
    accent: Color? = null,
) {
    ExchangePayloadCard(title = title) {
        ExchangePayloadEmpty(icon = icon, title = headline, subtitle = body, accent = accent)
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
    val colors = OmniBridgeTheme.colors
    val transfers by app.files.visible.collectAsState()
    val session by app.liveSession.collectAsState()
    val connection by app.connectionState.collectAsState()
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
    val resolved = PeerTarget.resolve(eligible, selectedHex)
    val peer = resolved.peerOrNull()

    SendExchangeScreen(
        title = "Send files",
        sourceIcon = R.drawable.ic_files,
        destination = rememberDestination(peer, session, connection),
        onClose = onClose,
    ) {
        when {
            uri == null ->
                EmptyPayload(
                    title = "Files",
                    icon = R.drawable.ic_files,
                    headline = "Nothing to send",
                    body = "That share did not include a file.",
                )

            name == null ->
                // The sanitizer refused it. Saying so is better than inventing
                // a name for a file whose own name was unusable.
                EmptyPayload(
                    title = "Files",
                    icon = R.drawable.ic_warning,
                    headline = "That file's name cannot be sent safely",
                    body = "Rename it and try again.",
                    accent = colors.accentAmber,
                )

            peers.isEmpty() ->
                EmptyPayload(
                    title = "Files",
                    icon = R.drawable.ic_device_generic,
                    headline = "No computer paired yet",
                    body = "Open OmniBridge and scan the pairing code first.",
                )

            eligible.isEmpty() ->
                // Paired but not permitted, which is a different sentence and a
                // different fix. Saying "nothing is paired" here would send
                // someone to the QR scanner for a grant they already own.
                EmptyPayload(
                    title = "Files",
                    icon = R.drawable.ic_shield_off,
                    headline = "No paired computer may receive files",
                    body = "Turn on \"Receive files\" for one of them in OmniBridge first.",
                    accent = colors.accentAmber,
                )

            peer == null -> {
                // Several eligible computers and none chosen — the case the
                // old code answered with `first()`, silently. There is no Send
                // button until the person names a destination.
                ExchangePayloadCard(title = "Files") {
                    FileLine(name)
                    Spacer(Modifier.height(OmniBridgeSpacing.xs))
                    Text(
                        "Which computer should receive $name?",
                        style = OmniBridgeType.body,
                        color = colors.textPrimary,
                    )
                    DestinationPicker(eligible, selectedHex, onChoose)
                }
            }

            else -> {
                ExchangePayloadCard(title = "Files") {
                    FileLine(name)

                    // Still offered when there is more than one candidate, even
                    // though one is already chosen: a preselected destination is a
                    // convenience, and it must stay visibly changeable rather than
                    // become the same silent routing under a nicer name.
                    if (eligible.size > 1) {
                        Spacer(Modifier.height(OmniBridgeSpacing.xs))
                        Text(
                            "Send to",
                            style = OmniBridgeType.label,
                            color = colors.textSecondary,
                        )
                        DestinationPicker(eligible, peer.fingerprint.toHex(), onChoose)
                    }
                }

                Spacer(Modifier.height(OmniBridgeSpacing.lg))

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
                            Text(
                                it.message,
                                style = OmniBridgeType.caption,
                                color = colors.accentRed,
                                modifier = Modifier.padding(bottom = OmniBridgeSpacing.xs),
                            )
                        }
                        OmniBridgePrimaryButton(
                            text = UiMapping.sendButtonLabel(
                                attempt,
                                idleLabel = "Send file to ${peer.deviceName}",
                            ),
                            icon = R.drawable.ic_send,
                            enabled = UiMapping.canStartSend(attempt, surface),
                            onClick = start,
                        )
                    }

                    is UiMapping.SendSurface.InFlight ->
                        // Null for the instant between `offer` returning and
                        // its row arriving. No button either way: an offer is
                        // already out.
                        surface.transfer?.let { TransferRow(it) }

                    is UiMapping.SendSurface.Ended -> {
                        TransferRow(surface.transfer)
                        Spacer(Modifier.height(OmniBridgeSpacing.sm))
                        // The whole of UX-DEBT-01. A declined, failed,
                        // cancelled or completed attempt is history; this
                        // starts a *new* one, with a new transfer id, through
                        // the same path the first attempt took. Never
                        // automatic, and never in the background.
                        OmniBridgePrimaryButton(
                            text = UiMapping.retryButtonLabel(surface.transfer.state),
                            icon = R.drawable.ic_send,
                            onClick = start,
                        )
                    }
                }
            }
        }
    }
}

/** The one file this share carries, as a tile and a name. */
@Composable
private fun FileLine(name: String) {
    val colors = OmniBridgeTheme.colors
    Row(verticalAlignment = Alignment.CenterVertically) {
        OmniBridgeIconTile(icon = R.drawable.ic_file, accent = colors.accentBlue)
        Spacer(Modifier.width(OmniBridgeSpacing.sm))
        // Already through the sanitizer: a display name from another app is
        // attacker-influenced and is treated exactly like a name off the wire.
        Text(
            name,
            style = OmniBridgeType.subtitle,
            color = colors.textPrimary,
            maxLines = 2,
            overflow = TextOverflow.Ellipsis,
        )
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
    val colors = OmniBridgeTheme.colors
    Column(verticalArrangement = Arrangement.spacedBy(OmniBridgeSpacing.xxs)) {
        for (candidate in candidates) {
            val chosen = candidate.fingerprint.toHex() == chosenHex
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .heightIn(min = MinTouchTarget)
                    // One selectable row, so a screen reader announces the
                    // name, the fingerprint and the selected state together
                    // rather than reading a bare radio button.
                    .selectable(
                        selected = chosen,
                        role = Role.RadioButton,
                        onClick = { onChoose(candidate) },
                    )
                    .padding(vertical = OmniBridgeSpacing.xxs),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(OmniBridgeSpacing.xs),
            ) {
                RadioButton(
                    selected = chosen,
                    onClick = null,
                    colors = RadioButtonDefaults.colors(selectedColor = colors.accentBlue),
                )
                Column {
                    Text(
                        candidate.deviceName,
                        style = OmniBridgeType.body,
                        color = colors.textPrimary,
                    )
                    Text(
                        candidate.fingerprint.toDisplayShort(),
                        style = OmniBridgeType.mono,
                        color = colors.textMuted,
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
    app: OmniBridgeApp,
    text: ClipboardText,
    peers: List<TrustStore.TrustedPeer>,
    selectedHex: String?,
    onChoose: (TrustStore.TrustedPeer) -> Unit,
    onSend: (TrustStore.TrustedPeer, ClipboardText, (UiMapping.SendAttempt) -> Unit) -> Unit,
    onClose: () -> Unit,
) {
    val colors = OmniBridgeTheme.colors
    val session by app.liveSession.collectAsState()
    val connection by app.connectionState.collectAsState()
    var attempt by remember { mutableStateOf<UiMapping.SendAttempt>(UiMapping.SendAttempt.Idle) }

    // Both gates, not one: the grant says this computer may speak clipboard at
    // all, the policy says in which direction. A peer failing either is not a
    // destination, and collapsing them is how a screen comes to offer a Send
    // that the capability will refuse.
    val eligible = peers.filter {
        it.allows(ClipboardCapability.ID) && it.clipboardPolicy.allowSend
    }
    val resolved = PeerTarget.resolve(eligible, selectedHex)
    val peer = resolved.peerOrNull()

    SendExchangeScreen(
        title = "Send text",
        sourceIcon = R.drawable.ic_clipboard,
        destination = rememberDestination(peer, session, connection),
        onClose = onClose,
    ) {
        when {
            peers.isEmpty() ->
                EmptyPayload(
                    title = "Text",
                    icon = R.drawable.ic_device_generic,
                    headline = "No computer paired yet",
                    body = "Open OmniBridge and scan the pairing code first.",
                )

            eligible.isEmpty() ->
                EmptyPayload(
                    title = "Text",
                    icon = R.drawable.ic_shield_off,
                    headline = "No paired computer is set up to receive your clipboard",
                    body = "Turn on \"Share clipboard with this computer\" in OmniBridge first.",
                    accent = colors.accentAmber,
                )

            peer == null ->
                ExchangePayloadCard(title = "Text", trailing = "${text.byteLength} bytes") {
                    Text(
                        "Which computer should receive this text?",
                        style = OmniBridgeType.body,
                        color = colors.textPrimary,
                    )
                    DestinationPicker(eligible, selectedHex, onChoose)
                }

            else -> {
                ExchangePayloadCard(title = "Text", trailing = "${text.byteLength} bytes") {
                    // Deliberately not a preview. See this function's note.
                    Text(
                        "The text is not shown here: you just selected it, and " +
                            "a preview on this screen would be readable over " +
                            "your shoulder.",
                        style = OmniBridgeType.caption,
                        color = colors.textSecondary,
                    )
                    if (eligible.size > 1) {
                        Spacer(Modifier.height(OmniBridgeSpacing.xs))
                        Text(
                            "Send to",
                            style = OmniBridgeType.label,
                            color = colors.textSecondary,
                        )
                        DestinationPicker(eligible, peer.fingerprint.toHex(), onChoose)
                    }
                }

                Spacer(Modifier.height(OmniBridgeSpacing.lg))

                (attempt as? UiMapping.SendAttempt.Failed)?.let {
                    Text(
                        it.message,
                        style = OmniBridgeType.caption,
                        color = colors.accentRed,
                        modifier = Modifier.padding(bottom = OmniBridgeSpacing.xs),
                    )
                }
                OmniBridgePrimaryButton(
                    text = UiMapping.sendButtonLabel(
                        attempt,
                        idleLabel = "Send text to ${peer.deviceName}",
                    ),
                    icon = R.drawable.ic_send,
                    enabled = attempt.canSend,
                    onClick = {
                        attempt = UiMapping.SendAttempt.Sending
                        onSend(peer, text) { outcome -> attempt = outcome }
                    },
                )
            }
        }
    }
}
