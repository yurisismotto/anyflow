package io.github.yurisismotto.omnibridge

import android.security.keystore.KeyProperties
import androidx.test.ext.junit.runners.AndroidJUnit4
import io.github.yurisismotto.omnibridge.identity.DeviceIdentity
import io.github.yurisismotto.omnibridge.identity.KeyDigests
import java.security.Signature
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

/**
 * The real proof of the Keystore fix. Requires a device.
 *
 * ## What broke
 *
 * The identity key was generated authorising only SHA-256/384/512. For a TLS
 * 1.3 `CertificateVerify` Conscrypt hashes the transcript itself and asks the
 * key to sign the digest raw, through `NONEwithECDSA`. The TEE refused with
 * `INCOMPATIBLE_DIGEST`, the handshake died with no alert on the wire, and
 * the phone could not connect at all — while looking, from the outside, like
 * a network problem.
 *
 * `KeyDigestsTest` guards the policy on a plain JVM. This one goes further
 * and asks a real hardware key to actually produce the signature, which is
 * the only thing that proves the TEE agrees.
 */
@RunWith(AndroidJUnit4::class)
class DeviceIdentityTest {

    @Test
    fun theIdentityKeyCanProduceATls13CertificateVerifySignature() {
        val identity = DeviceIdentity.loadOrCreate("androidtest-device-id")

        // `NONEwithECDSA` over a 32-byte value is exactly what Conscrypt asks
        // for during a TLS 1.3 handshake with a P-256 key. Before the fix,
        // this line threw.
        val digest = ByteArray(32) { it.toByte() }
        val signature = Signature.getInstance("NONEwithECDSA").apply {
            initSign(identity.privateKey)
            update(digest)
        }.sign()

        assertTrue("the TEE must produce a signature", signature.isNotEmpty())

        val verifier = Signature.getInstance("NONEwithECDSA").apply {
            initVerify(identity.certificate.publicKey)
            update(digest)
        }
        assertTrue("and it must verify against the certificate", verifier.verify(signature))
    }

    @Test
    fun theDigestPolicyMatchesTheKeystoreConstants() {
        assertArrayEquals(
            arrayOf(
                KeyProperties.DIGEST_NONE,
                KeyProperties.DIGEST_SHA256,
                KeyProperties.DIGEST_SHA384,
                KeyProperties.DIGEST_SHA512,
            ),
            KeyDigests.REQUIRED,
        )
    }

    @Test
    fun noAnyFlowEraAliasIsCreated() {
        DeviceIdentity.loadOrCreate("androidtest-device-id")
        val keystore = java.security.KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        // The OmniBridge alias namespace restarts at v1 and owes nothing to
        // the AnyFlow-era aliases: under this applicationId they were never
        // reachable, so neither may appear here.
        assertNull(keystore.getKey("anyflow-identity-v1", null))
        assertNull(keystore.getKey("anyflow-identity-v2", null))
        assertNotNull(keystore.getKey("omnibridge-identity-v1", null))
    }

    @Test
    fun thePrivateKeyIsNotExportable() {
        val identity = DeviceIdentity.loadOrCreate("androidtest-device-id")
        // A Keystore handle has no encoding: the material never leaves the
        // TEE, and this is the observable consequence of that.
        assertNull(identity.privateKey.encoded)
    }
}
