package io.github.yurisismotto.omnibridge

import java.io.ByteArrayInputStream
import java.security.cert.CertificateFactory
import java.security.cert.X509Certificate

/**
 * The shared cross-language fixtures in `protocol/testdata`.
 *
 * These are real certificates emitted by the real desktop identity code
 * (`cargo run -p omnibridge-core --example gen_test_vectors`), not something
 * hand-built for the test. `identity-a` plays the paired computer;
 * `identity-b` plays a different machine whose certificate is perfectly valid
 * and simply is not the pinned one.
 *
 * `desktop/core/tests/identity_and_store.rs` asserts the same fingerprints
 * against the same two files.
 */
object Fixtures {

    /** Must equal `FIXTURE_A_FINGERPRINT` in the Rust suite. */
    const val IDENTITY_A_FINGERPRINT =
        "1b759fb323a5c5260a0f762692d1f42458c5102f68e1e271699f1fa694769821"

    /** Must equal `FIXTURE_B_FINGERPRINT` in the Rust suite. */
    const val IDENTITY_B_FINGERPRINT =
        "f6b9ec37af6fa7cde4ea5377399c067cdfcb09def4d2ac701e11c95521d8efb7"

    fun certificateDer(name: String): ByteArray =
        checkNotNull(Fixtures::class.java.classLoader?.getResourceAsStream(name)) {
            "missing fixture $name; regenerate protocol/testdata"
        }.use { it.readBytes() }

    fun certificate(name: String): X509Certificate =
        CertificateFactory.getInstance("X.509")
            .generateCertificate(ByteArrayInputStream(certificateDer(name))) as X509Certificate

    fun identityA(): X509Certificate = certificate("identity-a.der")

    fun identityB(): X509Certificate = certificate("identity-b.der")
}
