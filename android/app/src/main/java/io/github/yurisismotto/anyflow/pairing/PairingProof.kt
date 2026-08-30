package io.github.yurisismotto.anyflow.pairing

import io.github.yurisismotto.anyflow.identity.Fingerprint
import java.nio.ByteBuffer
import java.security.MessageDigest
import javax.crypto.Mac
import javax.crypto.spec.SecretKeySpec

/**
 * The pairing proof, byte-for-byte identical to `anyflow_core::pairing`.
 *
 * ```text
 * proof = HMAC-SHA256(
 *     key = token,
 *     msg = domain
 *           || len_prefixed(responder_fingerprint)
 *           || len_prefixed(initiator_fingerprint)
 *           || len_prefixed(nonce))
 * ```
 *
 * Every field is length-prefixed so that no two different field splits can
 * produce the same message. The domain separator is what keeps a captured
 * proof from being replayed back as a confirmation.
 *
 * Standard HMAC-SHA256 from the platform provider. Nothing bespoke.
 */
object PairingProof {

    private val PROOF_DOMAIN = "anyflow/pairing-proof/v1".toByteArray(Charsets.US_ASCII)
    private val CONFIRM_DOMAIN =
        "anyflow/pairing-confirm/v1".toByteArray(Charsets.US_ASCII)

    fun compute(
        token: ByteArray,
        responder: Fingerprint,
        initiator: Fingerprint,
        nonce: ByteArray,
    ): ByteArray = mac(PROOF_DOMAIN, token, responder, initiator, nonce)

    fun computeConfirmation(
        token: ByteArray,
        responder: Fingerprint,
        initiator: Fingerprint,
        nonce: ByteArray,
    ): ByteArray = mac(CONFIRM_DOMAIN, token, responder, initiator, nonce)

    private fun mac(
        domain: ByteArray,
        token: ByteArray,
        responder: Fingerprint,
        initiator: Fingerprint,
        nonce: ByteArray,
    ): ByteArray {
        val mac = Mac.getInstance("HmacSHA256")
        mac.init(SecretKeySpec(token, "HmacSHA256"))
        mac.update(domain)
        updateLengthPrefixed(mac, responder.bytes)
        updateLengthPrefixed(mac, initiator.bytes)
        updateLengthPrefixed(mac, nonce)
        return mac.doFinal()
    }

    private fun updateLengthPrefixed(mac: Mac, data: ByteArray) {
        mac.update(ByteBuffer.allocate(4).putInt(data.size).array())
        mac.update(data)
    }

    /**
     * Constant-time comparison.
     *
     * [MessageDigest.isEqual] is documented to be time-constant, which
     * `contentEquals` is not. Comparing a MAC with an early-exit loop leaks
     * how many leading bytes matched.
     */
    fun verify(expected: ByteArray, received: ByteArray): Boolean =
        MessageDigest.isEqual(expected, received)
}
