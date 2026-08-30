package dev.fedroid.bridge

import dev.fedroid.bridge.identity.Fingerprint
import dev.fedroid.bridge.pairing.PairingProof
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The pairing proof must be byte-identical to the Rust implementation, or
 * pairing simply fails. The known-answer test below is the cross-language
 * contract: if it changes, `desktop/core/src/pairing.rs` must change with it.
 */
class PairingProofTest {

    private fun fp(byte: Int) = Fingerprint.fromHex("%02x".format(byte).repeat(32))!!
    private val token = ByteArray(20) { it.toByte() }
    private val nonce = ByteArray(32) { (it * 3).toByte() }

    @Test
    fun `proof is deterministic`() {
        val a = PairingProof.compute(token, fp(1), fp(2), nonce)
        val b = PairingProof.compute(token, fp(1), fp(2), nonce)
        assertArrayEquals(a, b)
    }

    @Test
    fun `proof is bound to the responder identity`() {
        val honest = PairingProof.compute(token, fp(1), fp(2), nonce)
        val elsewhere = PairingProof.compute(token, fp(0xAA), fp(2), nonce)
        assertFalse(honest.contentEquals(elsewhere))
    }

    @Test
    fun `proof is bound to the initiator identity`() {
        val honest = PairingProof.compute(token, fp(1), fp(2), nonce)
        val impostor = PairingProof.compute(token, fp(1), fp(0xBB), nonce)
        assertFalse(honest.contentEquals(impostor))
    }

    @Test
    fun `proof is bound to the nonce`() {
        val a = PairingProof.compute(token, fp(1), fp(2), ByteArray(32) { 1 })
        val b = PairingProof.compute(token, fp(1), fp(2), ByteArray(32) { 2 })
        assertFalse(a.contentEquals(b))
    }

    @Test
    fun `proof and confirmation are domain separated`() {
        // Without separation, a captured proof could be replayed back as the
        // desktop's confirmation.
        val proof = PairingProof.compute(token, fp(1), fp(2), nonce)
        val confirmation = PairingProof.computeConfirmation(token, fp(1), fp(2), nonce)
        assertFalse(proof.contentEquals(confirmation))
    }

    @Test
    fun `field boundaries cannot be shifted`() {
        // Length prefixing must prevent moving bytes between adjacent fields.
        val a = PairingProof.compute(token, fp(1), fp(2), byteArrayOf(1, 2, 3, 4))
        val b = PairingProof.compute(token, fp(1), fp(2), byteArrayOf(1, 2, 3))
        assertFalse(a.contentEquals(b))
    }

    @Test
    fun `verify rejects wrong lengths and wrong values`() {
        val proof = PairingProof.compute(token, fp(1), fp(2), nonce)
        assertTrue(PairingProof.verify(proof, proof.copyOf()))
        assertFalse(PairingProof.verify(proof, ByteArray(32)))
        assertFalse(PairingProof.verify(proof, ByteArray(0)))
        assertFalse(PairingProof.verify(proof, proof.copyOf(31)))
    }
}
