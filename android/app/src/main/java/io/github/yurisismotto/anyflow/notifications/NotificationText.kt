package io.github.yurisismotto.anyflow.notifications

/**
 * Every bound and every text rule `notifications.v1` enforces on this
 * platform.
 *
 * The limits are the ones `anyflow_core::notifications` pins on the desktop.
 * A limit that differed between the two ends would mean a notification one
 * side sends and the other refuses, which is the failure the shared `.proto`
 * and the shared constants exist to make impossible.
 */
object NotificationLimits {

    /** Exactly, never "at most". Identifiers are refused, never truncated. */
    const val NOTIFICATION_ID_LENGTH = 16

    /** One CSPRNG value per snapshot. Same width, same rule. */
    const val SYNC_ID_LENGTH = 16

    /** Truncated SHA-256 of the source's group key. Absent or exactly this. */
    const val GROUP_ID_LENGTH = 8

    /** SHA-256. Absent or exactly this. */
    const val CONTENT_HASH_LENGTH = 32

    /** `device_notification_secret`: 32 CSPRNG bytes (ADR-0016 §4). */
    const val SECRET_LENGTH = 32

    /** Android's package-name ceiling. */
    const val MAX_APP_ID_BYTES = 255
    const val MAX_APP_LABEL_BYTES = 128
    const val MAX_TITLE_BYTES = 512
    const val MAX_BODY_BYTES = 4096

    /** The whole encoded `NotificationControl`, before the envelope. */
    const val MAX_NOTIFICATION_BYTES = 8 * 1024

    /**
     * How many active notifications one snapshot may name.
     *
     * A source-only tunable: it is never negotiated and never on the wire
     * ([02 §7.5]). It exists so a snapshot cannot become a way to make one
     * peer send megabytes.
     */
    const val MAX_SNAPSHOT_ENTRIES = 100

    /**
     * How many `notification_id -> platform key` entries the source keeps.
     *
     * Bounds the only map N1 holds. Comfortably above any real notification
     * shade, and a hard ceiling regardless of what the platform reports.
     */
    const val MAX_TRACKED_NOTIFICATIONS = 512

    /**
     * How many events may be queued between the listener callback and the
     * ordered producer.
     *
     * Bounded on purpose: `Channel.UNLIMITED` would let a notification storm
     * grow the heap without limit, and the callback runs on the phone's main
     * thread so it can never be allowed to block. See
     * [NotificationOutboundQueue] for what happens when this is reached.
     */
    const val EVENT_QUEUE_CAPACITY = 256
}

/**
 * What the source is allowed to put in a text field, and how it shortens one
 * that is too long.
 *
 * Two rules, and the asymmetry between them is deliberate
 * ([02 §11.2](../../../../../../../../docs/research/notifications-v1/02-PROTOCOL-AND-EVENT-MODEL.md)):
 *
 *  * **the source truncates** — a notification body is display text the user
 *    is already reading in truncated form on their own phone, and dropping a
 *    4 KiB chat message entirely would be worse than shortening it;
 *  * **the receiver refuses** — at that point the message is malformed by our
 *    own rules, and a receiver that repairs malformed input is one whose
 *    limits are advisory.
 *
 * `clipboard.v1` refuses oversize text at both ends, and that difference is
 * not an inconsistency: a truncated password is a different, wrong value that
 * looks plausible, whereas a truncated notification body is still the message.
 */
object NotificationText {

    /** Appended when [reduce] shortened the text, so truncation is visible. */
    const val ELLIPSIS = "…"

