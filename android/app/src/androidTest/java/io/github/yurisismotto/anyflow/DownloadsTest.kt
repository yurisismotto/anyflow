package io.github.yurisismotto.anyflow

import android.provider.MediaStore
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.github.yurisismotto.anyflow.files.Downloads
import java.security.MessageDigest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

/**
 * [Downloads] against the real MediaStore.
 *
 * These cannot be unit tests. The properties under test — that a pending
 * entry is invisible, that a duplicate name is renamed rather than
 * overwritten, that a discarded entry leaves nothing behind — are behaviours
 * of the platform's provider, not of this code. A mock would assert that the
 * mock behaves as assumed, which is worth nothing: the whole integrity story
 * on Android rests on `IS_PENDING` really working the way the documentation
 * says.
 *
 * Requires a device or emulator (API 29+):
 *
 * ```bash
 * ./gradlew :app:connectedDebugAndroidTest
 * ```
 */
@RunWith(AndroidJUnit4::class)
class DownloadsTest {

    private val context = InstrumentationRegistry.getInstrumentation().targetContext
    private val downloads = Downloads(context)
    private val resolver = context.contentResolver

    private fun sha256(bytes: ByteArray) = MessageDigest.getInstance("SHA-256").digest(bytes)

    private fun uniqueName(prefix: String) =
        "$prefix-${System.nanoTime()}.bin"

    @Test
    fun a_written_and_published_file_is_readable_with_the_bytes_that_went_in() {
        val name = uniqueName("anyflow-test")
        val payload = ByteArray(120_000) { (it * 31).toByte() }

        val pending = downloads.beginWrite(name, "application/octet-stream")
        try {
            pending.stream.use { it.write(payload) }
            downloads.publish(pending)

            val readBack = resolver.openInputStream(pending.uri)!!.use { it.readBytes() }
            assertEquals(payload.size, readBack.size)
            assertTrue(sha256(payload).contentEquals(sha256(readBack)))
            assertEquals(name, downloads.displayName(pending.uri))
        } finally {
            resolver.delete(pending.uri, null, null)
        }
    }

    @Test
    fun a_pending_entry_is_not_visible_to_a_query_until_it_is_published() {
        val name = uniqueName("anyflow-pending")
        val pending = downloads.beginWrite(name, "application/octet-stream")
        try {
            pending.stream.use { it.write(ByteArray(10)) }

            // The integrity property: unverified bytes are not presented as a
            // finished download at any instant.
            assertNull(
                "a pending entry must not appear in a normal query",
                findByDisplayName(name),
            )

            downloads.publish(pending)
            assertNotNull(
                "publishing is what makes it visible",
                findByDisplayName(name),
            )
        } finally {
            resolver.delete(pending.uri, null, null)
        }
    }

    @Test
    fun a_discarded_entry_leaves_nothing_behind() {
        val name = uniqueName("anyflow-discard")
        val pending = downloads.beginWrite(name, "application/octet-stream")
        pending.stream.use { it.write(ByteArray(4096)) }

        // What happens on a hash mismatch, a cancel, or a dropped connection.
        downloads.discard(pending)

        assertNull(
            "a discarded transfer must not leave a visible file",
            findByDisplayName(name),
        )
        // And not an invisible one either: a pending row nobody can find is a
        // file the user cannot delete.
        assertNull(
            "a discarded transfer must not leave a pending row",
            findByDisplayName(name, includePending = true),
        )
    }

    @Test
    fun a_second_file_with_the_same_name_is_renamed_not_overwritten() {
        // FILE-10 on Android: MediaStore does this itself, and the whole
        // no-overwrite guarantee depends on that being true.
        val name = uniqueName("anyflow-dup")
        val first = ByteArray(1000) { 1 }
        val second = ByteArray(2000) { 2 }

        val a = downloads.beginWrite(name, "application/octet-stream")
        val b = downloads.beginWrite(name, "application/octet-stream")
        try {
            a.stream.use { it.write(first) }
            b.stream.use { it.write(second) }
            downloads.publish(a)
            downloads.publish(b)

            val nameA = downloads.displayName(a.uri)
            val nameB = downloads.displayName(b.uri)
            assertNotNull(nameA)
            assertNotNull(nameB)
            assertTrue(
                "the second file must get a different name, got $nameA and $nameB",
                nameA != nameB,
            )

            // Each kept its own bytes: nothing clobbered anything.
            val readA = resolver.openInputStream(a.uri)!!.use { it.readBytes() }
            val readB = resolver.openInputStream(b.uri)!!.use { it.readBytes() }
            assertEquals(first.size, readA.size)
            assertEquals(second.size, readB.size)
        } finally {
            resolver.delete(a.uri, null, null)
            resolver.delete(b.uri, null, null)
        }
    }

    @Test
    fun writing_a_large_file_does_not_require_holding_it_in_memory() {
        // Streamed in the same bounded buffer the data stream uses. The point
        // is that this completes at all on a device with a modest heap.
        val name = uniqueName("anyflow-large")
        val chunk = ByteArray(64 * 1024) { (it and 0xff).toByte() }
        val chunks = 200 // ~13 MiB

        val pending = downloads.beginWrite(name, "application/octet-stream")
        try {
            val digest = MessageDigest.getInstance("SHA-256")
            pending.stream.use { out ->
                repeat(chunks) {
                    out.write(chunk)
                    digest.update(chunk)
                }
            }
            downloads.publish(pending)

            val readDigest = MessageDigest.getInstance("SHA-256")
            var total = 0L
            resolver.openInputStream(pending.uri)!!.use { input ->
                val buffer = ByteArray(64 * 1024)
                while (true) {
                    val n = input.read(buffer)
                    if (n < 0) break
                    readDigest.update(buffer, 0, n)
                    total += n
                }
            }
            assertEquals(chunk.size.toLong() * chunks, total)
            assertTrue(digest.digest().contentEquals(readDigest.digest()))
        } finally {
            resolver.delete(pending.uri, null, null)
        }
    }

    /**
     * @return the row id, or null when no such download exists.
     *
     * `includePending` uses the query-argument form rather than the
     * deprecated `setIncludePending(Uri)`, so the test asks the platform the
     * same way current app code would.
     */
    private fun findByDisplayName(name: String, includePending: Boolean = false): Long? {
        val args = android.os.Bundle().apply {
            putString(
                android.content.ContentResolver.QUERY_ARG_SQL_SELECTION,
                "${MediaStore.Downloads.DISPLAY_NAME} = ?",
            )
            putStringArray(
                android.content.ContentResolver.QUERY_ARG_SQL_SELECTION_ARGS,
                arrayOf(name),
            )
            if (includePending) {
                putInt(MediaStore.QUERY_ARG_MATCH_PENDING, MediaStore.MATCH_INCLUDE)
            }
        }
        resolver.query(
            MediaStore.Downloads.EXTERNAL_CONTENT_URI,
            arrayOf(MediaStore.Downloads._ID),
            args,
            null,
        )?.use { cursor ->
            if (cursor.moveToFirst()) return cursor.getLong(0)
        }
        return null
    }
}
