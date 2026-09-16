package io.github.yurisismotto.anyflow

import android.app.Application
import android.util.Log
import io.github.yurisismotto.anyflow.capability.BatteryCapability
import io.github.yurisismotto.anyflow.capability.CapabilityRegistry
import io.github.yurisismotto.anyflow.capability.ClipboardCapability
import io.github.yurisismotto.anyflow.capability.FilesCapability
import io.github.yurisismotto.anyflow.capability.NotificationsCapability
import io.github.yurisismotto.anyflow.clipboard.ClipboardNotifications
import io.github.yurisismotto.anyflow.clipboard.ClipboardSync
import io.github.yurisismotto.anyflow.clipboard.SystemClipboard
import io.github.yurisismotto.anyflow.files.FileTransferManager
import io.github.yurisismotto.anyflow.identity.DeviceIdentity
import io.github.yurisismotto.anyflow.net.ConnectResult
import io.github.yurisismotto.anyflow.net.Discovery
import io.github.yurisismotto.anyflow.net.Endpoints
import io.github.yurisismotto.anyflow.net.PeerConnection
import io.github.yurisismotto.anyflow.notifications.KeyguardLockState
import io.github.yurisismotto.anyflow.notifications.NotificationAccess
import io.github.yurisismotto.anyflow.notifications.NotificationPolicy
import io.github.yurisismotto.anyflow.notifications.NotificationSecret
import io.github.yurisismotto.anyflow.notifications.NotificationSource
import io.github.yurisismotto.anyflow.pairing.QrPayload
import io.github.yurisismotto.anyflow.store.PeerTarget
import io.github.yurisismotto.anyflow.store.TrustStore
import java.net.InetSocketAddress
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.catch
import kotlinx.coroutines.flow.filter
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.take
import kotlinx.coroutines.withTimeoutOrNull

/**
 * Application-scoped wiring.
 *
 * Deliberately plain: no dependency-injection framework for six objects.
 */
class AnyFlowApp : Application() {

    lateinit var trustStore: TrustStore
        private set
    lateinit var identity: DeviceIdentity
        private set
    lateinit var battery: BatteryCapability
        private set
    lateinit var registry: CapabilityRegistry
        private set
    lateinit var discovery: Discovery
        private set
    lateinit var files: FileTransferManager
        private set
    lateinit var clipboard: ClipboardSync
        private set
    lateinit var systemClipboard: SystemClipboard
        private set

    /**
     * `notifications.v1` — the Android source adapter.
     *
     * Held here rather than inside the listener service because the service is
     * created and destroyed by the system as it binds and unbinds, and the
     * capability's state — role epochs, the id map, the outbound queue —
     * belongs to the app rather than to one binding.
     */
    lateinit var notifications: NotificationSource
        private set

    /**
     * Application-lifetime scope for file transfers.
     *
     * Not the connection service's scope: a transfer must survive the UI
     * being closed, and must not be cancelled by a screen rotation.
     */
    private val appScope = kotlinx.coroutines.CoroutineScope(
        kotlinx.coroutines.SupervisorJob() + kotlinx.coroutines.Dispatchers.Default,
    )

    private val _connectionState = MutableStateFlow<ConnectionState>(ConnectionState.Idle)
    val connectionState: StateFlow<ConnectionState> = _connectionState.asStateFlow()

    sealed interface ConnectionState {
        data object Idle : ConnectionState
        data object Connecting : ConnectionState
        data class Connected(val deviceName: String, val fingerprintShort: String) :
            ConnectionState

        /**
         * Not connected, but a retry is scheduled. Distinct from [Error]:
         * this says the app is still trying, which is the difference the
         * reconnect defect made invisible.
         */
        data class Retrying(val reason: String, val inSeconds: Long) : ConnectionState
        data class Error(val message: String) : ConnectionState
    }

    /** Lets the connection service publish the state it owns. */
    fun publishConnectionState(state: ConnectionState) {
        _connectionState.value = state
    }