    /**
     * Sanitizes and length-caps one attacker-controlled display string.
     *
     * ## Control characters are stripped, not refused
     *
     * `app_label`, `title` and `body` are chosen by an arbitrary application
     * and end up in a desktop popup and in a terminal (THREAT_MODEL T17), so
     * the same discipline `TrustStore.sanitizeDeviceName` applies is applied
     * here. Newline and tab survive, because a multi-line notification body is
     * ordinary and rewriting it would corrupt user content; every other C0 and
     * C1 control goes, including **U+0000**.
     *
     * NUL is stripped here rather than carried because the receiver refuses a
     * string containing one. Stripping at the source and refusing at the
     * receiver are the same rule seen from two ends: nothing this function
     * emits can contain a character the far side would reject.
     *
     * ## Truncation is on a UTF-8 boundary
     *
     * The limits are in UTF-8 bytes, because that is what crosses the wire,
     * and a cut mid-sequence would produce bytes that are not text. The result
     * is always valid UTF-8 and always within [limitBytes], including the
     * ellipsis.
     */
    fun reduce(candidate: CharSequence?, limitBytes: Int): String {
        val raw = candidate?.toString() ?: return ""
        val cleaned = buildString(raw.length) {
            for (ch in raw) {
                if (ch == '\n' || ch == '\t') {
                    append(ch)
                } else if (!ch.isISOControl()) {
                    append(ch)
                }
            }
        }
        return truncateUtf8(cleaned, limitBytes)
    }

    /** True when [value] would have to be shortened to fit [limitBytes]. */
    fun exceeds(value: String, limitBytes: Int): Boolean =
        value.toByteArray(Charsets.UTF_8).size > limitBytes

    /**
     * Shortens [value] to at most [limitBytes] UTF-8 bytes on a character
     * boundary, appending [ELLIPSIS] when anything was removed.
     *
     * Never used on an identifier. Shortening an identifier is how collisions
     * are manufactured, so `notification_id`, `group_id` and `content_hash`
     * are fixed-width or absent and never pass through here.
     */
    fun truncateUtf8(value: String, limitBytes: Int): String {
        val bytes = value.toByteArray(Charsets.UTF_8)
        if (bytes.size <= limitBytes) return value

        val ellipsisBytes = ELLIPSIS.toByteArray(Charsets.UTF_8).size
        val budget = limitBytes - ellipsisBytes
        if (budget <= 0) return ""

        // Walk code points rather than chars so a surrogate pair is never
        // split, and stop at the last one that still fits the budget.
        var kept = 0
        var used = 0
        while (kept < value.length) {
            val codePoint = value.codePointAt(kept)
            val width = String(Character.toChars(codePoint)).toByteArray(Charsets.UTF_8).size
            if (used + width > budget) break
            used += width
            kept += Character.charCount(codePoint)
        }
        return value.substring(0, kept) + ELLIPSIS
    }
}

/**
 * Talking about notifications without printing one.
 *
 * The rule is absolute and has no debug override: no notification title, body,
 * subtext, raw platform key or tag is rendered at any level, including
 * verbose, and not in a test failure. What may be logged is an event type, a
 * reason code, a count, and the first bytes of an already-opaque
 * `notification_id`.
 *
 * On the certification hardware the platform's own OTP redaction did not fire
 * at all (POC-NOTIF-01), so every string a notification carries is treated as
 * fully sensitive user data.
 */
object NotificationRedact {

    fun hex(bytes: ByteArray): String = bytes.joinToString("") { "%02x".format(it) }

    /**
     * First 8 hex characters of a derived `notification_id`.
     *
     * Safe because the id is already an HMAC of the platform key under a
     * secret that never leaves the device: it names a notification without
     * describing one, and it is what lets two devices' logs be correlated.
     */
    fun idPrefix(notificationId: ByteArray): String =
        hex(notificationId.copyOfRange(0, minOf(4, notificationId.size)))

    /**
     * A platform key, reduced to something that cannot be logged by accident.
     *
     * The raw key is `userId|pkg|id|tag|uid` and never appears in a log or on
     * a wire. This returns a fixed placeholder plus its length, which is
     * enough to see that a key was present and useless to anyone reading it.
     */
    fun platformKey(key: String): String = "key(len=${key.length})"
}
