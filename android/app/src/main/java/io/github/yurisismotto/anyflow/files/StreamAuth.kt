package io.github.yurisismotto.anyflow.files

import io.github.yurisismotto.anyflow.identity.Fingerprint
import java.nio.ByteBuffer
import java.security.MessageDigest
import javax.crypto.Mac
import javax.crypto.spec.SecretKeySpec

/**
 * Authenticates a `files.v1` data stream.
 *
 * Byte-for-byte identical to `anyflow_capability_files::auth`, and the same
 * shape as [io.github.yurisismotto.anyflow.pairing.PairingProof]: a standard
 * MAC, a versioned domain separator, and every field length-prefixed so two
 * different field splits cannot produce the same message.
 *
 * ```text
 * mac = HMAC-SHA256(
 *     key = stream_challenge,
 *     msg = "anyflow/files.v1/data-stream/v1"
 *           || len_prefixed(acceptor_fingerprint)
 *           || len_prefixed(dialer_fingerprint)
 *           || len_prefixed(transfer_id))
 * ```
 *
 * Why this exists at all: TLS with a pinned identity answers "which device is
 * on this socket", which is necessary and not sufficient. It says nothing
 * about *which transfer* the connection is for. Letting the dialer simply
 * name a `transfer_id` would make that id a bearer token, and bearer tokens
 * leak. See ADR-0013.
 *
 * The phone is always the dialer: a phone is not a stable listener, so it
 * opens the stream in both directions of transfer and proves the challenge
 * the desktop issued.
 */
object StreamAuth {

    private val DOMAIN = "anyflow/files.v1/data-stream/v1".toByteArray(Charsets.US_ASCII)

    /** Length of a transfer id. Must match `TRANSFER_ID_LEN` on the desktop. */
    const val TRANSFER_ID_LENGTH = 16

    /** Length of the single-use challenge. Must match the desktop. */
    const val CHALLENGE_LENGTH = 32

    /**
     * A fresh transfer id.
     *
     * Random, and derived from nothing: not the filename, not the URI, not a
     * counter, not the clock. That is what makes every attempt at sending a
     * file a *new* transfer rather than a resumption of an old one — a
     * retry after a decline shares a name with what was declined and must
     * share nothing else, because the id is what the stream challenge is
     * bound to (see [compute]) and what the peer answers about.
     *
     * The generator is a parameter so the property can be tested without a
     * device; production always passes a `SecureRandom`.
     */
    fun newTransferId(random: java.util.Random): ByteArray =
        ByteArray(TRANSFER_ID_LENGTH).also { random.nextBytes(it) }

    /** Lowercase hex, which is how a transfer id is spelled everywhere else. */
    fun toHex(bytes: ByteArray): String = bytes.joinToString("") { "%02x".format(it) }

    /**
     * @param challenge the single-use secret the acceptor sent over the
     *   control session. Never logged and never persisted.
     * @param acceptor the device that accepts data streams and issued the
     *   challenge — the desktop.
     * @param dialer the device that opens the stream — this phone.
     */
    fun compute(
        challenge: ByteArray,
        acceptor: Fingerprint,
        dialer: Fingerprint,
        transferId: ByteArray,
    ): ByteArray {
        require(challenge.size == CHALLENGE_LENGTH) { "a challenge is $CHALLENGE_LENGTH bytes" }
        require(transferId.size == TRANSFER_ID_LENGTH) {
            "a transfer id is $TRANSFER_ID_LENGTH bytes"
        }

        val mac = Mac.getInstance("HmacSHA256")
        mac.init(SecretKeySpec(challenge, "HmacSHA256"))
        mac.update(DOMAIN)
        updateLengthPrefixed(mac, acceptor.bytes)
        updateLengthPrefixed(mac, dialer.bytes)
        updateLengthPrefixed(mac, transferId)
        return mac.doFinal()
    }

    private fun updateLengthPrefixed(mac: Mac, data: ByteArray) {
        mac.update(ByteBuffer.allocate(4).putInt(data.size).array())
        mac.update(data)
    }

    /**
     * Constant-time comparison. [MessageDigest.isEqual] is documented to be
     * time-constant; `contentEquals` is not, and leaks how many leading bytes
     * matched.
     */
    fun verify(expected: ByteArray, received: ByteArray): Boolean =
        MessageDigest.isEqual(expected, received)
}