    override fun onCreate() {
        super.onCreate()
        trustStore = TrustStore(this)
        identity = DeviceIdentity.loadOrCreate(trustStore.deviceId)
        battery = BatteryCapability(this)

        // The grant is re-read from the trust store on every question rather
        // than captured once, so revoking `files.v1` — or forgetting the
        // computer entirely — takes effect immediately, including against a
        // transfer that is already running.
        files = FileTransferManager(
            context = this,
            identity = identity,
            scope = appScope,
            isAuthorized = { peer -> isFileTransferAllowed(peer) },
        )

        // clipboard.v1. The grant and the per-peer policy are both re-read
        // from the trust store on every question, so forgetting a computer —
        // or withdrawing just its clipboard grant — stops traffic at once,
        // including on a session that is already connected.
        systemClipboard = SystemClipboard(this)
        clipboard = ClipboardSync(
            systemClipboard = systemClipboard,
            authorizer = { peer -> clipboardPolicyFor(peer) },
            localDeviceId = trustStore.deviceId,
        )

        // With `autoReceive` off — the default — an accepted clip is held in
        // memory and offered here. The notification carries no clipboard
        // text, only the computer's name and the size.
        val clipPending = ClipboardNotifications(this)
        clipboard.onClipPending = clipPending::show

        // notifications.v1. Three permissions, checked independently and none
        // of them implying another: Android's notification access, this peer's
        // grant in the trust store, and the roles each side announces. The
        // secret lives in the Keystore and is resolved lazily, so a keystore
        // that is briefly unavailable makes the capability inert for that
        // session rather than replacing a live secret.
        notifications = NotificationSource(
            ownPackage = packageName,
            localDeviceId = trustStore.deviceId,
            authorizer = { peer -> notificationPolicyFor(peer) },
            appLabels = ::appLabelFor,
            secretProvider = ::notificationSecret,
            lockState = KeyguardLockState(this),
            access = NotificationAccess.controlFor(this),
        )
        notifications.start(appScope)

        registry = CapabilityRegistry(
            listOf(
                battery,
                FilesCapability(files),
                ClipboardCapability(clipboard) { peer ->
                    trustStore.peer(peer)?.deviceName ?: "a paired computer"
                },
                NotificationsCapability(notifications),
            ),
        )
        discovery = Discovery(this)

        Log.i(
            TAG,
            "identity ${identity.fingerprint.toDisplayShort()} " +
                "(strongbox=${identity.isStrongBoxBacked})",
        )
    }

    /**
     * Whether a computer may transfer files with this phone, right now.
     *
     * Answered from the trust store every time. A peer that was forgotten, or
     * whose `files.v1` grant was withdrawn, stops being authorized at once —
     * the handshake's answer is a snapshot and would not.
     */
    private fun isFileTransferAllowed(peer: io.github.yurisismotto.anyflow.identity.Fingerprint):
        Boolean = runCatching {
        trustStore.peer(peer)?.grantedCapabilities?.contains(FilesCapability.ID) == true
    }.getOrDefault(false)

    /**
     * What a computer may do with this phone's clipboard, right now.
     *
     * Failing closed on any error is deliberate: an unreadable trust store is
     * a reason to permit nothing, not a reason to permit everything.
     */
    private fun clipboardPolicyFor(
        peer: io.github.yurisismotto.anyflow.identity.Fingerprint,
    ): io.github.yurisismotto.anyflow.clipboard.ClipboardPolicy = runCatching {
        trustStore.clipboardPolicyFor(peer)
    }.getOrDefault(io.github.yurisismotto.anyflow.clipboard.ClipboardPolicy.DENIED)

    /**
     * What a computer may be told about this phone's notifications, right now.
     *
     * Failing closed on any error, for the same reason [clipboardPolicyFor]
     * does: an unreadable trust store is a reason to permit nothing.
     */
    private fun notificationPolicyFor(
        peer: io.github.yurisismotto.anyflow.identity.Fingerprint,
    ): NotificationPolicy = runCatching {
        trustStore.notificationPolicyFor(peer)
    }.getOrDefault(NotificationPolicy.DENIED)

    /**
     * `device_notification_secret`, resolved on demand.
     *
     * Cached once it is available, because it does not change while the
     * process lives; **not** cached as "absent", so a keystore that could not
     * answer once is asked again rather than disabling the capability for the
     * life of the process. Nothing here can return the key material: the
     * secret is a non-exportable Keystore HMAC key and this returns a handle
     * that can only be used to compute a MAC.
     */
    @Volatile
    private var cachedNotificationSecret: NotificationSecret? = null

