package dev.fedroid.bridge.identity

import java.security.MessageDigest
import java.security.cert.Certificate

/**
 * SHA-256 over a DER SubjectPublicKeyInfo.
 *
 * We fingerprint the public key, not the certificate, so a certificate can be
 * reissued from the same key without invalidating an existing pairing. This
 * mirrors [`fedroid_core::fingerprint`] on the desktop byte for byte.
 */
@JvmInline
value class Fingerprint(val bytes: ByteArray) {

    init {
        require(bytes.size == LENGTH) { "a fingerprint is $LENGTH bytes" }
    }

    fun toHex(): String = bytes.joinToString("") { "%02x".format(it) }

    /**
     * Groups for human comparison, e.g. `A1B2 C3D4 E5F6 0718`.
     *
     * Display only. Never parse this back and never make a trust decision
     * from it: 64 bits is not enough to resist a deliberate collision search.
     */
    fun toDisplayShort(): String =
        bytes.take(8).joinToString("") { "%02X".format(it) }.chunked(4).joinToString(" ")

    fun contentEquals(other: Fingerprint): Boolean = bytes.contentEquals(other.bytes)

    companion object {
        const val LENGTH = 32

        fun ofCertificate(certificate: Certificate): Fingerprint {
            val spki = certificate.publicKey.encoded
                ?: error("public key has no DER encoding")
            return ofSpki(spki)
        }

        fun ofSpki(spki: ByteArray): Fingerprint =
            Fingerprint(MessageDigest.getInstance("SHA-256").digest(spki))

        /** Strict: lowercase hex only, so one identity has exactly one spelling. */
        fun fromHex(hex: String): Fingerprint? {
            if (hex.length != LENGTH * 2) return null
            if (!hex.all { it in '0'..'9' || it in 'a'..'f' }) return null
            val out = ByteArray(LENGTH)
            for (i in 0 until LENGTH) {
                out[i] = hex.substring(i * 2, i * 2 + 2).toInt(16).toByte()
            }
            return Fingerprint(out)
        }
    }
}
