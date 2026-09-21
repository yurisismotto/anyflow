package io.github.yurisismotto.omnibridge

import io.github.yurisismotto.omnibridge.identity.Fingerprint
import io.github.yurisismotto.omnibridge.pairing.QrPayload
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * A scanned QR code is attacker-controlled input: the user may be pointing
 * the camera at anything. These tests pin the parser's strictness.
 */
class QrPayloadTest {

    private val fingerprintHex = "a".repeat(64)
    private val tokenBase32 = "FUPJRRCMFSG6KYYHR5ODB5BBLM7HVSCZ" // 32 chars = 20 bytes
    private val deviceId = "deb769060ca6dce3bee7a92f6a82e606"

    private fun payload(
        scheme: String = "omnibridge1",
        fingerprint: String = fingerprintHex,
        token: String = tokenBase32,
        device: String = deviceId,
        addresses: String = "192.168.1.10:55432",
    ) = "$scheme:$fingerprint:$token:$device:$addresses"

    @Test
    fun `parses a well formed payload`() {
        val parsed = QrPayload.parse(payload())
        assertNotNull(parsed)
        assertEquals(deviceId, parsed!!.deviceId)
        assertEquals(fingerprintHex, parsed.fingerprint.toHex())
        assertEquals(QrPayload.TOKEN_LENGTH, parsed.token.size)
        assertEquals(1, parsed.addresses.size)
        assertEquals(55432, parsed.addresses[0].port)
    }

    @Test
    fun `keeps ipv6 addresses intact`() {
        val parsed = QrPayload.parse(payload(addresses = "[fe80::1]:55432,10.0.0.5:55432"))
        assertNotNull(parsed)
        assertEquals(2, parsed!!.addresses.size)
    }

    @Test
    fun `drops unparseable addresses without rejecting the code`() {
        // Addresses are hints; one bad entry must not waste a valid code.
        val parsed = QrPayload.parse(payload(addresses = "garbage,10.0.0.7:55432"))
        assertNotNull(parsed)
        assertEquals(1, parsed!!.addresses.size)
    }

    @Test
    fun `rejects a foreign scheme`() {
        assertNull(QrPayload.parse(payload(scheme = "omnibridge9")))
        assertNull(QrPayload.parse("http://evil.example/"))
        assertNull(QrPayload.parse(""))
    }

    @Test
    fun `rejects a malformed fingerprint`() {
        assertNull(QrPayload.parse(payload(fingerprint = "abc")))
        assertNull(QrPayload.parse(payload(fingerprint = "z".repeat(64))))
        // Uppercase is not the canonical spelling and must not be accepted,
        // or one identity would have two representations.
        assertNull(QrPayload.parse(payload(fingerprint = "A".repeat(64))))
    }

    @Test
    fun `rejects a token of the wrong length`() {
        assertNull(QrPayload.parse(payload(token = "AAAA")))
        assertNull(QrPayload.parse(payload(token = "A".repeat(64))))
    }

    @Test
    fun `rejects a malformed device id`() {
        assertNull(QrPayload.parse(payload(device = "")))
        assertNull(QrPayload.parse(payload(device = "not-hex-at-all")))
        assertNull(QrPayload.parse(payload(device = "a".repeat(200))))
    }

    @Test
    fun `refuses an oversized payload before parsing it`() {
        assertNull(QrPayload.parse("a".repeat(100_000)))
    }

    @Test
    fun `toString never leaks the token`() {
        val parsed = QrPayload.parse(payload())!!
        val rendered = parsed.toString()
        assertTrue(rendered.contains("<redacted>"))
        assertTrue(!rendered.contains(tokenBase32))
    }

    @Test
    fun `fingerprint hex round trips`() {
        val fp = Fingerprint.fromHex(fingerprintHex)
        assertNotNull(fp)
        assertEquals(fingerprintHex, fp!!.toHex())
        assertEquals(Fingerprint.LENGTH, fp.bytes.size)
    }

    @Test
    fun `short display form is grouped and truncated`() {
        val fp = Fingerprint.fromHex("0123456789abcdef".repeat(4))!!
        assertEquals("0123 4567 89AB CDEF", fp.toDisplayShort())
    }
}