    private fun notificationSecret(): NotificationSecret? {
        cachedNotificationSecret?.let { return it }
        val secret = NotificationSecret.loadOrCreate(
            NotificationSecret.KeystoreSecretStore(),
            trustStore,
        )
        if (secret != null) {
            // A state name and a generation count. No key material, ever.
            Log.i(TAG, "notification secret ${secret.state} generation=${secret.generation}")
            cachedNotificationSecret = secret
        }
        return secret
    }

    /**
     * A package name resolved to something a person recognises.
     *
     * Only the desktop's alternative would be a package database it does not
     * have, so the label is resolved here — off the listener callback thread,
     * because this is a binder call. A package that cannot be resolved falls
     * back to its own name rather than to an empty string: "com.example.chat"
     * is worse than "Chat" and much better than nothing.
     *
     * This needs no `QUERY_ALL_PACKAGES`: an app that just posted a
     * notification to our listener is visible to us, and a package that is not
     * resolvable simply keeps its id.
     */
    private fun appLabelFor(packageName: String): String = runCatching {
        val info = packageManager.getApplicationInfo(packageName, 0)
        packageManager.getApplicationLabel(info).toString()
    }.getOrDefault(packageName)

    /**
     * Which computer this phone is connecting to, right now.
     *
     * The single answer to "where to", read fresh every time from the trust
     * store and the person's choice. It replaces `peers().firstOrNull()` —
     * the certified U2 §39.17 defect, where storage order was the routing
     * table and an offline first entry made every later peer unreachable.
     *
     * Failing closed on a trust-store error, for the reason the capability
     * authorizers do: an unreadable store is a reason to dial nothing, not a
     * reason to dial whatever was cached.
     */
    fun targetResolution(): PeerTarget.Resolution = runCatching {
        PeerTarget.resolve(trustStore.peers(), trustStore.selectedPeerHex)
    }.getOrDefault(PeerTarget.Resolution.NoTrustedPeer)

    /** The one computer to dial, or null when there is no unambiguous one. */
    fun targetPeer(): TrustStore.TrustedPeer? = targetResolution().peerOrNull()

    /**
     * Records the person's choice of computer.
     *
     * Returns false — and changes nothing — for a fingerprint that is not
     * trusted. Choosing a destination is not a route to trust: an unpaired
     * computer still has to be paired, and the pin is still checked on every
     * connection to a chosen one.
     */
    fun selectPeer(fingerprint: io.github.yurisismotto.anyflow.identity.Fingerprint): Boolean =
        runCatching { trustStore.selectPeer(fingerprint) }.getOrDefault(false)

    /**
     * Where to try reaching a paired computer, best guess first.
     *
     * A peer is a *set* of possible endpoints, not one address. A dual-stack
     * daemon publishes several, and the phone may also remember where the
     * computer was last time. Taking the first of those is how the phone
     * ended up dialling a link-local IPv6 address that could never answer.
     *
     * `round` is the attempt number within the current reconnect cycle.
     * Round 1 uses the remembered addresses alone when there are any: they
     * usually work and cost nothing, whereas browsing costs a multicast lock
     * and several seconds. From round 2 — meaning the remembered ones just
     * failed — discovery runs and its results are merged in. The order within
     * the result is decided by [Endpoints.order].
     */
    suspend fun candidateAddresses(
        peer: TrustStore.TrustedPeer,
        round: Int = 1,
    ): List<InetSocketAddress> {
        val remembered = peer.addresses.mapNotNull(Endpoints::parse)
        if (round <= 1 && remembered.isNotEmpty()) return remembered

        return remembered + discover(peer.deviceId)
    }

    /**
     * Collects every address advertised for [deviceId] within the discovery
     * window.
     *
     * Deliberately not `first { }`: one resolve callback delivers all of a
     * service's addresses, and keeping only one of them throws away the very
     * alternative that would have worked. The window is bounded and the
     * result is capped, so a hostile responder cannot make this run long or
     * return an unbounded list.
     */
    private suspend fun discover(deviceId: String): List<InetSocketAddress> {
        // Collected into a set the timeout cannot take away: whatever was
        // found before the window closed is still worth dialling.
        val found = LinkedHashSet<InetSocketAddress>()
        withTimeoutOrNull(DISCOVERY_TIMEOUT_MS) {
            discovery.browse()
                // Discovery failing is "nothing found", never an exception
                // thrown at the reconnect loop. Belt and braces on top of
                // Discovery's own handling: this collector must not be the
                // thing that kills a reconnect job.
                .catch { e -> Log.w(TAG, "discovery stopped: ${e.javaClass.simpleName}") }
                // The device id is only a filter to avoid dialling unrelated
                // services. It is not identity: that is decided by the pinned
                // key during the TLS handshake.
                .filter { it.deviceId == null || it.deviceId == deviceId }
                .map { it.address }
                // Completes the flow rather than cancelling anything, and
                // bounds the work a hostile responder can cause.
                .take(MAX_DISCOVERED_ADDRESSES)
                .collect { found += it }
        }
        return found.toList()
    }

