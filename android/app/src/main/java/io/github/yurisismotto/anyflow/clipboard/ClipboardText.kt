package io.github.yurisismotto.anyflow.clipboard

import java.security.MessageDigest

/**
 * What counts as clipboard text, and what it hashes to.
 *
 * This is the Kotlin twin of `desktop/capabilities/clipboard/src/text.rs` and
 * the two must agree exactly. A clip that one platform sends and the other
 * refuses is a bug, not a policy — so every rule below has a matching rule
 * there, and `ClipboardTextTest` pins the same vectors the Rust suite pins.
 */
sealed interface TextRejection {
    /** Nothing to share. */
    data object Empty : TextRejection

    /** Past [ClipboardLimits.MAX_TEXT_BYTES]. Never truncated. */
    data class TooLarge(val bytes: Int) : TextRejection

    /** Contains U+0000. See [ClipboardText.validate]. */
    data object ContainsNul : TextRejection

    /** A short, peer-safe description. Contains no clipboard content. */
    fun describe(): String = when (this) {
        Empty -> "the clipboard is empty"
        is TooLarge ->
            "clipboard text is $bytes bytes; the limit is " +
                "${ClipboardLimits.MAX_TEXT_BYTES} bytes. Share it as a file instead."
        ContainsNul -> "the clipboard contains a NUL character"
    }
}

/**
 * Validated clipboard text, with its content hash.
 *
 * Constructing one is the only way to get text into an outbound update or
 * onto the system clipboard, so every path is validated by construction.
 *
 * Note that this is deliberately **not** a `data class`. A generated
 * `toString()` would put clipboard content into every log line, every
 * exception message and every test failure that touched one — which is
 * precisely the leak the logging audit exists to prevent.
 */
class ClipboardText private constructor(
    val text: String,
    val hash: ByteArray,
) {
    /** Size in UTF-8 bytes. Safe to log. */
    val byteLength: Int get() = text.toByteArray(Charsets.UTF_8).size

    /** Redacted on purpose: size and hash prefix, never the text. */
    override fun toString(): String =
        "ClipboardText(bytes=$byteLength, sha256=${Redact.hashPrefix(hash)})"

    override fun equals(other: Any?): Boolean =
        other is ClipboardText && hash.contentEquals(other.hash) && text == other.text

    override fun hashCode(): Int = text.hashCode()

    companion object {
        /**
         * Validates a candidate string.
         *
         * ## What is accepted
         *
         * Any non-empty text within [ClipboardLimits.MAX_TEXT_BYTES] UTF-8
         * bytes and free of U+0000. Nothing is normalised, trimmed or
         * re-encoded: ASCII, accented Latin, CJK, emoji, tabs, CR, LF and
         * CRLF all pass through unchanged. A clipboard that "helpfully"
         * rewrote content would corrupt exactly the payloads a user is least
         * able to notice — indentation-sensitive code, a password with a
         * trailing space.
         *
         * ## Why NUL is refused
         *
         * It is refused rather than carried because it cannot be carried
         * *faithfully* end to end: the X11 and `text/plain` conventions on the
         * desktop do not carry it, and any consumer along the path that
         * touches a C string API truncates at the first NUL, silently. Silent
         * truncation is the one failure a clipboard must never have, and
         * refusing is the only behaviour that is identical on both platforms
         * and visible to the user.
         *
         * ## Why oversize is refused rather than truncated
         *
         * A truncated password or command is not a shorter version of the
         * original, it is a different and wrong value that looks plausible.
         *
         * ## Encoding
         *
         * Android strings are UTF-16 internally; the limit and the hash are
         * both over the **UTF-8** encoding, because that is what crosses the
         * wire. A string containing an unpaired surrogate — which
         * `CharSequence` permits and UTF-8 cannot represent — encodes to the
         * replacement character, so the text is re-derived from the encoded
         * bytes and it is that round-tripped value which is sent. Both ends
         * then agree on exactly the same bytes.
         */
        fun validate(candidate: CharSequence?): Result<ClipboardText> {
            val raw = candidate?.toString() ?: return Result.failure(
                ClipboardRejected(TextRejection.Empty),
            )
            if (raw.isEmpty()) return Result.failure(ClipboardRejected(TextRejection.Empty))

            // Round-trip through UTF-8 so that what is validated, hashed and
            // sent are the same bytes. For well-formed text this is the
            // identity; for a string with an unpaired surrogate it is the
            // only self-consistent choice.
            val bytes = raw.toByteArray(Charsets.UTF_8)
            if (bytes.size > ClipboardLimits.MAX_TEXT_BYTES) {
                return Result.failure(ClipboardRejected(TextRejection.TooLarge(bytes.size)))
            }
            val text = String(bytes, Charsets.UTF_8)
            if (text.contains('\u0000')) {
                return Result.failure(ClipboardRejected(TextRejection.ContainsNul))
            }

            return Result.success(ClipboardText(text, contentHash(text)))
        }

        /**
         * SHA-256 over the UTF-8 bytes.
         *
         * The cross-language contract: `anyflow_capability_clipboard::text::
         * content_hash` computes the same bytes, and both suites pin the same
         * vectors.
         */
        fun contentHash(text: String): ByteArray =
            MessageDigest.getInstance("SHA-256").digest(text.toByteArray(Charsets.UTF_8))
    }
}

