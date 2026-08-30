package dev.fedroid.bridge.net

import dev.fedroid.bridge.proto.Envelope
import java.io.DataInputStream
import java.io.EOFException
import java.io.InputStream
import java.io.OutputStream

/**
 * Length-prefixed protobuf framing: a 4-byte big-endian length, then the
 * encoded [Envelope].
 *
 * The length is validated before any buffer is allocated, so a peer claiming
 * a 4 GiB frame costs us nothing. Mirrors `fedroid_core::framing`.
 */
object Framing {

    /** Must stay identical to `fedroid_core::framing::MAX_FRAME_LEN`. */
    const val MAX_FRAME_LEN = 64 * 1024

    class FrameTooLarge(val length: Long) :
        ProtocolException("frame of $length bytes exceeds the $MAX_FRAME_LEN byte limit")

    open class ProtocolException(message: String) : java.io.IOException(message)

    fun write(out: OutputStream, envelope: Envelope) {
        val body = envelope.toByteArray()
        if (body.size > MAX_FRAME_LEN) throw FrameTooLarge(body.size.toLong())
        val header = ByteArray(4)
        header[0] = (body.size ushr 24).toByte()
        header[1] = (body.size ushr 16).toByte()
        header[2] = (body.size ushr 8).toByte()
        header[3] = body.size.toByte()
        out.write(header)
        out.write(body)
        out.flush()
    }

    /** @throws EOFException when the peer closed the connection cleanly. */
    fun read(input: InputStream): Envelope {
        val data = DataInputStream(input)
        // readInt() is signed; widen before comparing so a length with the
        // high bit set reads as a huge positive number and is rejected,
        // rather than as a negative one that slips past a `> MAX` check.
        val length = data.readInt().toLong() and 0xFFFFFFFFL
        if (length > MAX_FRAME_LEN) throw FrameTooLarge(length)
        if (length == 0L) throw ProtocolException("zero-length frame")

        val body = ByteArray(length.toInt())
        data.readFully(body)
        return Envelope.parseFrom(body)
    }
}
