package io.github.yurisismotto.anyflow

import android.app.Application
import android.util.Log
import io.github.yurisismotto.anyflow.capability.BatteryCapability
import io.github.yurisismotto.anyflow.capability.CapabilityRegistry
import io.github.yurisismotto.anyflow.capability.FilesCapability
import io.github.yurisismotto.anyflow.files.FileTransferManager
import io.github.yurisismotto.anyflow.identity.DeviceIdentity
import io.github.yurisismotto.anyflow.net.ConnectResult
import io.github.yurisismotto.anyflow.net.Discovery
import io.github.yurisismotto.anyflow.net.Endpoints
import io.github.yurisismotto.anyflow.net.PeerConnection
import io.github.yurisismotto.anyflow.pairing.QrPayload
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

        registry = CapabilityRegistry(listOf(battery, FilesCapability(files)))
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
                        grantedCapabilities = connection.negotiatedCapabilities.toSet(),
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
