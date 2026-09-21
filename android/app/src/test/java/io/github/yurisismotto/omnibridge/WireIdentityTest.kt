package io.github.yurisismotto.omnibridge

import io.github.yurisismotto.omnibridge.net.Discovery
import io.github.yurisismotto.omnibridge.net.TlsFactory
import io.github.yurisismotto.omnibridge.pairing.QrPayload
import io.github.yurisismotto.omnibridge.proto.Envelope
import io.github.yurisismotto.omnibridge.proto.capabilities.BatteryState
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The wire identities, pinned as literals.
 *
 * The mirror image of `desktop/core/tests/wire_identity.rs`, which spells out
 * the same strings from the Rust side. Every value here is a contract with
 * the desktop daemon: a peer that disagrees about any one of them does not
 * fail gracefully. A wrong ALPN fails the TLS handshake, a wrong service type
 * makes the daemon invisible to discovery, and a wrong QR prefix rejects a
 * valid pairing code — all of which look like "the network is broken" rather
 * than "somebody renamed a constant".
 *
 * Asserting `SERVICE_TYPE == SERVICE_TYPE` would prove nothing, so the
 * expected strings are written out. Changing an identity means changing both
 * files in the same commit. The values are recorded in ADR-0018.
 */
class WireIdentityTest {

    @Test
    fun `control ALPN is omnibridge 1`() {
        assertEquals("omnibridge/1", TlsFactory.ALPN_PROTOCOL)
    }

    @Test
    fun `data ALPN is omnibridge-data 1`() {
        assertEquals("omnibridge-data/1", TlsFactory.ALPN_DATA_PROTOCOL)
    }

    @Test
    fun `the two ALPN identifiers are distinct`() {
        assertNotEquals(TlsFactory.ALPN_PROTOCOL, TlsFactory.ALPN_DATA_PROTOCOL)
    }

    /**
     * Android's NSD wants the service type without the `.local.` suffix the
     * desktop's `mdns-sd` uses, so the two constants are not string-equal.
     * What has to agree is the label, which is what this asserts.
     */
    @Test
    fun `mDNS service type is the omnibridge tcp service`() {
        assertEquals("_omnibridge._tcp.", Discovery.SERVICE_TYPE)
        assertTrue(
            "the desktop advertises _omnibridge._tcp.local.",
            "_omnibridge._tcp.local.".startsWith(Discovery.SERVICE_TYPE),
        )
    }

    @Test
    fun `QR scheme is omnibridge1`() {
        assertEquals("omnibridge1", QrPayload.SCHEME)
    }

    @Test
    fun `no pre-rename identity survives`() {
        // A clean pre-v1 rename means a grep for a dead name is unambiguously
        // a bug (ADR-0011 "Consequences", carried into ADR-0018).
        for (dead in listOf("anyflow", "fedroid")) {
            assertFalse(TlsFactory.ALPN_PROTOCOL.contains(dead))
            assertFalse(TlsFactory.ALPN_DATA_PROTOCOL.contains(dead))
            assertFalse(Discovery.SERVICE_TYPE.contains(dead))
            assertFalse(QrPayload.SCHEME.contains(dead))
        }
    }

    /**
     * The protobuf namespace, observed from generated code rather than from
     * the `.proto` text: `package omnibridge.v1` plus
     * `java_package = "io.github.yurisismotto.omnibridge.proto"` is what puts
     * [Envelope] here. A namespace rename that missed either option would
     * land the class somewhere else and this would not compile.
     */
    @Test
    fun `generated protobuf types live in the omnibridge namespace`() {
        assertEquals(
            "io.github.yurisismotto.omnibridge.proto.Envelope",
            Envelope::class.java.name,
        )
        assertEquals(
            "io.github.yurisismotto.omnibridge.proto.capabilities.BatteryState",
            BatteryState::class.java.name,
        )
    }
}
