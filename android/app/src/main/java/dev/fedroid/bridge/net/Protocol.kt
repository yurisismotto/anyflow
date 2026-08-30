package dev.fedroid.bridge.net

import com.google.protobuf.ByteString
import dev.fedroid.bridge.proto.Envelope
import java.security.SecureRandom

/** Protocol constants. Must match `fedroid_core::session`. */
object Protocol {
    const val VERSION_MIN = 1
    const val VERSION_MAX = 1

    const val MESSAGE_ID_LENGTH = 16
    const val NONCE_LENGTH = 32

    /** Handshake read timeout, in milliseconds. */
    const val HANDSHAKE_TIMEOUT_MS = 15_000

    fun negotiateVersion(peerMin: Int, peerMax: Int): Int? {
        val chosen = minOf(peerMax, VERSION_MAX)
        val floor = maxOf(peerMin, VERSION_MIN)
        return if (chosen >= floor) chosen else null
    }
}

/**
 * Assigns envelope headers.
 *
 * One place owns sequence numbers, so they cannot be duplicated or reordered
 * by two call sites racing.
 */
class EnvelopeFactory(var protocolVersion: Int) {

    private val random = SecureRandom()
    private var sequence: Long = 0

    fun build(configure: Envelope.Builder.() -> Unit): Envelope {
        sequence += 1
        return Envelope.newBuilder()
            .setProtocolVersion(protocolVersion)
            .setMessageId(randomMessageId())
            .setSequence(sequence)
            // Informational only. Never a security input on either side,
            // because two devices' clocks routinely disagree.
            .setTimestampUnixMs(System.currentTimeMillis())
            .apply(configure)
            .build()
    }

    fun buildReply(correlate: ByteString, configure: Envelope.Builder.() -> Unit): Envelope =
        build {
            setCorrelationId(correlate)
            configure()
        }

    private fun randomMessageId(): ByteString {
        val bytes = ByteArray(Protocol.MESSAGE_ID_LENGTH)
        random.nextBytes(bytes)
        return ByteString.copyFrom(bytes)
    }
}

/**
 * Rejects replayed and duplicated envelopes within one connection.
 *
 * Mirrors the Rust `ReplayGuard`. TLS already stops an off-path attacker from
 * injecting or reordering records, so this is defence against a hostile or
 * broken *peer*, which TLS says nothing about.
 */
class ReplayGuard {

    private var lastSequence: Long = 0
    private val seen = LinkedHashSet<ByteString>()

    /** @return null when the envelope is acceptable, or a reason to close. */
    fun admit(envelope: Envelope): String? {
        if (envelope.messageId.size() != Protocol.MESSAGE_ID_LENGTH) {
            return "message_id must be ${Protocol.MESSAGE_ID_LENGTH} bytes"
        }
        if (envelope.sequence <= lastSequence) {
            return "sequence number did not increase"
        }
        if (!seen.add(envelope.messageId)) {
            return "duplicate message_id"
        }

        lastSequence = envelope.sequence
        if (seen.size > DEDUP_WINDOW) {
            val oldest = seen.first()
            seen.remove(oldest)
        }
        return null
    }

    private companion object {
        const val DEDUP_WINDOW = 1024
    }
}
