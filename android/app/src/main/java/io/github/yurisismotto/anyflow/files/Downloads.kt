package io.github.yurisismotto.anyflow.files

import android.content.ContentResolver
import android.content.ContentValues
import android.content.Context
import android.net.Uri
import android.os.Environment
import android.provider.MediaStore
import android.util.Log
import java.io.IOException
import java.io.OutputStream

/**
 * Where a received file goes on Android.
 *
 * ## Why MediaStore and not a path
 *
 * Under scoped storage an app has no business writing to an arbitrary path in
 * shared storage, and asking for `MANAGE_EXTERNAL_STORAGE` to get one would
 * be wildly disproportionate for "receive a photo". `MediaStore.Downloads`
 * with a `RELATIVE_PATH` of `Download/AnyFlow` needs **no storage permission
 * at all** on API 29+, which is this app's floor. The user finds the file in
 * Files under Downloads/AnyFlow, and it survives uninstalling the app.
 *
 * ## Why IS_PENDING is the whole integrity story
 *
 * A pending MediaStore item is invisible to every other app. So the flow maps
 * exactly onto the desktop's temp-file-then-verify-then-promote model:
 *
 * 1. insert with `IS_PENDING = 1` and stream the bytes in;
 * 2. hash while writing;
 * 3. compare against the offer;
 * 4. **only then** clear `IS_PENDING`, which publishes it.
 *
 * A file that fails its hash is deleted at step 3 and was never visible under
 * any name. There is no window in which unverified bytes are presented as a
 * finished download.
 *
 * ## Duplicate names
 *
 * MediaStore does this itself: inserting a second `photo.jpg` yields
 * `photo (1).jpg`. It never overwrites, which is the property FILE-10 asks
 * for, and it is the same convention the desktop implements by hand and that
 * every browser uses — so it needs no explaining to a user.
 */
class Downloads(private val context: Context) {

    private val resolver: ContentResolver get() = context.contentResolver

    /** The subdirectory of Downloads that received files land in. */
    private val relativePath = "${Environment.DIRECTORY_DOWNLOADS}/AnyFlow"

    /** A file being written, before it is visible to anything else. */
    class Pending(
        val uri: Uri,
        val stream: OutputStream,
    )

    /**
     * Creates an invisible, pending entry and opens it for writing.
     *
     * @param filename must already be sanitized: this is not the place that
     *   check belongs, and passing a raw peer string here would be a bug.
     */
    fun beginWrite(filename: String, mimeType: String): Pending {
        val values = ContentValues().apply {
            put(MediaStore.Downloads.DISPLAY_NAME, filename)
            if (mimeType.isNotEmpty()) {
                put(MediaStore.Downloads.MIME_TYPE, mimeType)
            }
            put(MediaStore.Downloads.RELATIVE_PATH, relativePath)
            // Invisible to other apps until we publish it.
            put(MediaStore.Downloads.IS_PENDING, 1)
        }

        val uri = resolver.insert(MediaStore.Downloads.EXTERNAL_CONTENT_URI, values)
            ?: throw IOException("MediaStore refused to create the download entry")

        val stream = try {
            resolver.openOutputStream(uri, "w")
                ?: throw IOException("MediaStore returned no output stream")
        } catch (e: Exception) {
            // Leaving a pending row behind would be an invisible file nobody
            // can reach or delete.
            runCatching { resolver.delete(uri, null, null) }
            throw IOException("could not open the download for writing", e)
        }

        return Pending(uri, stream)
    }

    /**
     * Publishes a verified file. After this it is visible in Files, and only
     * after this.
     */
    fun publish(pending: Pending) {
        val values = ContentValues().apply { put(MediaStore.Downloads.IS_PENDING, 0) }
        resolver.update(pending.uri, values, null, null)
    }

    /**
     * Discards a file that must never be seen — a failed hash, a cancelled
     * transfer, a dropped connection.
     */
    fun discard(pending: Pending) {
        runCatching { pending.stream.close() }
        val deleted = runCatching { resolver.delete(pending.uri, null, null) }.getOrNull()
        if (deleted == null || deleted == 0) {
            // Worth a log: an undeleted pending row is an invisible file that
            // the user cannot find or remove.
            Log.w(TAG, "could not delete a discarded download entry")
        }
    }

    /**
     * The name the file actually ended up with.
     *
     * Not necessarily the name that was asked for: MediaStore appends
     * ` (1)`, ` (2)` and so on rather than overwriting. Read back so the UI
     * shows what is really on disk instead of what was requested.
     */
    fun displayName(uri: Uri): String? =
        runCatching {
            resolver.query(uri, arrayOf(MediaStore.Downloads.DISPLAY_NAME), null, null, null)
                ?.use { cursor ->
                    if (cursor.moveToFirst()) cursor.getString(0) else null
                }
        }.getOrNull()

    private companion object {
        const val TAG = "Downloads"
    }
}
