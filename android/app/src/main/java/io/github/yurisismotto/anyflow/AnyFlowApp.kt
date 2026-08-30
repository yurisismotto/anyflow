package io.github.yurisismotto.anyflow

import android.app.Application
import android.util.Log
import io.github.yurisismotto.anyflow.capability.BatteryCapability
import io.github.yurisismotto.anyflow.capability.CapabilityRegistry
import io.github.yurisismotto.anyflow.identity.DeviceIdentity
import io.github.yurisismotto.anyflow.net.ConnectResult
import io.github.yurisismotto.anyflow.net.Discovery
import io.github.yurisismotto.anyflow.net.PeerConnection
import io.github.yurisismotto.anyflow.pairing.QrPayload
import io.github.yurisismotto.anyflow.store.TrustStore
import java.net.InetSocketAddress
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.first
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

    private val _connectionState = MutableStateFlow<ConnectionState>(ConnectionState.Idle)
    val connectionState: StateFlow<ConnectionState> = _connectionState.asStateFlow()

    sealed interface ConnectionState {
        data object Idle : ConnectionState
        data object Connecting : ConnectionState
        data class Connected(val deviceName: String, val fingerprintShort: String) :
            ConnectionState
        data class Error(val message: String) : ConnectionState
    }

    override fun onCreate() {
        super.onCreate()
        trustStore = TrustStore(this)
        identity = DeviceIdentity.loadOrCreate(trustStore.deviceId)
        battery = BatteryCapability(this)
        registry = CapabilityRegistry(listOf(battery))
        discovery = Discovery(this)

        Log.i(
            TAG,
            "identity ${identity.fingerprint.toDisplayShort()} " +
                "(strongbox=${identity.isStrongBoxBacked})",
        )
    }

    /**
     * Where to try reaching a paired computer, best guess first.
     *
     * The remembered address is tried before discovery because it usually
     * works and costs nothing; mDNS is the fallback for when the computer
     * moved.
     */
    suspend fun candidateAddresses(peer: TrustStore.TrustedPeer): List<InetSocketAddress> {
        val remembered = peer.addresses.mapNotNull(::parseAddress)

        val discovered = withTimeoutOrNull(DISCOVERY_TIMEOUT_MS) {
            discovery.browse().first { found ->
                // The device id is only a filter to avoid dialling unrelated
                // services. It is not identity: that is decided by the pinned
                // key during the TLS handshake.
                found.deviceId == null || found.deviceId == peer.deviceId
            }.address
        }

        return (remembered + listOfNotNull(discovered)).distinct()
    }

    /** Connects to a known peer using its persisted, pinned identity. */
    suspend fun connect(
        peer: TrustStore.TrustedPeer,
        address: InetSocketAddress,
    ): ConnectResult {
        _connectionState.value = ConnectionState.Connecting
        val result = PeerConnection.connect(
            address = address,
            identity = identity,
            deviceName = trustStore.deviceName,
            pinned = peer.fingerprint,
            registry = registry,
        )
        _connectionState.value = when (result) {
            is ConnectResult.Established -> ConnectionState.Connected(
                peer.deviceName,
                peer.fingerprint.toDisplayShort(),
            )
            is ConnectResult.PairingRequired -> ConnectionState.Error("pairing required")
            is ConnectResult.Failed -> ConnectionState.Error(result.reason)
        }
        return result
    }

    /**
     * Completes pairing from a scanned QR code.
     *
     * The desktop's fingerprint comes from the QR and is pinned before the
     * socket opens, so there is no window in which an impostor could answer.
     */
    suspend fun pair(payload: QrPayload): Result<TrustStore.TrustedPeer> {
        _connectionState.value = ConnectionState.Connecting

        val addresses = payload.addresses.ifEmpty {
            listOfNotNull(
                withTimeoutOrNull(DISCOVERY_TIMEOUT_MS) {
                    discovery.browse().first { it.deviceId == payload.deviceId }.address
                },
            )
        }
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
                        addresses = listOf("${address.hostString}:${address.port}"),
                    )
                    trustStore.addPeer(peer)
                    connection.disconnect()
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

    private fun parseAddress(text: String): InetSocketAddress? {
        val separator = text.lastIndexOf(':')
        if (separator <= 0) return null
        val port = text.substring(separator + 1).toIntOrNull() ?: return null
        return runCatching {
            InetSocketAddress.createUnresolved(text.substring(0, separator), port)
        }.getOrNull()
    }

    companion object {
        private const val TAG = "AnyFlowApp"
        private const val DISCOVERY_TIMEOUT_MS = 5_000L
    }
}
