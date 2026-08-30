package io.github.yurisismotto.anyflow

import io.github.yurisismotto.anyflow.identity.Fingerprint
import io.github.yurisismotto.anyflow.net.PinnedTrustManager
import java.security.cert.CertificateException
import java.security.cert.X509Certificate
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The pinning check is the whole of the app's transport security, so it is
 * tested against real X.509 certificates produced by the real desktop
 * implementation rather than against a stub.
 *
 * What is *not* covered here: proof of private-key possession. That is the TLS
 * stack's handshake signature check, which no unit test can meaningfully
 * stand in for, and which is exercised end-to-end by the Rust suite over a
 * real TLS 1.3 connection.
 */
class PinnedTrustManagerTest {

    private fun pinnedTo(hex: String) =
        PinnedTrustManager(Fingerprint.fromHex(hex)!!)

    private fun chainOf(vararg certs: X509Certificate) = certs

    @Test
    fun `accepts the pinned identity`() {
        pinnedTo(Fixtures.IDENTITY_A_FINGERPRINT)
            .checkServerTrusted(chainOf(Fixtures.identityA()), "ECDHE")
    }

    @Test
    fun `rejects a valid certificate belonging to a different identity`() {
        // identity-b is a perfectly well formed, currently valid certificate.
        // It is refused purely because it is not the pinned key.
        val error = assertThrows(CertificateException::class.java) {
            pinnedTo(Fixtures.IDENTITY_A_FINGERPRINT)
                .checkServerTrusted(chainOf(Fixtures.identityB()), "ECDHE")
        }
        assertTrue(error.message!!.contains("identity mismatch"))
    }

    @Test
    fun `rejects an empty chain`() {
        assertThrows(CertificateException::class.java) {
            pinnedTo(Fixtures.IDENTITY_A_FINGERPRINT).checkServerTrusted(emptyArray(), "ECDHE")
        }
    }

    @Test
    fun `rejects a null chain`() {
        assertThrows(CertificateException::class.java) {
            pinnedTo(Fixtures.IDENTITY_A_FINGERPRINT).checkServerTrusted(null, "ECDHE")
        }
    }

    @Test
    fun `pins the leaf, not any certificate in the chain`() {
        // A hostile server may append the certificate we expect after its own.
        // Only the leaf may decide the identity.
        assertThrows(CertificateException::class.java) {
            pinnedTo(Fixtures.IDENTITY_A_FINGERPRINT)
                .checkServerTrusted(chainOf(Fixtures.identityB(), Fixtures.identityA()), "ECDHE")
        }
    }

    @Test
    fun `never accepts an inbound client`() {
        // This app is never a TLS server; wiring it in as one must fail loudly.
        assertThrows(CertificateException::class.java) {
            pinnedTo(Fixtures.IDENTITY_A_FINGERPRINT)
                .checkClientTrusted(chainOf(Fixtures.identityA()), "ECDHE")
        }
    }

    @Test
    fun `trusts no certificate authority`() {
        assertEquals(0, pinnedTo(Fixtures.IDENTITY_A_FINGERPRINT).acceptedIssuers.size)
    }
}
