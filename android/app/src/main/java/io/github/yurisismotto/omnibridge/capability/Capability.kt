package io.github.yurisismotto.omnibridge.capability

import com.google.protobuf.ByteString
import io.github.yurisismotto.omnibridge.identity.Fingerprint
import io.github.yurisismotto.omnibridge.proto.ErrorCode

/**
 * A feature of the protocol.
 *
 * The transport never mentions a capability by name: it routes on
 * [id] to whatever is in the [CapabilityRegistry]. Adding clipboard or file
 * transfer later means adding a class here, not touching the connection code.
 * Mirrors `omnibridge_core::capability`.
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

    /**
     * The peer answered one of **our** frames with a transport-level error.
     *
     * [correlationId] is `Envelope.correlation_id`, which a refusing peer sets
     * to the `message_id` of the frame it is refusing — the id
     * [CapabilityContext.send] handed back when that frame went out. A
     * capability that kept its ids can therefore attribute the refusal to the
     * exact operation that caused it; one that did not simply ignores this.
     *
     * ## Why every capability is told, rather than one being routed to
     *
     * The transport cannot know which capability a `message_id` belongs to:
     * the error names an envelope, and the envelope's capability id is not
     * echoed back. So this is a fan-out, and the safety of it rests on the id
     * being 16 bytes of randomness minted per frame — a capability that did
     * not mint this one cannot match it.
     *
     * Default no-op: a capability that never correlates anything is unaffected.
     */
    suspend fun onPeerError(
        context: CapabilityContext,
        correlationId: ByteString,
        code: ErrorCode,
    ) {
    }

    suspend fun onPeerDisconnected(peer: Fingerprint) {}
}

/** What a capability is given when it runs. */
class CapabilityContext(
    val peer: Fingerprint,
    val peerDeviceId: String,
    private val sender: suspend (String, ByteString) -> ByteString,
) {
    /**
     * Sends one capability frame and answers the `message_id` it went out
     * under.
     *
     * Returning the id is what makes a peer's refusal attributable. Before
     * this, `ERROR_CODE_UNSUPPORTED_CAPABILITY` could only be logged: the id
     * it correlated to had already been discarded inside the transport, so a
     * clipboard the desktop had dropped was reported to the person as sent
     * (GitHub #8).
     */
    suspend fun send(capabilityId: String, payload: ByteString): ByteString =
        sender(capabilityId, payload)
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
