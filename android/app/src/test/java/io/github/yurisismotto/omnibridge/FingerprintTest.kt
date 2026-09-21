package io.github.yurisismotto.omnibridge

import io.github.yurisismotto.omnibridge.identity.Fingerprint
import java.security.MessageDigest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The identity is `SHA-256(DER SubjectPublicKeyInfo)`. If this diverges from
 * `omnibridge_core::fingerprint`, every pairing breaks, so the fixtures below are
 * the same ones the Rust suite asserts against.
 */
class FingerprintTest {

    @Test
    fun `fixture certificates have the expected SPKI fingerprints`() {
        assertEquals(
            Fixtures.IDENTITY_A_FINGERPRINT,
            Fingerprint.ofCertificate(Fixtures.identityA()).toHex(),
        )
        assertEquals(
            Fixtures.IDENTITY_B_FINGERPRINT,
            Fingerprint.ofCertificate(Fixtures.identityB()).toHex(),
        )
    }

    @Test
    fun `fingerprint covers the public key not the whole certificate`() {
        // Reissuing a certificate from the same key must not change the
        // identity. Hashing the certificate bytes would break that, so assert
        // the two are genuinely different values.
        val der = Fixtures.certificateDer("identity-a.der")
        val overCertificate = Fingerprint.ofSpki(der)
        val overSpki = Fingerprint.ofCertificate(Fixtures.identityA())
        assertNotEquals(overCertificate.toHex(), overSpki.toHex())
        assertEquals(Fixtures.IDENTITY_A_FINGERPRINT, overSpki.toHex())
    }

    @Test
    fun `ofSpki is plain SHA-256 over the encoded public key`() {
        val spki = Fixtures.identityA().publicKey.encoded
        val expected = MessageDigest.getInstance("SHA-256").digest(spki)
        assertEquals(
            expected.joinToString("") { "%02x".format(it) },
            Fingerprint.ofSpki(spki).toHex(),
        )
    }

    @Test
    fun `two different identities do not collide`() {
        val a = Fingerprint.ofCertificate(Fixtures.identityA())
        val b = Fingerprint.ofCertificate(Fixtures.identityB())
        assertFalse(a.contentEquals(b))
    }

    @Test
    fun `hex parsing is strict`() {
        val valid = Fixtures.IDENTITY_A_FINGERPRINT
        assertEquals(valid, Fingerprint.fromHex(valid)!!.toHex())

        // Uppercase is refused so one identity has exactly one spelling.
        assertNull(Fingerprint.fromHex(valid.uppercase()))
        assertNull(Fingerprint.fromHex(valid.dropLast(2)))
        assertNull(Fingerprint.fromHex(valid + "00"))
        assertNull(Fingerprint.fromHex(""))
        assertNull(Fingerprint.fromHex("z".repeat(64)))
    }

    @Test
    fun `short display form is truncated and never parseable back`() {
        val fp = Fingerprint.fromHex(Fixtures.IDENTITY_A_FINGERPRINT)!!
        val short = fp.toDisplayShort()
        assertEquals("1B75 9FB3 23A5 C526", short)
        // Display only: it must not round trip into a trust decision.
        assertNull(Fingerprint.fromHex(short))
        assertTrue(short.length < Fingerprint.LENGTH * 2)
    }
}
