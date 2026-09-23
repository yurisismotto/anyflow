package io.github.yurisismotto.omnibridge

import com.google.protobuf.ByteString
import io.github.yurisismotto.omnibridge.net.Framing
import io.github.yurisismotto.omnibridge.proto.Envelope
import io.github.yurisismotto.omnibridge.proto.Ping
import java.io.ByteArrayInputStream
import java.io.ByteArrayOutputStream
import java.io.EOFException
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test

/** Must stay in lockstep with `omnibridge_core::framing`. */
class FramingTest {

    private fun header(length: Long) = byteArrayOf(
        (length ushr 24).toByte(),
        (length ushr 16).toByte(),
        (length ushr 8).toByte(),
        length.toByte(),
    )

    private fun read(bytes: ByteArray) = Framing.read(ByteArrayInputStream(bytes))

    @Test
    fun `round trips an envelope`() {
        val envelope = Envelope.newBuilder()
            .setProtocolVersion(1)
            .setMessageId(ByteString.copyFrom(ByteArray(16)))
            .setSequence(1)
            .setPing(Ping.getDefaultInstance())
            .build()

        val out = ByteArrayOutputStream()
        Framing.write(out, envelope)
        assertEquals(envelope, read(out.toByteArray()))
    }

    @Test
    fun `refuses a length prefix over MAX_FRAME_LEN before allocating`() {
        // Only the 4-byte header is supplied. If the implementation tried to
        // allocate or read the body first, this would hang or OOM instead of
        // throwing.
        val error = assertThrows(Framing.FrameTooLarge::class.java) {
            read(header(Framing.MAX_FRAME_LEN + 1L))
        }
        assertEquals(Framing.MAX_FRAME_LEN + 1L, error.length)
    }

    @Test
    fun `a length with the high bit set is huge, not negative`() {
        // readInt() is signed. Treating 0xFFFFFFFF as -1 would slip past a
        // naive `> MAX` check and then be used as an allocation size.
        val error = assertThrows(Framing.FrameTooLarge::class.java) {
            read(header(0xFFFFFFFFL))
        }
        assertEquals(0xFFFFFFFFL, error.length)
        assertTrue(error.length > 0)
    }

    @Test
    fun `refuses a zero-length frame`() {
        val error = assertThrows(Framing.ProtocolException::class.java) { read(header(0)) }
        assertTrue(error.message!!.contains("zero-length"))
    }

    @Test
    fun `accepts a frame exactly at the limit`() {
        // The boundary must be inclusive on both sides, or the two
        // implementations disagree about one byte.
        assertThrows(EOFException::class.java) {
            read(header(Framing.MAX_FRAME_LEN.toLong()))
        }
    }

    @Test
    fun `a truncated body reports end of stream`() {
        val body = ByteArray(10)
        assertThrows(EOFException::class.java) { read(header(100) + body) }
    }

    @Test
    fun `an empty stream reports end of stream`() {
        assertThrows(EOFException::class.java) { read(ByteArray(0)) }
    }

    @Test
    fun `refuses a malformed protobuf body`() {
        val garbage = byteArrayOf(-1, -1, -1, -1, 0x7F, 0x7F, 0x7F)
        assertThrows(com.google.protobuf.InvalidProtocolBufferException::class.java) {
            read(header(garbage.size.toLong()) + garbage)
        }
    }

    @Test
    fun `MAX_FRAME_LEN matches the desktop`() {
        assertEquals(64 * 1024, Framing.MAX_FRAME_LEN)
    }
}
