package io.github.yurisismotto.anyflow.net

import android.util.Log
import com.google.protobuf.ByteString
import io.github.yurisismotto.anyflow.capability.CapabilityContext
import io.github.yurisismotto.anyflow.capability.CapabilityRegistry
import io.github.yurisismotto.anyflow.identity.DeviceIdentity
import io.github.yurisismotto.anyflow.identity.Fingerprint
import io.github.yurisismotto.anyflow.pairing.PairingProof
import io.github.yurisismotto.anyflow.proto.CapabilityMessage
import io.github.yurisismotto.anyflow.proto.DeviceInfo
import io.github.yurisismotto.anyflow.proto.Envelope
import io.github.yurisismotto.anyflow.proto.Hello
import io.github.yurisismotto.anyflow.proto.HelloStatus
import io.github.yurisismotto.anyflow.proto.PairRequest
import io.github.yurisismotto.anyflow.proto.PairStatus
import io.github.yurisismotto.anyflow.proto.Ping
import io.github.yurisismotto.anyflow.proto.Platform
import io.github.yurisismotto.anyflow.proto.Pong
import java.io.EOFException
import java.io.InputStream
import java.io.OutputStream
import java.net.InetSocketAddress
import java.net.Socket
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import javax.net.ssl.SSLSocket

/** Outcome of an attempt to reach a desktop. */
sealed interface ConnectResult {
    /**
     * The session is up.
     *
     * [provedPairing] says *how* it came up: true when the responder
     * demanded a pairing proof and this connection produced one from a
     * scanned token, false when the responder already knew this device and
     * answered `HELLO_STATUS_TRUSTED`.
     *
     * It is a fact about the handshake that just happened, not an
     * assumption, and it exists because the caller cannot otherwise tell
     * them apart: a scan of a fresh QR against a computer that still trusts
     * this phone resolves as an ordinary reconnection and the QR's
     * single-use token is never used. That is correct — the responder
     * decides what it requires — but it must not be *reported* as a fresh
     * pairing, and the previous sprint recorded exactly that confusion
     * (UX-HARDENING §20, test 8a).
     */
    data class Established(
        val connection: PeerConnection,
        val provedPairing: Boolean,
    ) : ConnectResult

    /** The desktop does not know us and we had no pairing token to offer. */
    data object PairingRequired : ConnectResult

    /**
     * The attempt failed. [kind] is what reconnection acts on: a transport
     * failure is retried promptly, a security failure slowly, and a
     * revocation not at all.
     */
    data class Failed(
        val reason: String,
        val kind: FailureKind = FailureKind.TRANSPORT,
        val cause: Throwable? = null,
    ) : ConnectResult
}

/**
 * One authenticated session with a paired computer.
 *
 * The initiator is always the phone: a desktop is a stable listener and a
 * phone is not, and fixing the direction leaves exactly one handshake path
 * to reason about.
 */
