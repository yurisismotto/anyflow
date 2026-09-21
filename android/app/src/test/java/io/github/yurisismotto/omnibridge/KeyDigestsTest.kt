package io.github.yurisismotto.omnibridge

import io.github.yurisismotto.omnibridge.identity.KeyDigests
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Regression guard for the Keystore digest policy.
 *
 * A key generated without `NONE` cannot sign a TLS 1.3 `CertificateVerify`:
 * the TEE refuses with `INCOMPATIBLE_DIGEST` and the handshake dies with no
 * alert on the wire, so it presents as an unexplained network failure. It
 * cost a hardware session to find, and Keystore authorisations are immutable,
 * so the only remedy is a new key under a new alias.
 *
 * This runs on a plain JVM with no device, which is the point: the
 * instrumented `DeviceIdentityTest` proves the behaviour on real hardware,
 * but this fails in CI the moment someone tidies the list.
 */
class KeyDigestsTest {

    @Test
    fun `NONE is required, because TLS client authentication depends on it`() {
        assertTrue(
            "Removing NONE breaks TLS client auth silently. See KeyDigests.",
            KeyDigests.REQUIRED.contains(KeyDigests.NONE),
        )
    }

    @Test
    fun `the hashing digests the certificate needs are present too`() {
        assertTrue(KeyDigests.REQUIRED.contains(KeyDigests.SHA_256))
        assertTrue(KeyDigests.REQUIRED.contains(KeyDigests.SHA_384))
        assertTrue(KeyDigests.REQUIRED.contains(KeyDigests.SHA_512))
    }

    @Test
    fun `the values are the Keystore spellings`() {
        // KeyProperties.DIGEST_* are these exact strings. Getting one wrong
        // would be accepted by the builder and fail only at signing time.
        assertTrue(KeyDigests.NONE == "NONE")
        assertTrue(KeyDigests.SHA_256 == "SHA-256")
        assertTrue(KeyDigests.SHA_384 == "SHA-384")
        assertTrue(KeyDigests.SHA_512 == "SHA-512")
    }
}