/** Carries a [TextRejection] through a [Result]. Never carries the content. */
class ClipboardRejected(val rejection: TextRejection) : Exception(rejection.describe())

/** Every bound `clipboard.v1` enforces on this platform. */
object ClipboardLimits {
    /** Length of a clipboard event id, in bytes. */
    const val EVENT_ID_LENGTH = 16

    /** Length of a content hash, in bytes. */
    const val CONTENT_HASH_LENGTH = 32

    /**
     * Largest clipboard text this device will send or accept, in UTF-8 bytes.
     *
     * 32 KiB, matching the desktop exactly — a limit that differed between
     * the two would mean a clip that one end sends and the other refuses.
     * It sits well below the transport's 64 KiB `MAX_FRAME_LEN` so that the
     * envelope, the hash, the ids and the protobuf framing all fit with room
     * to spare. See `limits.rs` for the full reasoning.
     */
    const val MAX_TEXT_BYTES = 32 * 1024

    /** How many recently handled event ids are remembered. */
    const val EVENT_CACHE_ENTRIES = 256

    /** How long a handled event id stays in the de-duplication cache. */
    const val EVENT_CACHE_TTL_MS = 300_000L

    /** How many content hashes may sit in the loop-suppression cache. */
    const val SUPPRESSION_ENTRIES = 64

    /**
     * How long a suppression entry survives if the local change is never
     * observed. Short on purpose: a stale entry would swallow a legitimate
     * re-copy of the same text.
     */
    const val SUPPRESSION_TTL_MS = 10_000L

    /** How long a clip held for the user survives before it is dropped. */
    const val PENDING_CLIP_TTL_MS = 300_000L
}

/**
 * Talking about clipboard content without printing it.
 *
 * Every diagnostic goes through here. The rule is absolute and has no debug
 * override: clipboard text is never rendered, not at verbose level, not in a
 * test failure. What is logged instead is the size, the hash prefix and the
 * event id — enough to correlate two devices' logs, useless to anyone
 * reading one.
 */
object Redact {
    fun hex(bytes: ByteArray): String = bytes.joinToString("") { "%02x".format(it) }

    /** First 8 hex characters of a content hash. */
    fun hashPrefix(hash: ByteArray): String = hex(hash.copyOfRange(0, minOf(4, hash.size)))

    /** First 8 hex characters of an event id. */
    fun eventPrefix(eventId: ByteArray): String =
        hex(eventId.copyOfRange(0, minOf(4, eventId.size)))
}
