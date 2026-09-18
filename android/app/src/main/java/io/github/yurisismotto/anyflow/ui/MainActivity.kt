package io.github.yurisismotto.anyflow.ui

import android.Manifest
import android.content.ActivityNotFoundException
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
import io.github.yurisismotto.anyflow.AnyFlowApp
import io.github.yurisismotto.anyflow.R
import io.github.yurisismotto.anyflow.capability.BatteryCapability
import io.github.yurisismotto.anyflow.capability.ClipboardCapability
import io.github.yurisismotto.anyflow.capability.FilesCapability
import io.github.yurisismotto.anyflow.capability.NotificationsCapability
import io.github.yurisismotto.anyflow.clipboard.ClipboardLimits
import io.github.yurisismotto.anyflow.clipboard.ClipboardNotifications
import io.github.yurisismotto.anyflow.clipboard.ClipboardPolicy
import io.github.yurisismotto.anyflow.clipboard.ClipboardSendFailed
import io.github.yurisismotto.anyflow.clipboard.ClipboardSync
import io.github.yurisismotto.anyflow.clipboard.SystemClipboard
import io.github.yurisismotto.anyflow.files.FileTransferManager
import io.github.yurisismotto.anyflow.files.OpenAction
import io.github.yurisismotto.anyflow.identity.Fingerprint
import io.github.yurisismotto.anyflow.notifications.InstalledApps
import io.github.yurisismotto.anyflow.notifications.NotificationAccess
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
        // Rotating the scanner recreates its Activity, but this launcher
        // belongs to *this* Activity and is delivered exactly one result per
        // scan — so a code scanned after a rotation pairs once, not twice.
        val payload = when (val outcome = PairingScanner.outcomeOf(result.contents)) {
            // Back, or dismissed. Nothing to report and nothing to do.
            PairingScanner.Outcome.Cancelled -> return@registerForActivityResult
            // Never echo the scanned text back to the screen: it may contain
            // a pairing token, and it is attacker-supplied either way.
            PairingScanner.Outcome.NotAnyFlowCode -> {
                showError("That QR code is not a AnyFlow pairing code.")
                return@registerForActivityResult
            }
            is PairingScanner.Outcome.Pair -> outcome.payload
        }
        lifecycleScope.launch {
            app.pair(payload)
                // The computer that was just paired becomes the target. It is
                // the only reading of "scan this code" that is not a guess,
                // and it is what makes pairing a second desktop work: without
                // it the new peer would be trusted and unreachable behind
                // whichever entry the store happened to hold first.
                .onSuccess { outcome ->
                    ConnectionService.start(this@MainActivity, outcome.peer.fingerprint)
                    // Says which of the two things happened. A scan against a
                    // computer that still trusts this phone is a reconnection
                    // and no token was spent; calling that "paired" is what
                    // made UX-HARDENING §20 test 8a read as more than it was.
                    showError(
                        UiMapping.pairedMessage(
                            outcome.peer.deviceName,
                            outcome.provedToken,
                        ),
                    )
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
     * The Quick Settings clipboard request, and when it may run.
     *
     * Android refuses `getPrimaryClip` to an app without window focus, and
     * this Activity is not focused when a tile intent is delivered — not in
     * `onCreate`, not in `onNewIntent` behind the shade, and not in
     * `onResume`. Reading there is GitHub #7: the read came back empty and
     * the person was told their clipboard was empty. The request therefore
     * waits here until `onWindowFocusChanged(true)`. See [ClipboardShortcut].
     */
    private val clipboardShortcut = ClipboardShortcut()

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
        // Restored *before* the launch intent is looked at, and that order is
        // the whole of test F12. A configuration change recreates this
        // Activity and `getIntent()` still returns the tile's intent, so
        // without the id this instance already spent, a rotation after a tile
        // send would send the clipboard again.
        clipboardShortcut.restore(savedInstanceState?.getString(STATE_CONSUMED_REQUEST))
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
     * Re-reads the permission state Android owns, and tells the capability
     * when it has moved.
     *
     * Called on every resume, which covers the one path that matters: the
     * person went to Settings and came back. It is also the path that covers
     * a *revocation* made there, which is why it re-reads rather than only
     * checking when it expects a grant.
     *
     * # Why the second line exists
     *
     * Reading it into [notificationAccess] updates the *screen*. It does not
     * update the capability, and the OS notification-access grant is one of
     * the three independent inputs to whether this device can source at all —
     * so a person who granted access in Android's settings and came back got a
     * consent screen that said "Allowed" over a session that had not asked the
     * system to bind the listener and had not re-announced its roles. Nothing
     * mirrored until some *other* in-app write happened to call
     * [NotificationSource.policyChanged] as a side effect, and picking an app
     * was usually that write — which is why it looked like it worked.
     *
     * Only on a change: `policyChanged` is idempotent and announces nothing
     * for an unchanged role set, but a resume is frequent and this keeps the
     * event meaning what it says.
     */
    override fun onSaveInstanceState(outState: Bundle) {
        super.onSaveInstanceState(outState)
        outState.putString(STATE_CONSUMED_REQUEST, clipboardShortcut.consumedId())
    }

    /**
     * The only place a Quick Settings clipboard request is ever executed.
     *
     * Not `onResume`, and not after a delay. An Activity behind the Quick
     * Settings shade is resumed and unfocused, and so is one behind a
     * permission dialog; `hasFocus` is the platform's own answer to the
     * question `getPrimaryClip` actually asks. Draining is idempotent, so the
     * repeated `true` callbacks Android emits do nothing after the first.
     */
    override fun onWindowFocusChanged(hasFocus: Boolean) {
        super.onWindowFocusChanged(hasFocus)
        if (clipboardShortcut.onWindowFocusChanged(hasFocus) == ClipboardShortcut.Action.SEND) {
            sendClipboardFromShortcut()
        }
    }

    override fun onResume() {
        super.onResume()
        val granted = NotificationAccess.isGranted(this)
        val changed = granted != notificationAccess
        notificationAccess = granted
        if (changed) {
            app.notifications.policyChanged()
        }
    }

    /** Collects every observable the UI draws from into one snapshot. */
    @Composable
    private fun rememberMainUiState(): MainUiState {
        // Observed, not read once. An earlier release read the peer list during
        // composition and never again, so a grant changed anywhere else stayed
        // invisible until the screen was recreated.
        val connection by app.connectionState.collectAsState()
        val peers by app.trustStore.peersFlow.collectAsState()
        // The second view of the same records: what the list draws, revoked
        // rows included. Collected beside the trusted set rather than derived
        // from it, because the two answer different questions.
        val listedPeers by app.trustStore.listedPeersFlow.collectAsState()
        // Observed for the same reason the peer list is: the connection
        // service and the share sheet both write the choice, so a value read
        // once here would go stale the moment either of them re-pointed the
        // link.
        val selectedPeerHex by app.trustStore.selectedPeerFlow.collectAsState()
        val offers by app.files.pendingOffers.collectAsState()
        val transfers by app.files.visible.collectAsState()
        val pendingClips by app.clipboard.pendingClips.collectAsState()
        val deliveries by app.clipboard.lastDelivery.collectAsState()
        // What the session that is up actually negotiated. Observed, because a
        // session ending has to grey the Send button out rather than leave it
        // live over nothing.
        val liveSession by app.liveSession.collectAsState()
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
            listedPeers = listedPeers,
            selectedPeerHex = selectedPeerHex,
            offers = offers,
            transfers = transfers,
            pendingClips = pendingClips,
            clipboardDeliveries = deliveries,
            liveSession = liveSession,
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
        onRevoke = { peer ->
            // `revokePeer` drops the choice with the trust, so the next
            // connection cannot be aimed at something no longer trusted, and
            // nothing is chosen in its place.
            app.trustStore.revokePeer(peer.fingerprint)
            ConnectionService.stop(this)
        },
        onRemoveFromList = { peer ->
            // Refused by the store for anything still trusted, so this cannot
            // become a quiet second way to revoke. The connection is stopped
            // for the same reason the revoke does it: the row is going, and a
            // service still dialling on its behalf would outlive it.
            if (app.trustStore.hideRevokedPeer(peer.fingerprint)) {
                ConnectionService.stop(this)
            }
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
        onOpenTransfer = { transferId -> openTransfer(transferId) },
        onRefreshOpenTargets = { app.files.refreshOpenTargets() },
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
     * Hands a finished transfer's file to Android to open.
     *
     * ## What "open" means here, and what it deliberately does not mean
     *
     * It means `ACTION_VIEW` with a `content://` URI and a read grant for
     * that one item, resolved through a chooser. AnyFlow does not run
     * anything, does not decide whether the file is safe, and does not treat
     * an extension as evidence about either. Whatever Android would do with
     * this file from the Files app is what happens, with the same consent
     * prompts in the same places.
     *
     * ## Three rules the URI obeys
     *
     * *It is never a `file://` URI.* Passing one to another app throws
     * `FileUriExposedException` on every API level this app supports, and the
     * reason it does is that a path is not a permission: the receiving app
     * would need its own access to the file, which on shared storage means a
     * storage permission that AnyFlow deliberately does not hold. The
     * received-file URI is MediaStore's own; the sent-file URI is the one the
     * person shared in. Neither is ever converted to a path.
     *
     * *It is fetched now, not when the row was drawn.* [FileTransferManager.resolveOpen]
     * re-checks the grant or the item's existence at this instant, so a lapsed
     * permission is reported rather than acted on. See its documentation for
     * why the two directions are checked differently.
     *
     * *It is granted, not assumed.* `FLAG_GRANT_READ_URI_PERMISSION` passes
     * AnyFlow's own read access to whichever app the person picks, for that
     * URI alone and for the life of that activity. Read, not write: opening a
     * file is not permission to change it.
     *
     * Every failure has its own sentence, because "it did not open" is four
     * different pieces of news and only one of them is worth going to look in
     * Downloads for.
     */
    private fun openTransfer(transferId: String) {
        lifecycleScope.launch {
            when (val resolution = app.files.resolveOpen(transferId)) {
                is FileTransferManager.OpenResolution.Ready -> launchViewer(resolution)
                is FileTransferManager.OpenResolution.Unavailable ->
                    showError(openFailureMessage(resolution.action))
            }
        }
    }

    private fun launchViewer(ready: FileTransferManager.OpenResolution.Ready) {
        val view = Intent(Intent.ACTION_VIEW).apply {
            setDataAndType(ready.uri, ready.mimeType)
            addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
            // The chooser is a separate task; without this it would be
            // launched into AnyFlow's, and backing out of the viewer would
            // land somewhere in the middle of this app.
            addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        }
        // A chooser rather than whatever happens to be default: a file that
        // arrived from another machine is exactly the case where a person may
        // want to choose, and it also means no silent hand-off to an app that
        // was made default for an unrelated reason.
        val chooser = Intent.createChooser(view, null).apply {
            addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        }
        try {
            startActivity(chooser)
        } catch (e: ActivityNotFoundException) {
            // Truthful, and specifically not "the file is missing": the file
            // is there and this device simply has nothing that reads it.
            showError(getString(R.string.files_open_no_viewer))
        }
    }

    /** The sentence for each way opening does not happen. */
    private fun openFailureMessage(action: OpenAction): String = when (action) {
        OpenAction.FileMissing -> getString(R.string.files_open_file_missing)
        OpenAction.SourceUnavailable -> getString(R.string.files_open_source_unavailable)
        OpenAction.NoViewer -> getString(R.string.files_open_no_viewer)
        // Neither should reach a person: the button is not drawn for a
        // transfer that cannot be opened. If one does, saying the file is not
        // there is the honest answer, because AnyFlow cannot reach it.
        OpenAction.NotApplicable, OpenAction.Available ->
            getString(R.string.files_open_file_missing)
    }

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

            ACTION_SEND_CLIPBOARD -> {
                // Recorded, not executed. `hasWindowFocus()` is asked rather
                // than assumed: an intent arriving at a screen the person is
                // already looking at — the ordinary `onNewIntent` case — must
                // run now, because the focus change it would otherwise wait
                // for has already happened and will not repeat.
                val action = clipboardShortcut.onRequest(
                    requestId = intent.getStringExtra(EXTRA_REQUEST_ID),
                    hasWindowFocus = hasWindowFocus(),
                )
                if (action == ClipboardShortcut.Action.SEND) sendClipboardFromShortcut()
            }
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
            1 -> {
                val peer = eligible.single()
                // The same gate the buttons use, so the tile cannot be the
                // one path that starts a send the session cannot carry. It is
                // asked here rather than folded into the filter above because
                // the *destination* is a trust question and the gate is a
                // session question, and answering them together would turn
                // "not negotiated yet" into "no computer is set up".
                when (val gate = UiMapping.clipboardSendGate(peer, app.liveSession.value)) {
                    is UiMapping.ClipboardSendGate.Ready -> sendClipboard(peer.fingerprint)
                    is UiMapping.ClipboardSendGate.Blocked -> showError(gate.reason)
                }
            }
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
                .onSuccess { receipt ->
                    // **The whole of GitHub #8 is this line not saying "Sent".**
                    // `sendCurrentClipboard` succeeding means the frame is on
                    // the session and nothing more; the computer's verdict
                    // arrives afterwards, and on a LAN it arrives in
                    // milliseconds. So the message waits for it, and when no
                    // verdict comes it says that rather than inventing one.
                    //
                    // The wait is a suspending one in the Activity's own
                    // scope: no thread is blocked, the screen stays live, and
                    // `lastDelivery` — which the UI draws — is updated whether
                    // or not anyone is still here to read the toast.
                    showError(receipt.awaitVerdict(ClipboardLimits.VERDICT_TIMEOUT_MS).describe(name))
                }
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
        // The request is built in `PairingScanner` so that what it does and
        // does not ask for — notably that it does not lock the orientation —
        // is testable rather than buried in a call site.
        scanLauncher.launch(PairingScanner.options())
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

        /**
         * A fresh random id per tile press, minted by [ClipboardTileService].
         *
         * An idempotency token, not a credential: it authorizes nothing, and
         * the send it leads to still asks the trust store for the grant and
         * the per-peer policy. Its only job is to tell a genuine second press
         * from a replay of the first through `getIntent()`.
         */
        const val EXTRA_REQUEST_ID = "io.github.yurisismotto.anyflow.CLIPBOARD_REQUEST_ID"

        /** Saved-state key for the last request id actually executed. */
        private const val STATE_CONSUMED_REQUEST = "clipboard_shortcut_consumed"

    }
}
