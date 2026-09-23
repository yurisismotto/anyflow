package io.github.yurisismotto.omnibridge

import com.google.protobuf.ByteString
import io.github.yurisismotto.omnibridge.capability.Capability
import io.github.yurisismotto.omnibridge.capability.CapabilityContext
import io.github.yurisismotto.omnibridge.capability.CapabilityRegistry
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Capability authorization is an allow-list intersection, never a claim the
 * peer makes about itself. Mirrors `omnibridge_core::capability`.
 */
class CapabilityRegistryTest {

    private class Fake(override val id: String) : Capability {
        override suspend fun onMessage(context: CapabilityContext, payload: ByteString) = Unit
    }

    private fun registry(vararg ids: String) = CapabilityRegistry(ids.map { Fake(it) })

    @Test
    fun `advertises a sorted, deterministic list`() {
        // HELLO must be byte-identical across runs, or two peers can disagree
        // about what was offered.
        assertEquals(
            listOf("battery.v1", "clipboard.v1", "ping.v1"),
            registry("ping.v1", "battery.v1", "clipboard.v1").advertised(),
        )
    }

    @Test
    fun `negotiates only what both sides implement`() {
        val local = registry("battery.v1", "ping.v1")
        assertEquals(
            listOf("battery.v1"),
            local.negotiate(listOf("battery.v1", "clipboard.v1")),
        )
    }

    @Test
    fun `a capability we do not implement is never negotiated`() {
        val local = registry("battery.v1")
        // It does not matter what the peer advertises.
        assertEquals(emptyList<String>(), local.negotiate(listOf("filesystem.v1")))
        assertFalse(local.supports("filesystem.v1"))
        // And it cannot be dispatched to, because there is nothing to dispatch
        // to: this is what stops an un-negotiated capability message.
        assertNull(local["filesystem.v1"])
    }

    @Test
    fun `an un-negotiated capability is not in the session allow-list`() {
        // Reproduces the per-message check in PeerConnection: the id must be
        // in the negotiated list, not merely implemented locally.
        val local = registry("battery.v1", "ping.v1")
        val negotiated = local.negotiate(listOf("battery.v1"))

        assertTrue("battery.v1" in negotiated)
        // ping.v1 is implemented here but was never agreed for this session.
        assertTrue(local.supports("ping.v1"))
        assertFalse("ping.v1" in negotiated)
    }

    @Test
    fun `a peer cannot inflate the negotiated set by repeating itself`() {
        val local = registry("battery.v1")
        assertEquals(
            listOf("battery.v1"),
            local.negotiate(listOf("battery.v1", "battery.v1", "battery.v1")),
        )
    }

    @Test
    fun `negotiation result is sorted`() {
        val local = registry("a.v1", "b.v1", "c.v1")
        assertEquals(
            listOf("a.v1", "b.v1", "c.v1"),
            local.negotiate(listOf("c.v1", "a.v1", "b.v1")),
        )
    }

    @Test
    fun `an empty peer list negotiates nothing`() {
        assertEquals(emptyList<String>(), registry("battery.v1").negotiate(emptyList()))
    }
}
