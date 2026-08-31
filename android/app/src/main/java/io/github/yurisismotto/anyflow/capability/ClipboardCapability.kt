package io.github.yurisismotto.anyflow.capability

import com.google.protobuf.ByteString
import io.github.yurisismotto.anyflow.clipboard.ClipboardSync
import io.github.yurisismotto.anyflow.identity.Fingerprint

/**
 * `clipboard.v1` — text clipboard sharing with the paired computer.
 *
 * Thin on purpose, exactly as [FilesCapability] is: it decodes nothing and
 * decides nothing beyond routing, so every policy question has one answer in
 * one place ([ClipboardSync]).
 *
 * Everything here travels on the existing control session. There is no second
 * socket, no second listener and no data stream: clipboard text is
 * control-plane data, bounded well below the 64 KiB frame limit.
 */
class ClipboardCapability(
    private val sync: ClipboardSync,
    /** How the peer is named in a notification. Never its own claim. */
    private val peerName: (Fingerprint) -> String,
) : Capability {

    override val id: String = ID

    override suspend fun onPeerConnected(context: CapabilityContext) {
        // Recorded so a send can be started from the UI rather than only in
        // reply to an inbound message.
        sync.attachSession(context.peer) { payload -> context.send(ID, payload) }
    }

    override suspend fun onMessage(context: CapabilityContext, payload: ByteString) {
        val reply = sync.handleControl(
            peer = context.peer,
            peerDeviceId = context.peerDeviceId,
            peerName = peerName(context.peer),
            payload = payload,
        )
        if (reply != null) {
            context.send(ID, reply)
        }
    }

    override suspend fun onPeerDisconnected(peer: Fingerprint) {
        sync.detachSession(peer)
    }

    companion object {
        const val ID = "clipboard.v1"
    }
}
