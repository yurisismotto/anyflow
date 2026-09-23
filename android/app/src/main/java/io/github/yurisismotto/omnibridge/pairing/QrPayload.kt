package io.github.yurisismotto.omnibridge.pairing

import io.github.yurisismotto.omnibridge.identity.Fingerprint
import java.net.InetSocketAddress

/**
 * Parses the QR code shown by the desktop.
 *
 * ```text
 * omnibridge1:<responder-fingerprint-hex>:<token-base32>:<device-id>:<addr>[,<addr>]
 * ```
 *
 * Everything in a scanned code is attacker-controlled: the user may well be
 * pointing the camera at something hostile. The parser is therefore strict
 * about structure, bounded in size, and returns null rather than throwing on
 * anything unexpected.
 *
 * The fingerprint is the important field. It is pinned *before* the socket is
 * opened, which is what removes the man-in-the-middle window that a
 * trust-on-first-use design would have.
 */
data class QrPayload(
    val fingerprint: Fingerprint,
    val token: ByteArray,
    val deviceId: String,
    val addresses: List<InetSocketAddress>,
) {
    companion object {
        const val SCHEME = "omnibridge1"
        const val MAX_LENGTH = 512
        const val TOKEN_LENGTH = 20

        fun parse(input: String): QrPayload? {
            if (input.length > MAX_LENGTH) return null

            // Addresses contain ':' (ports and IPv6), so split into exactly
            // five parts and keep the remainder as the address list.
            val parts = input.split(':', limit = 5)
            if (parts.size < 4) return null
            if (parts[0] != SCHEME) return null

            val fingerprint = Fingerprint.fromHex(parts[1]) ?: return null
            val token = Base32.decode(parts[2]) ?: return null
            if (token.size != TOKEN_LENGTH) return null

            val deviceId = parts[3]
            if (deviceId.isEmpty() || deviceId.length > 64) return null
            if (!deviceId.all { it in '0'..'9' || it in 'a'..'f' }) return null

            // Addresses are hints only: one unparseable entry must not make an
            // otherwise valid code unusable.
            val addresses = parts.getOrNull(4)
                ?.split(',')
                ?.mapNotNull(::parseAddress)
                ?: emptyList()

            return QrPayload(fingerprint, token, deviceId, addresses)
        }

        private fun parseAddress(text: String): InetSocketAddress? {
            val trimmed = text.trim()
            if (trimmed.isEmpty()) return null
            val separator = trimmed.lastIndexOf(':')
            if (separator <= 0) return null
            val host = trimmed.substring(0, separator).removeSurrounding("[", "]")
            val port = trimmed.substring(separator + 1).toIntOrNull() ?: return null
            if (port !in 1..65535) return null
            return runCatching { InetSocketAddress.createUnresolved(host, port) }.getOrNull()
        }
    }

    // ByteArray in a data class needs these to behave sanely.
    override fun equals(other: Any?): Boolean {
        if (this === other) return true
        if (other !is QrPayload) return false
        return fingerprint.contentEquals(other.fingerprint) &&
            token.contentEquals(other.token) &&
            deviceId == other.deviceId &&
            addresses == other.addresses
    }

    override fun hashCode(): Int = deviceId.hashCode() * 31 + token.contentHashCode()

    /** Never let a token reach a log or a crash report. */
    override fun toString(): String =
        "QrPayload(fingerprint=${fingerprint.toDisplayShort()}, deviceId=$deviceId, " +
            "addresses=$addresses, token=<redacted>)"
}

/** RFC 4648 base32, unpadded, uppercase. Matches `data_encoding::BASE32_NOPAD`. */
internal object Base32 {
    private const val ALPHABET = "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567"

    fun decode(input: String): ByteArray? {
        if (input.isEmpty()) return null
        var buffer = 0L
        var bits = 0
        val out = java.io.ByteArrayOutputStream()
        for (c in input) {
            val value = ALPHABET.indexOf(c)
            if (value < 0) return null
            buffer = (buffer shl 5) or value.toLong()
            bits += 5
            if (bits >= 8) {
                bits -= 8
                out.write(((buffer shr bits) and 0xFF).toInt())
            }
        }
        // Any leftover bits must be zero padding; anything else is a
        // malformed encoding rather than something to silently truncate.
        if (bits >= 5) return null
        if (bits > 0 && (buffer and ((1L shl bits) - 1)) != 0L) return null
        return out.toByteArray()
    }
}
