package io.github.yurisismotto.omnibridge.files

import android.content.ContentResolver
import android.content.Context
import android.net.Uri
import android.provider.OpenableColumns
import java.io.IOException
import java.io.InputStream

/**
 * A file the user shared into OmniBridge, read through the `ContentResolver`.
 *
 * ## Why never a real path
 *
 * A `content://` URI is a *capability*, not a location. Resolving one to a
 * filesystem path is the classic Android mistake: the path is often wrong,
 * often unreadable, and on a `FileProvider` URI from another app it can be
 * turned into a traversal (the "content provider path traversal" family of
 * bugs). The resolver is the supported way to read one, it respects the
 * permission grant that came with the intent, and it works for providers that
 * have no file behind them at all — Drive, a scanner, a screenshot.
 *
 * So this class only ever calls [ContentResolver.openInputStream] and
 * [ContentResolver.query]. There is no `getPath` anywhere in `files.v1`.
 *
 * ## Two passes
 *
 * The offer must carry the SHA-256 before the first byte moves, so the stream
 * is read twice: once to hash, once to send. That costs one extra read of the
 * file and keeps memory flat — the alternative, buffering the whole thing to
 * hash it once, is exactly what FILE-13 forbids.
 *
 * A provider whose content changes between the two passes produces a hash
 * mismatch on the receiver, which fails the transfer rather than delivering
 * something that does not match its own digest. That is the safe outcome.
 */
class SharedFile(
    private val context: Context,
    val uri: Uri,
) {

    /** What the provider says this file is called, sanitized. */
    fun displayName(): String? {
        val raw = queryString(OpenableColumns.DISPLAY_NAME)
            ?: uri.lastPathSegment
            ?: return null
        // The name comes from another app, so it is no more trusted than one
        // from the network.
        return Filenames.sanitize(raw)
    }

    /** The provider's declared size, when it has one. */
    fun declaredSize(): Long? =
        runCatching {
            context.contentResolver
                .query(uri, arrayOf(OpenableColumns.SIZE), null, null, null)
                ?.use { cursor ->
                    if (cursor.moveToFirst() && !cursor.isNull(0)) cursor.getLong(0) else null
                }
        }.getOrNull()

    fun mimeType(): String = context.contentResolver.getType(uri).orEmpty()

    fun openStream(): InputStream =
        context.contentResolver.openInputStream(uri)
            ?: throw IOException("the app that shared this file would not open it")

    /**
     * Streams the file once to learn its true size and digest.
     *
     * The provider's declared size is a hint and is not used: it can be
     * absent, stale, or simply wrong, and the offer's `size_bytes` has to be
     * exact or the receiver will reject the transfer.
     */
    fun measure(): Pair<Long, ByteArray> = openStream().use { DataStream.hash(it) }

    private fun queryString(column: String): String? =
        runCatching {
            context.contentResolver.query(uri, arrayOf(column), null, null, null)?.use { cursor ->
                if (cursor.moveToFirst() && !cursor.isNull(0)) cursor.getString(0) else null
            }
        }.getOrNull()
}
