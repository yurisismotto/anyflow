package io.github.yurisismotto.omnibridge.capability

import com.google.protobuf.ByteString
import io.github.yurisismotto.omnibridge.files.FileTransferManager
import io.github.yurisismotto.omnibridge.identity.Fingerprint

/**
 * `files.v1` — secure file transfer with the paired computer.
 *
 * Thin on purpose: it decodes nothing and decides nothing. It hands the
 * payload to [FileTransferManager] along with the identity the transport
 * established, so all the policy lives in one place.
 *
 * Note that no file byte ever passes through here. Control messages travel on
 * the session; the bytes move on a separate authenticated TLS stream, which
 * is what lets `MAX_FRAME_LEN` stay at 64 KiB and lets a cancel or an unpair
 * take effect while a multi-gigabyte copy is running. See ADR-0012.
 */
class FilesCapability(
    private val manager: FileTransferManager,
) : Capability {

    override val id: String = ID

    override suspend fun onPeerConnected(context: CapabilityContext) {
        // Recorded so a transfer can be started from the Sharesheet rather
        // than only in reply to an inbound message.
        manager.attachControl { payload -> context.send(ID, payload) }
    }

    override suspend fun onMessage(context: CapabilityContext, payload: ByteString) {
        manager.handleControl(context.peer, payload)
    }

    override suspend fun onPeerDisconnected(peer: Fingerprint) {
        // Every live transfer fails here. The data stream is a *separate* TCP
        // connection and would otherwise keep running after the control link
        // died, or leave a transfer in `transferring` forever.
        manager.onSessionEnded()
    }

    companion object {
        const val ID = "files.v1"
    }
}
