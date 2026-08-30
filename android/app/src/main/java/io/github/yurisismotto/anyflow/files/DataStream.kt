package io.github.yurisismotto.anyflow.files

import io.github.yurisismotto.anyflow.identity.DeviceIdentity
import io.github.yurisismotto.anyflow.identity.Fingerprint
import io.github.yurisismotto.anyflow.net.TlsFactory
import java.io.DataInputStream
import java.io.EOFException
import java.io.IOException
import java.io.InputStream
import java.io.OutputStream
import java.net.InetSocketAddress
import java.net.Socket
import java.security.MessageDigest
import javax.net.ssl.SSLSocket

/**
 * The `files.v1` data stream: its two handshake frames, and the copy loop.
 *
 * Nothing here goes through [io.github.yurisismotto.anyflow.net.Framing], the
 * replay guard or the capability dispatch table. That machinery is for a
 * low-rate stream of small control messages, and ADR-0012 records why pushing
 * a multi-gigabyte transfer through it is the wrong shape. What *is* reused is
 * everything that decides trust: TLS 1.3, mutual authentication, the pinned
 * identity, and the desktop's existing port.
 *
 * A transfer costs [COPY_BUFFER_BYTES] of memory and one file descriptor,
 * whatever the file's size.
 */
object DataStream {

    /**
     * What a transfer costs in memory, whatever the file's size. Must match
     * `COPY_BUFFER_BYTES` on the desktop closely enough that neither side is
     * surprised; they are independent buffers, so the values need not be
     * equal, only both small.
     */
    const val COPY_BUFFER_BYTES = 64 * 1024

    /**
     * Cap on the two protobuf frames that open a data stream. Both are tens
     * of bytes. Checked before allocation, exactly as `MAX_FRAME_LEN` is on
     * the control session.
     */
    const val MAX_FRAME_BYTES = 4096

    /** How long a stalled stream is tolerated before it is abandoned. */
    const val IDLE_TIMEOUT_MS = 60_000

    /** How long to wait for the desktop's reply to our authentication. */
    const val HANDSHAKE_TIMEOUT_MS = 15_000

    /**
     * Opens an authenticated data stream to the desktop.
     *
     * The same pinned identity as the control session, the same port, and the
     * same `PinnedTrustManager`. Only the ALPN differs.
     */
    fun open(
        address: InetSocketAddress,
        identity: DeviceIdentity,
        pinned: Fingerprint,
        connectTimeoutMs: Int = 8_000,
    ): SSLSocket {
        val context = TlsFactory.sslContext(identity, pinned)
        val plain = Socket()
        plain.connect(InetSocketAddress(address.hostString, address.port), connectTimeoutMs)
        val socket = context.socketFactory.createSocket(
            plain, address.hostString, address.port, true,
        ) as SSLSocket
        TlsFactory.harden(socket, TlsFactory.ALPN_DATA_PROTOCOL)
        socket.soTimeout = HANDSHAKE_TIMEOUT_MS
        // Forces the handshake now, so a pinning failure surfaces here rather
        // than on the first read.
        socket.startHandshake()
        return socket
    }

    /** Writes one length-prefixed protobuf frame. */
    fun writeFrame(out: OutputStream, body: ByteArray) {
        if (body.size > MAX_FRAME_BYTES) throw IOException("data stream frame too large")
        out.write(
            byteArrayOf(
                (body.size ushr 24).toByte(),
                (body.size ushr 16).toByte(),
                (body.size ushr 8).toByte(),
                body.size.toByte(),
            ),
        )
        out.write(body)
        out.flush()
    }

    /**
     * Reads one length-prefixed protobuf frame.
     *
     * The length is checked before the buffer is allocated. This frame comes
     * from a peer that has completed TLS but proved nothing about which
     * transfer it wants, so it is the least trusted input in the capability.
     */
    fun readFrame(input: InputStream): ByteArray {
        val data = DataInputStream(input)
        // Widened before comparing, so a length with the high bit set reads
        // as a huge positive number and is rejected rather than as a negative
        // one that slips past a `> MAX` check.
        val length = data.readInt().toLong() and 0xFFFFFFFFL
        if (length == 0L) throw IOException("zero-length data stream frame")
        if (length > MAX_FRAME_BYTES) throw IOException("data stream frame exceeds the limit")
        val body = ByteArray(length.toInt())
        data.readFully(body)
        return body
    }

