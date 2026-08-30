package io.github.yurisismotto.anyflow

import com.google.protobuf.ByteString
import io.github.yurisismotto.anyflow.net.Protocol
import io.github.yurisismotto.anyflow.net.ReplayGuard
import io.github.yurisismotto.anyflow.proto.Envelope
import java.security.SecureRandom
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Test

/** Must stay in lockstep with `anyflow_core::session`. */
class ProtocolTest {

    private val random = SecureRandom()

    private fun messageId(): ByteString {
        val bytes = ByteArray(Protocol.MESSAGE_ID_LENGTH)
        random.nextBytes(bytes)
        return ByteString.copyFrom(bytes)
    }

    private fun envelope(
        sequence: Long,
        id: ByteString = messageId(),
    ): Envelope = Envelope.newBuilder()
        .setProtocolVersion(1)
        .setMessageId(id)
        .setSequence(sequence)
        .build()

    // -- version negotiation -------------------------------------------------

    @Test
    fun `negotiates the only supported version`() {
        assertEquals(1, Protocol.negotiateVersion(1, 1))
    }

    @Test
    fun `negotiates the highest version both sides support`() {
        assertEquals(1, Protocol.negotiateVersion(1, 9))
    }

    @Test
    fun `refuses a peer that is entirely newer than us`() {
        assertNull(Protocol.negotiateVersion(2, 9))
    }

    @Test
    fun `refuses a peer that is entirely older than us`() {
        assertNull(Protocol.negotiateVersion(0, 0))
    }

    // -- replay / duplicate protection --------------------------------------

    @Test
    fun `admits a strictly increasing sequence`() {
        val guard = ReplayGuard()
        assertNull(guard.admit(envelope(1)))
        assertNull(guard.admit(envelope(2)))
        assertNull(guard.admit(envelope(3)))
    }

    @Test
    fun `refuses a message id of the wrong length`() {
        val guard = ReplayGuard()
        val short = ByteString.copyFrom(ByteArray(Protocol.MESSAGE_ID_LENGTH - 1))
        assertNotNull(guard.admit(envelope(1, short)))
    }

    @Test
    fun `refuses a repeated sequence number`() {
        val guard = ReplayGuard()
        assertNull(guard.admit(envelope(1)))
        assertNotNull(guard.admit(envelope(1)))
    }

    @Test
    fun `refuses a sequence number that goes backwards`() {
        val guard = ReplayGuard()
        assertNull(guard.admit(envelope(5)))
        assertNotNull(guard.admit(envelope(4)))
    }

    @Test
    fun `refuses a duplicated message id`() {
        val guard = ReplayGuard()
        val id = messageId()
        assertNull(guard.admit(envelope(1, id)))
        assertNotNull(guard.admit(envelope(2, id)))
    }

    @Test
    fun `version bounds match the desktop`() {
        assertEquals(1, Protocol.VERSION_MIN)
        assertEquals(1, Protocol.VERSION_MAX)
        assertEquals(16, Protocol.MESSAGE_ID_LENGTH)
        assertEquals(32, Protocol.NONCE_LENGTH)
    }
}
