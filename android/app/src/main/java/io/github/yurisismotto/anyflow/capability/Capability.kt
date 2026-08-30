package io.github.yurisismotto.anyflow.capability

import com.google.protobuf.ByteString
import io.github.yurisismotto.anyflow.identity.Fingerprint

/**
 * A feature of the protocol.
 *
 * The transport never mentions a capability by name: it routes on
 * [id] to whatever is in the [CapabilityRegistry]. Adding clipboard or file
 * transfer later means adding a class here, not touching the connection code.
 * Mirrors `anyflow_core::capability`.
 */
interface Capability {
    /** Versioned identifier, e.g. `"battery.v1"`. */
    val id: String

    /** Called once the session is established and this capability is agreed. */
    suspend fun onPeerConnected(context: CapabilityContext) {}

    /**
     * Handles an inbound message.
     *
     * [payload] is untrusted, attacker-controlled bytes. Validate before use.
     * Throwing is logged and does not tear down the connection: one bad
     * capability must not cost the user everything else.
     */
    suspend fun onMessage(context: CapabilityContext, payload: ByteString)

    suspend fun onPeerDisconnected(peer: Fingerprint) {}
}

/** What a capability is given when it runs. */
class CapabilityContext(
    val peer: Fingerprint,
    val peerDeviceId: String,
    private val sender: suspend (String, ByteString) -> Unit,
) {
    suspend fun send(capabilityId: String, payload: ByteString) = sender(capabilityId, payload)
}

/** The set of capabilities this device implements. */
class CapabilityRegistry(capabilities: List<Capability>) {

    private val byId: Map<String, Capability> =
        capabilities.associateBy { it.id }.toSortedMap()

    /** Sorted, so HELLO is deterministic. */
    fun advertised(): List<String> = byId.keys.toList()

    operator fun get(id: String): Capability? = byId[id]

    fun supports(id: String): Boolean = byId.containsKey(id)

    /** Capabilities both sides support. */
    fun negotiate(peerAdvertised: List<String>): List<String> =
        peerAdvertised.filter(::supports).distinct().sorted()
}