class PeerConnection private constructor(
    private val socket: SSLSocket,
    private val input: InputStream,
    private val output: OutputStream,
    private val factory: EnvelopeFactory,
    private val guard: ReplayGuard,
    private val registry: CapabilityRegistry,
    val peer: Fingerprint,
    val peerDevice: DeviceInfo,
    val negotiatedCapabilities: List<String>,
    val protocolVersion: Int,
) {

    /**
     * Where this session's computer actually is.
     *
     * A `files.v1` data stream dials this same address and port: there is no
     * second discovery mechanism and no second port to open (ADR-0013). Read
     * from the live socket rather than remembered from the dial, so it is the
     * address that genuinely answered.
     */
    val remoteAddress: InetSocketAddress
        get() = InetSocketAddress(socket.inetAddress, socket.port)

    private val outbound = Channel<Envelope>(Channel.BUFFERED)
    private val _closed = MutableStateFlow(false)
    val closed: StateFlow<Boolean> = _closed.asStateFlow()

    /**
     * The capability send seam.
     *
     * Builds the envelope *first* so its `message_id` can be answered to the
     * capability. That id is the only handle a capability has on a frame once
     * it is written, and it is what `Envelope.correlation_id` names when a
     * peer refuses the frame — so a capability that keeps it can attribute a
     * refusal instead of merely observing one. See [CapabilityContext.send].
     */
    private val context = CapabilityContext(peer, peerDevice.deviceId) { id, payload ->
        val envelope = factory.build {
            capabilityMessage = CapabilityMessage.newBuilder()
                .setCapabilityId(id)
                .setPayload(payload)
                .build()
        }
        send(envelope)
        envelope.messageId
    }

    /**
     * Runs the session until the connection ends, returning a short
     * description of why it ended.
     *
     * Reads and writes live on separate coroutines with a channel between
     * them: the blocking frame read is never interrupted part-way, which
     * would desynchronise the stream.
     *
     * The returned reason exists so the reconnect coordinator can log why a
     * session stopped rather than merely that it did. It is never sent to a
     * peer.
     */
    suspend fun run(scope: CoroutineScope): String = withContext(Dispatchers.IO) {
        val writer = scope.launch(Dispatchers.IO) {
            try {
                for (envelope in outbound) {
                    Framing.write(output, envelope)
                }
            } catch (e: Exception) {
                Log.d(TAG, "writer stopped: ${e.javaClass.simpleName}")
            }
        }

        registry.let {
            for (id in negotiatedCapabilities) {
                runCatching { it[id]?.onPeerConnected(context) }
                    .onFailure { e -> Log.w(TAG, "capability $id failed on connect", e) }
            }
        }

        var ending = "closed"
        try {
            while (isActive) {
                val envelope = try {
                    Framing.read(input)
                } catch (e: EOFException) {
                    ending = "peer closed the connection"
                    break
                } catch (e: java.net.SocketTimeoutException) {
                    // Nothing has arrived for SESSION_IDLE_TIMEOUT_MS. The
                    // desktop probes an idle link once a minute, so silence
                    // this long means the link is gone even though the socket
                    // has not been closed — the half-open case that a phone
                    // leaving Wi-Fi range produces. Ending here is what lets
                    // the reconnect loop start over.
                    ending = "peer stopped responding"
                    break
                }

                if (envelope.protocolVersion != protocolVersion) {
                    Log.w(TAG, "peer changed protocol version mid-connection; closing")
                    ending = "protocol version changed"
                    break
                }
                guard.admit(envelope)?.let { reason ->
                    // Replay or duplication is fatal, not something to skip: a
                    // well-behaved peer never produces one.
                    Log.w(TAG, "closing session: $reason")
                    close(writer)
                    return@withContext "replay protection: $reason"
                }

                when (envelope.bodyCase) {
                    Envelope.BodyCase.PING -> {
                        if (envelope.ping.payload.size() > MAX_PING_PAYLOAD) {
                            Log.w(TAG, "oversized ping payload; closing")
                            ending = "oversized ping"
                            break
                        }
                        send(
                            factory.buildReply(envelope.messageId) {
                                pong = Pong.newBuilder()
                                    .setPayload(envelope.ping.payload)
                                    .build()
                            },
                        )
                    }

                    Envelope.BodyCase.CAPABILITY_MESSAGE -> {
                        val message = envelope.capabilityMessage
                        // Re-checked per message from the negotiated list,
                        // never inferred from the message itself.
                        if (message.capabilityId !in negotiatedCapabilities) {
                            Log.w(TAG, "ignoring un-negotiated capability")
                            continue
                        }
                        val capability = registry[message.capabilityId] ?: continue
                        runCatching { capability.onMessage(context, message.payload) }
                            .onFailure { e ->
                                // The payload is never logged: capability
                                // payloads carry user data.
                                Log.w(TAG, "capability ${message.capabilityId} rejected a message: ${e.javaClass.simpleName}")
                            }
                    }

                    Envelope.BodyCase.PONG -> Unit
                    Envelope.BodyCase.CAPABILITY_ANNOUNCE -> Unit
                    Envelope.BodyCase.ERROR -> {
                        // The code, never `error.message`: that is a free
                        // string chosen by the other end of the wire.
                        Log.w(TAG, "peer error code=${envelope.error.code} fatal=${envelope.error.fatal}")
                        // Offered to the capabilities so the operation that
                        // caused it can be told what happened. Before this,
                        // a peer refusing `clipboard.v1` produced this log
                        // line and nothing else, and the person who pressed
                        // Send was told their clipboard had been sent
                        // (GitHub #8). A correlation id is only ever matched
                        // against ids a capability minted itself.
                        if (!envelope.correlationId.isEmpty) {
                            for (id in negotiatedCapabilities) {
                                runCatching {
                                    registry[id]?.onPeerError(
                                        context,
                                        envelope.correlationId,
                                        envelope.error.code,
                                    )
                                }.onFailure { e ->
                                    Log.w(TAG, "capability $id failed on a peer error: ${e.javaClass.simpleName}")
                                }
                            }
                        }
                        if (envelope.error.fatal) {
                            ending = "peer reported a fatal error"
                            break
                        }
                    }

                    else -> {
                        Log.w(TAG, "unexpected message on an established session; closing")
                        ending = "unexpected message"
                        break
                    }
                }
            }
        } catch (e: Exception) {
            Log.d(TAG, "session ended: ${e.javaClass.simpleName}")
            ending = e.javaClass.simpleName
        } finally {
            close(writer)
        }
        ending
    }

    private suspend fun close(writer: kotlinx.coroutines.Job) {
        for (id in negotiatedCapabilities) {
            runCatching { registry[id]?.onPeerDisconnected(peer) }
        }
        outbound.close()
        writer.cancel()
        runCatching { socket.close() }
        _closed.value = true
    }

    suspend fun send(envelope: Envelope) {
        runCatching { outbound.send(envelope) }
    }

    suspend fun sendCapability(capabilityId: String, payload: ByteString) {
        context.send(capabilityId, payload)
    }

    suspend fun ping() {
        val bytes = ByteArray(8).also { java.security.SecureRandom().nextBytes(it) }
        send(
            factory.build {
                ping = Ping.newBuilder().setPayload(ByteString.copyFrom(bytes)).build()
            },
        )
    }

    fun disconnect() {
        runCatching { socket.close() }
    }

    companion object {
        private const val TAG = "PeerConnection"
        private const val MAX_PING_PAYLOAD = 64

        /**
         * Connects, completes the TLS handshake against the pinned identity,
         * and runs the protocol handshake.
         *
         * @param pairingToken when present, offers a PAIR_REQUEST if the
         *   desktop asks for one. Absent for a normal reconnection.
         */
        suspend fun connect(
            address: InetSocketAddress,
            identity: DeviceIdentity,
            deviceName: String,
            pinned: Fingerprint,
            registry: CapabilityRegistry,
            pairingToken: ByteArray? = null,
            connectTimeoutMs: Int = 8_000,
        ): ConnectResult = withContext(Dispatchers.IO) {
            val socket = try {
                openTls(address, identity, pinned, connectTimeoutMs)
            } catch (e: Exception) {
                // A pinning failure lands here. It is reported as an ordinary
                // failure to the UI, but it is never retried against a
                // different key and never bypassed.
                //
                // The cause is logged because a swallowed TLS error is
                // undiagnosable in the field. The exception carries no key
                // material: PinnedTrustManager's message contains only
                // fingerprints, which are public.
                Log.w(TAG, "TLS failed to ${address.hostString}:${address.port}", e)
                val kind = FailureClassifier.classify(e)
                return@withContext ConnectResult.Failed(
                    when (kind) {
                        FailureKind.SECURITY -> "the computer did not present the pinned identity"
                        else -> "could not establish a trusted connection"
                    },
                    kind,
                    e,
                )
            }

            try {
                handshake(
                    socket, identity, deviceName, pinned, registry, pairingToken,
                )
            } catch (e: Exception) {
                runCatching { socket.close() }
                ConnectResult.Failed(
                    "handshake failed: ${e.javaClass.simpleName}",
                    FailureClassifier.classify(e),
                    e,
                )
            }
        }

        private fun openTls(
            address: InetSocketAddress,
            identity: DeviceIdentity,
            pinned: Fingerprint,
            connectTimeoutMs: Int,
        ): SSLSocket {
            val context = TlsFactory.sslContext(identity, pinned)
            val plain = Socket()
            plain.connect(
                InetSocketAddress(address.hostString, address.port),
                connectTimeoutMs,
            )
            val socket = context.socketFactory.createSocket(
                plain, address.hostString, address.port, true,
            ) as SSLSocket
            TlsFactory.harden(socket)
            socket.soTimeout = Protocol.HANDSHAKE_TIMEOUT_MS
            // Forces the handshake now, so a pinning failure surfaces here
            // rather than on the first read.
            socket.startHandshake()
            return socket
        }

        private suspend fun handshake(
            socket: SSLSocket,
            identity: DeviceIdentity,
            deviceName: String,
            pinned: Fingerprint,
            registry: CapabilityRegistry,
            pairingToken: ByteArray?,
        ): ConnectResult {
            val input = socket.inputStream
            val output = socket.outputStream
            val factory = EnvelopeFactory(Protocol.VERSION_MAX)
            val guard = ReplayGuard()

            val localInfo = DeviceInfo.newBuilder()
                .setDeviceId(identity.deviceId)
                .setDeviceName(deviceName)
                .setPlatform(Platform.PLATFORM_ANDROID)
                .setIdentityFingerprint(identity.fingerprint.toHex())
                .build()

            Framing.write(
                output,
                factory.build {
                    hello = Hello.newBuilder()
                        .setDevice(localInfo)
                        .setMinProtocolVersion(Protocol.VERSION_MIN)
                        .setMaxProtocolVersion(Protocol.VERSION_MAX)
                        .addAllCapabilities(registry.advertised())
                        .build()
                },
            )

            val ackEnvelope = Framing.read(input)
            guard.admit(ackEnvelope)?.let {
                return ConnectResult.Failed("bad HELLO_ACK: $it")
            }
            if (ackEnvelope.bodyCase != Envelope.BodyCase.HELLO_ACK) {
                return ConnectResult.Failed("expected HELLO_ACK")
            }
            val ack = ackEnvelope.helloAck

            // The identity the desktop claims must be the key it actually
            // authenticated with, or a trusted device could be impersonated at
            // the application layer.
            val claimed = Fingerprint.fromHex(ack.device.identityFingerprint)
                ?: return ConnectResult.Failed("malformed fingerprint in HELLO_ACK")
            if (!claimed.contentEquals(pinned)) {
                return ConnectResult.Failed(
                    "HELLO_ACK identity does not match the pinned key",
                    FailureKind.SECURITY,
                )
            }

            val version = ack.negotiatedProtocolVersion
            when (ack.status) {
                HelloStatus.HELLO_STATUS_VERSION_UNSUPPORTED ->
                    return ConnectResult.Failed("no mutually supported protocol version")
                HelloStatus.HELLO_STATUS_REJECTED ->
                    // Terminal: retrying cannot help and only a human can
                    // change the answer.
                    return ConnectResult.Failed(
                        "this device's pairing was revoked",
                        FailureKind.REVOKED,
                    )
                HelloStatus.HELLO_STATUS_TRUSTED -> {
                    if (version !in Protocol.VERSION_MIN..Protocol.VERSION_MAX) {
                        return ConnectResult.Failed("desktop selected an unsupported version")
                    }
                    factory.protocolVersion = version
                    // The desktop already knows this device, so it asked for
                    // no proof and none was given — whether or not this dial
                    // was carrying a freshly scanned token.
                    return established(
                        socket, input, output, factory, guard, registry, pinned,
                        ack.device, registry.negotiate(ack.capabilitiesList), version,
                        provedPairing = false,
                    )
                }
                HelloStatus.HELLO_STATUS_PAIRING_REQUIRED -> Unit
                else -> return ConnectResult.Failed("HELLO_ACK with an unknown status")
            }

            val token = pairingToken ?: return ConnectResult.PairingRequired
            if (ack.pairingNonce.size() != Protocol.NONCE_LENGTH) {
                return ConnectResult.Failed("desktop is not in pairing mode")
            }
            if (version !in Protocol.VERSION_MIN..Protocol.VERSION_MAX) {
                return ConnectResult.Failed("desktop selected an unsupported version")
            }
            factory.protocolVersion = version

            val nonce = ack.pairingNonce.toByteArray()
            val proof = PairingProof.compute(token, pinned, identity.fingerprint, nonce)

            Framing.write(
                output,
                factory.build {
                    pairRequest = PairRequest.newBuilder()
                        .setProof(ByteString.copyFrom(proof))
                        .build()
                },
            )

            // The desktop is waiting for a human to confirm, so allow well
            // over the handshake timeout here.
            socket.soTimeout = PAIRING_CONFIRM_TIMEOUT_MS
            val responseEnvelope = Framing.read(input)
            socket.soTimeout = 0
            guard.admit(responseEnvelope)?.let {
                return ConnectResult.Failed("bad PAIR_RESPONSE: $it")
            }
            if (responseEnvelope.bodyCase != Envelope.BodyCase.PAIR_RESPONSE) {
                return ConnectResult.Failed("expected PAIR_RESPONSE")
            }
            val response = responseEnvelope.pairResponse

            if (response.status != PairStatus.PAIR_STATUS_ACCEPTED) {
                return ConnectResult.Failed(
                    when (response.status) {
                        PairStatus.PAIR_STATUS_DECLINED_BY_USER -> "declined on the computer"
                        PairStatus.PAIR_STATUS_NOT_IN_PAIRING_MODE ->
                            "the computer is not in pairing mode"
                        PairStatus.PAIR_STATUS_RATE_LIMITED -> "too many failed attempts"
                        else -> "the pairing code was rejected"
                    },
                )
            }

            // Mutual: the desktop must prove it also held the token. Without
            // this the phone would accept a pairing from a peer that merely
            // owned the pinned key.
            val expected = PairingProof.computeConfirmation(
                token, pinned, identity.fingerprint, nonce,
            )
            if (!PairingProof.verify(expected, response.confirmation.toByteArray())) {
                return ConnectResult.Failed(
                    "the computer did not prove it knew the pairing code",
                    FailureKind.SECURITY,
                )
            }

            // Reached only after the proof was sent, PAIR_RESPONSE came back
            // accepted, and the responder's own confirmation verified. Every
            // one of those is above; none of them is skippable.
            return established(
                socket, input, output, factory, guard, registry, pinned,
                ack.device, registry.negotiate(ack.capabilitiesList), version,
                provedPairing = true,
            )
        }

        @Suppress("LongParameterList")
        private fun established(
            socket: SSLSocket,
            input: InputStream,
            output: OutputStream,
            factory: EnvelopeFactory,
            guard: ReplayGuard,
            registry: CapabilityRegistry,
            peer: Fingerprint,
            device: DeviceInfo,
            capabilities: List<String>,
            version: Int,
            provedPairing: Boolean,
        ): ConnectResult {
            // An idle connection is normal, so the timeout is not a deadline
            // for useful traffic — it is a liveness bound. The desktop probes
            // a quiet link once a minute, so nothing at all for three minutes
            // means the link is gone. Leaving this at 0, as it was, meant a
            // phone that lost Wi-Fi without the socket closing sat in `read`
            // forever believing it was connected.
            socket.soTimeout = Protocol.SESSION_IDLE_TIMEOUT_MS
            return ConnectResult.Established(
                PeerConnection(
                    socket, input, output, factory, guard, registry,
                    peer, device, capabilities, version,
                ),
                provedPairing,
            )
        }

        private const val PAIRING_CONFIRM_TIMEOUT_MS = 90_000
    }
}