    /** What a copy produced. */
    class CopyResult(val sha256: ByteArray)

    /**
     * Reads exactly [expected] bytes into [sink], hashing as it goes.
     *
     * The caller compares the digest against the offer; this deliberately
     * does not, so that "did the bytes arrive" and "are they the right bytes"
     * stay two separate answers.
     *
     * Enforces truncation (EOF before [expected]) and overrun (anything after
     * it) as failures rather than quietly accepting a file of the wrong
     * length.
     *
     * @param onProgress called with the running byte count.
     * @param isCancelled polled between chunks so a cancel takes effect while
     *   bytes are moving, not at the end of the file.
     */
    fun receiveExactly(
        input: InputStream,
        sink: OutputStream,
        expected: Long,
        onProgress: (Long) -> Unit,
        isCancelled: () -> Boolean,
    ): CopyResult {
        val buffer = ByteArray(COPY_BUFFER_BYTES)
        val digest = MessageDigest.getInstance("SHA-256")
        var remaining = expected

        while (remaining > 0) {
            if (isCancelled()) throw TransferCancelled()

            // Never read more than is still expected, so an overrun cannot
            // reach the file even for an instant.
            val want = minOf(buffer.size.toLong(), remaining).toInt()
            val n = input.read(buffer, 0, want)
            if (n < 0) throw TruncatedTransfer(expected - remaining, expected)

            digest.update(buffer, 0, n)
            sink.write(buffer, 0, n)
            remaining -= n
            onProgress(expected - remaining)
        }
        sink.flush()

        // The sender must be finished. Anything further means it disagreed
        // with its own offer about how large the file is.
        if (input.read() >= 0) throw OversizedTransfer(expected)

        return CopyResult(digest.digest())
    }

    /**
     * Writes exactly [expected] bytes from [source] to [out].
     *
     * Half-closes on success, so the receiver can tell "the file ended" from
     * "the link stalled".
     */
    fun sendExactly(
        source: InputStream,
        out: OutputStream,
        socket: SSLSocket,
        expected: Long,
        onProgress: (Long) -> Unit,
        isCancelled: () -> Boolean,
    ) {
        val buffer = ByteArray(COPY_BUFFER_BYTES)
        var remaining = expected

        while (remaining > 0) {
            if (isCancelled()) throw TransferCancelled()

            val want = minOf(buffer.size.toLong(), remaining).toInt()
            val n = source.read(buffer, 0, want)
            if (n < 0) {
                // The local file is shorter than the offer said. Sending a
                // short stream would make the receiver fail its hash check
                // for a reason it could not diagnose.
                throw TruncatedTransfer(expected - remaining, expected)
            }
            out.write(buffer, 0, n)
            remaining -= n
            onProgress(expected - remaining)
        }
        out.flush()
        // Half-close, so the receiver sees a definite end of file.
        runCatching { socket.shutdownOutput() }
    }

    /** Streams a source to compute its size and digest without buffering it. */
    fun hash(source: InputStream): Pair<Long, ByteArray> {
        val buffer = ByteArray(COPY_BUFFER_BYTES)
        val digest = MessageDigest.getInstance("SHA-256")
        var size = 0L
        while (true) {
            val n = source.read(buffer)
            if (n < 0) break
            digest.update(buffer, 0, n)
            size += n
        }
        return size to digest.digest()
    }

    class TransferCancelled : IOException("the transfer was cancelled")
    class TruncatedTransfer(val got: Long, val expected: Long) :
        IOException("the stream ended after $got of $expected bytes")
    class OversizedTransfer(val expected: Long) :
        IOException("the sender wrote more than the $expected bytes it offered")
    class StreamClosed : EOFException("the data stream closed")
}