    /**
     * Connects to a known peer using its persisted, pinned identity.
     *
     * Reports no state of its own: the connection service owns the link
     * state, because it is the only component that knows whether a failure is
     * about to be retried. Setting it here as well was how the UI came to
     * show "Connected" long after the session had died.
     */
    suspend fun connect(
        peer: TrustStore.TrustedPeer,
        address: InetSocketAddress,
    ): ConnectResult = PeerConnection.connect(
        address = address,
        identity = identity,
        deviceName = trustStore.deviceName,
        pinned = peer.fingerprint,
        registry = registry,
    )

    /**
     * Completes pairing from a scanned QR code.
     *
     * The desktop's fingerprint comes from the QR and is pinned before the
     * socket opens, so there is no window in which an impostor could answer.
     */
    suspend fun pair(payload: QrPayload): Result<TrustStore.TrustedPeer> {
        _connectionState.value = ConnectionState.Connecting

        val addresses = Endpoints.order(
            payload.addresses.ifEmpty { discover(payload.deviceId) },
        )
        if (addresses.isEmpty()) {
            _connectionState.value = ConnectionState.Error("could not find the computer")
            return Result.failure(IllegalStateException("no reachable address"))
        }

        var lastError = "could not reach the computer"
        for (address in addresses) {
            val result = PeerConnection.connect(
                address = address,
                identity = identity,
                deviceName = trustStore.deviceName,
                pinned = payload.fingerprint,
                registry = registry,
                pairingToken = payload.token,
            )
            when (result) {
                is ConnectResult.Established -> {
                    val connection = result.connection
                    val peer = TrustStore.TrustedPeer(
                        deviceId = connection.peerDevice.deviceId,
                        deviceName = TrustStore.sanitizeDeviceName(
                            connection.peerDevice.deviceName,
                        ),
                        fingerprint = payload.fingerprint,
                        pairedAtUnix = System.currentTimeMillis() / 1000,
                        // What the desktop advertises is what both sides
                        // *support*, not what either has authorized. The
                        // clipboard grant is withheld here and turned on from
                        // the device card, because a computer that can write
                        // this phone's clipboard can also see what is pasted
                        // next — a side effect that needs its own yes.
                        // `notifications.v1` is withheld here for a stronger
                        // version of the clipboard's reason: a computer that
                        // can see this phone's notifications sees banking
                        // alerts, 2FA codes and message previews, and on the
                        // certification hardware the platform's own OTP
                        // redaction did not fire at all. It is never in
                        // `auto_grant` (ADR-0015 §4) and is turned on per peer,
                        // deliberately, from the device card.
                        grantedCapabilities = connection.negotiatedCapabilities
                            .toSet() - ClipboardCapability.ID - NotificationsCapability.ID,
                        addresses = listOf(Endpoints.format(address)),
                    )
                    trustStore.addPeer(peer)
                    connection.disconnect()
                    // `files.v1` is granted here because the person just
                    // paired this computer by hand, and every incoming file
                    // still needs their explicit approval. The grant is
                    // revocable from the device card, and revoking it takes
                    // effect immediately.
                    _connectionState.value = ConnectionState.Idle
                    return Result.success(peer)
                }
                is ConnectResult.PairingRequired -> lastError = "the computer declined"
                is ConnectResult.Failed -> lastError = result.reason
            }
        }

        _connectionState.value = ConnectionState.Error(lastError)
        return Result.failure(IllegalStateException(lastError))
    }

    companion object {
        private const val TAG = "AnyFlowApp"
        private const val DISCOVERY_TIMEOUT_MS = 5_000L

        /**
         * Enough to cover a dual-stack daemon plus a spare. A cap is needed
         * because the responder is attacker-controlled and could otherwise
         * publish records until the phone ran out of patience.
         */
        private const val MAX_DISCOVERED_ADDRESSES = 6
    }
}
