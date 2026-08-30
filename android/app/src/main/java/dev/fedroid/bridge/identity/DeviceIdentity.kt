package dev.fedroid.bridge.identity

import android.os.Build
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Log
import java.math.BigInteger
import java.security.KeyPairGenerator
import java.security.KeyStore
import java.security.PrivateKey
import java.security.SecureRandom
import java.security.cert.X509Certificate
import java.security.spec.ECGenParameterSpec
import java.util.Calendar
import javax.security.auth.x500.X500Principal

/**
 * This device's cryptographic identity.
 *
 * ## Where the private key lives
 *
 * In the Android Keystore, hardware-backed (StrongBox when the device has a
 * secure element, otherwise the TEE). The key is generated inside the
 * keystore and is **never** exportable: this class can ask for signatures but
 * cannot read the key, and neither can anything else in the app, including a
 * future bug in it. A rooted-device attacker cannot extract it either, which
 * is the difference between "the phone was compromised" and "the identity is
 * compromised forever".
 *
 * ## Why ECDSA P-256
 *
 * It is the algorithm Android Keystore supports with hardware backing across
 * essentially the whole fleet, and it is usable as a TLS client-certificate
 * key through Conscrypt. Ed25519 is a nicer primitive on paper but is neither
 * universally hardware-backed nor usable for TLS client auth here, and
 * keeping the key inside the TEE matters more. See ADR-0006.
 *
 * ## The certificate
 *
 * Android Keystore self-signs a certificate for the generated key pair. We
 * use it verbatim as the TLS client certificate. There is no CA and no chain:
 * trust comes from the desktop pinning this key's SPKI fingerprint during
 * pairing.
 */
class DeviceIdentity private constructor(
    val alias: String,
    val certificate: X509Certificate,
    val privateKey: PrivateKey,
    val deviceId: String,
    val isStrongBoxBacked: Boolean,
) {

    /** SHA-256 over the DER SubjectPublicKeyInfo, lowercase hex. */
    val fingerprint: Fingerprint by lazy {
        Fingerprint.ofCertificate(certificate)
    }

    companion object {
        private const val TAG = "DeviceIdentity"
        private const val KEY_ALIAS = "fedroid-bridge-identity-v1"
        private const val KEYSTORE = "AndroidKeyStore"

        /**
         * Loads the existing identity, generating one on first run.
         *
         * @param deviceId the persistent random device id, supplied by the
         *   caller so that identity generation and id generation stay in one
         *   transaction at the storage layer.
         */
        fun loadOrCreate(deviceId: String): DeviceIdentity {
            val keyStore = KeyStore.getInstance(KEYSTORE).apply { load(null) }

            keyStore.getEntry(KEY_ALIAS, null)?.let { entry ->
                if (entry is KeyStore.PrivateKeyEntry) {
                    val cert = entry.certificate as X509Certificate
                    return DeviceIdentity(
                        alias = KEY_ALIAS,
                        certificate = cert,
                        privateKey = entry.privateKey,
                        deviceId = deviceId,
                        isStrongBoxBacked = false,
                    )
                }
            }

            return generate(deviceId)
        }

        private fun generate(deviceId: String): DeviceIdentity {
            // Try StrongBox first, fall back to the TEE. StrongBox is absent
            // on many devices and throws only at generation time, so the
            // fallback has to be a catch rather than a capability query.
            val strongBox = Build.VERSION.SDK_INT >= Build.VERSION_CODES.P
            if (strongBox) {
                runCatching { generateWith(deviceId, useStrongBox = true) }
                    .onSuccess { return it }
                    .onFailure { Log.i(TAG, "StrongBox unavailable; using TEE-backed key") }
            }
            return generateWith(deviceId, useStrongBox = false)
        }

        private fun generateWith(deviceId: String, useStrongBox: Boolean): DeviceIdentity {
            val notBefore = Calendar.getInstance()
            val notAfter = Calendar.getInstance().apply { add(Calendar.YEAR, 10) }

            val spec = KeyGenParameterSpec.Builder(
                KEY_ALIAS,
                KeyProperties.PURPOSE_SIGN or KeyProperties.PURPOSE_VERIFY,
            )
                .setAlgorithmParameterSpec(ECGenParameterSpec("secp256r1"))
                .setDigests(
                    KeyProperties.DIGEST_SHA256,
                    KeyProperties.DIGEST_SHA384,
                    KeyProperties.DIGEST_SHA512,
                )
                // No biometric or lock-screen gate: the connection must be
                // able to re-establish while the phone is in a pocket. The
                // key is still confined to the TEE.
                .setUserAuthenticationRequired(false)
                .setCertificateSubject(X500Principal("CN=fedroid:$deviceId"))
                .setCertificateSerialNumber(BigInteger(64, SecureRandom()))
                .setCertificateNotBefore(notBefore.time)
                .setCertificateNotAfter(notAfter.time)
                .apply {
                    if (useStrongBox && Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
                        setIsStrongBoxBacked(true)
                    }
                }
                .build()

            val generator = KeyPairGenerator.getInstance(
                KeyProperties.KEY_ALGORITHM_EC,
                KEYSTORE,
            )
            generator.initialize(spec)
            generator.generateKeyPair()

            val keyStore = KeyStore.getInstance(KEYSTORE).apply { load(null) }
            val entry = keyStore.getEntry(KEY_ALIAS, null) as KeyStore.PrivateKeyEntry

            return DeviceIdentity(
                alias = KEY_ALIAS,
                certificate = entry.certificate as X509Certificate,
                privateKey = entry.privateKey,
                deviceId = deviceId,
                isStrongBoxBacked = useStrongBox,
            )
        }

        /** Destroys the identity. Every existing pairing becomes unusable. */
        fun delete() {
            runCatching {
                KeyStore.getInstance(KEYSTORE).apply { load(null) }.deleteEntry(KEY_ALIAS)
            }
        }
    }
}
