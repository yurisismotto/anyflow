package io.github.yurisismotto.anyflow

import io.github.yurisismotto.anyflow.files.DataStream
import java.io.ByteArrayInputStream
import java.io.ByteArrayOutputStream
import java.security.MessageDigest
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The copy loop, and the two ways a peer can disagree with its own offer.
 *
 * Pure stream work, so it runs on the JVM with no device: what is being
 * tested is arithmetic and control flow, and a real socket would add nothing
 * but flakiness. The socket-level behaviour is covered by the Rust
 * integration suite and on hardware.
 */
class DataStreamCopyTest {

    private fun sample(size: Int): ByteArray {
        var x = 0x9e3779b9.toInt()
        return ByteArray(size) {
            x = x * 1664525 + 1013904223
            (x ushr 24).toByte()
        }
    }

    private fun sha256(bytes: ByteArray): ByteArray =
        MessageDigest.getInstance("SHA-256").digest(bytes)

    @Test
    fun `an exact transfer hashes what it wrote`() {
        val payload = sample(200_000)
        val sink = ByteArrayOutputStream()
        var lastProgress = 0L

        val result = DataStream.receiveExactly(
            input = ByteArrayInputStream(payload),
            sink = sink,
            expected = payload.size.toLong(),
            onProgress = { lastProgress = it },
            isCancelled = { false },
        )

        assertArrayEquals(payload, sink.toByteArray())
        assertArrayEquals(sha256(payload), result.sha256)
        assertEquals(payload.size.toLong(), lastProgress)
    }

    @Test
    fun `a zero byte file is a valid transfer`() {
        val sink = ByteArrayOutputStream()
        val result = DataStream.receiveExactly(
            input = ByteArrayInputStream(ByteArray(0)),
            sink = sink,
            expected = 0,
            onProgress = {},
            isCancelled = { false },
        )
        assertArrayEquals(sha256(ByteArray(0)), result.sha256)
        assertEquals(0, sink.size())
    }

    @Test(expected = DataStream.TruncatedTransfer::class)
    fun `a truncated stream fails rather than producing a short file`() {
        DataStream.receiveExactly(
            input = ByteArrayInputStream(ByteArray(10)),
            sink = ByteArrayOutputStream(),
            expected = 100,
            onProgress = {},
            isCancelled = { false },
        )
    }

    @Test
    fun `a stream longer than its offer is refused and the overflow never lands`() {
        val sink = ByteArrayOutputStream()
        var threw = false
        try {
            DataStream.receiveExactly(
                input = ByteArrayInputStream(ByteArray(100)),
                sink = sink,
                expected = 10,
                onProgress = {},
                isCancelled = { false },
            )
        } catch (e: DataStream.OversizedTransfer) {
            threw = true
        }
        assertTrue("an oversized stream must be refused", threw)
        // Only the declared number of bytes was ever written out.
        assertEquals(10, sink.size())
    }

    @Test(expected = DataStream.TransferCancelled::class)
    fun `cancelling stops the copy`() {
        DataStream.receiveExactly(
            input = ByteArrayInputStream(ByteArray(1_000_000)),
            sink = ByteArrayOutputStream(),
            expected = 1_000_000,
            onProgress = {},
            isCancelled = { true },
        )
    }

    @Test
    fun `hashing a stream matches hashing its bytes`() {
        val payload = sample(150_000)
        val (size, digest) = DataStream.hash(ByteArrayInputStream(payload))
        assertEquals(payload.size.toLong(), size)
        assertArrayEquals(sha256(payload), digest)
    }

    @Test
    fun `the copy buffer is small enough that a transfer does not scale with the file`() {
        // FILE-13 is a property of this constant plus the loop above: a
        // transfer costs one buffer, whatever the file's size.
        assertTrue(DataStream.COPY_BUFFER_BYTES <= 64 * 1024)
    }

    @Test
    fun `a frame round trips and an oversized one is refused before allocating`() {
        val body = ByteArray(64) { it.toByte() }
        val buffer = ByteArrayOutputStream()
        DataStream.writeFrame(buffer, body)
        assertArrayEquals(body, DataStream.readFrame(ByteArrayInputStream(buffer.toByteArray())))

        // A claim of 4 GiB must cost a rejection, not four gigabytes.
        val huge = byteArrayOf(-1, -1, -1, -1)
        var refused = false
        try {
            DataStream.readFrame(ByteArrayInputStream(huge))
        } catch (e: java.io.IOException) {
            refused = true
        }
        assertTrue(refused)
    }
}
