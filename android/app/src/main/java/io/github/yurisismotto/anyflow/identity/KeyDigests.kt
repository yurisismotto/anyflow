package io.github.yurisismotto.anyflow.identity

/**
 * The digest algorithms this device's identity key must be authorised for.
 *
 * ## Why `NONE` is not optional
 *
 * For a TLS 1.3 `CertificateVerify`, Conscrypt hashes the handshake
 * transcript itself and then asks the key to sign the resulting digest raw,
 * through `NONEwithECDSA`. A Keystore key generated without [NONE] makes the
 * TEE refuse that operation with `INCOMPATIBLE_DIGEST`. The handshake then
 * dies with no alert on the wire, so the failure presents as an unexplained
 * connection problem rather than as a key problem — which is exactly how it
 * presented on a Galaxy S25, and why it took a hardware session to find.
 *
 * Keystore authorisations are immutable, so a key generated without it cannot
 * be repaired. That is why [DeviceIdentity] versions its alias rather than
 * trying to fix an existing entry.
 *
 * ## Why the values are spelled out here
 *
 * These are the same strings as `KeyProperties.DIGEST_*`, deliberately
 * restated in a file with no Android dependency, so that the rule can be
 * asserted by a plain JVM unit test with no device attached. The instrumented
 * `DeviceIdentityTest` checks both that this list matches `KeyProperties` and
 * that a generated key really can produce a `NONEwithECDSA` signature — the
 * only test that proves the thing end to end.
 *
 * If you are about to remove an entry from [REQUIRED]: `NONE` is the one that
 * breaks TLS client authentication, silently.
 */
object KeyDigests {
    const val NONE = "NONE"
    const val SHA_256 = "SHA-256"
    const val SHA_384 = "SHA-384"
    const val SHA_512 = "SHA-512"

    val REQUIRED: Array<String> = arrayOf(NONE, SHA_256, SHA_384, SHA_512)
}
