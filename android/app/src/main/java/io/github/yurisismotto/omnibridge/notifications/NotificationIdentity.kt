package io.github.yurisismotto.omnibridge.notifications

import java.security.MessageDigest
import javax.crypto.Mac

/**
 * What a notification is called on the wire, and how that name is computed.
 *
 * ADR-0016, implemented. Three derivations live here and nothing else does:
 * there is no policy, no I/O, no platform type and no notification content
 * retained past the call.
 *
 * ```text
 * notification_id = HMAC-SHA256(
 *     key = device_notification_secret,
 *     msg = "omnibridge/notifications.v1/id/v1" || len32(platform_key) || platform_key
 * )[0..16]
 * ```
 *
 * **The raw Android key is never transmitted.** It is `userId|pkg|id|tag|uid`
 * and three of its five components — the profile number, the install-specific
 * uid, and the app-internal id and tag — would arrive at a desktop that never
 * had a use for them. The package name does reach the sink, deliberately, in
 * the separate `app_id` field: the derived id must never be described as
 * anonymising the source app (ADR-0016 §8).
 *
 * **The derivation does not depend on title or body**, and that is
 * load-bearing rather than incidental. An app that edits a message in place
 * would otherwise produce a second mirror, and two genuinely distinct alerts
 * that happened to read the same would merge into one.
 */
object NotificationIdentity {

    /**
     * The exact domain string. Not to be improvised.
     *
     * Pinned by `NotificationIdentityTest` on this side and by
     * `desktop/core/tests/notifications_protocol.rs` on the other, so a typo
     * fails a test rather than silently producing a different id space.
     */
    const val ID_DOMAIN = "omnibridge/notifications.v1/id/v1"

    /** ADR-0016 / [02 §6.5]: the group digest's own domain. */
    const val GROUP_DOMAIN = "omnibridge/notifications.v1/group/v1"

    /** ADR-0016 §11 / [02 §5.4]: the content digest's own domain. */
    const val CONTENT_DOMAIN = "omnibridge/notifications.v1/content/v1"

    /**
     * Big-endian `uint32`, the length-prefixing convention the pairing proof
     * and the `files.v1` data-stream MAC already use, for the same reason:
     * concatenation must be unambiguous, or two different inputs could hash
     * to one value.
     */
    fun len32(value: Int): ByteArray = byteArrayOf(
        (value ushr 24).toByte(),
        (value ushr 16).toByte(),
        (value ushr 8).toByte(),
        value.toByte(),
    )

    /**
     * The transmitted identity for one platform notification key.
     *
     * [mac] must already be initialised with `device_notification_secret`;
     * this function never sees the key material, which is what lets the secret
     * live in the Android Keystore and never enter the process.
     *
     * @return exactly [NotificationLimits.NOTIFICATION_ID_LENGTH] bytes.
     */
    fun derive(mac: Mac, platformKey: String): ByteArray {
        val keyBytes = platformKey.toByteArray(Charsets.UTF_8)
        mac.reset()
        mac.update(ID_DOMAIN.toByteArray(Charsets.UTF_8))
        mac.update(len32(keyBytes.size))
        mac.update(keyBytes)
        return mac.doFinal().copyOf(NotificationLimits.NOTIFICATION_ID_LENGTH)
    }

    /**
     * The 8-byte digest of a platform group key.
     *
     * Android's `getGroupKey()` embeds the package and often app-internal
     * identifiers; the sink needs only "these mirrors belong together", and 64
     * bits of digest says that and nothing else.
     */
    fun groupId(groupKey: String): ByteArray {
        val bytes = groupKey.toByteArray(Charsets.UTF_8)
        val digest = MessageDigest.getInstance("SHA-256")
        digest.update(GROUP_DOMAIN.toByteArray(Charsets.UTF_8))
        digest.update(len32(bytes.size))
        digest.update(bytes)
        return digest.digest().copyOf(NotificationLimits.GROUP_ID_LENGTH)
    }

    /**
     * A digest over the semantic content of one upsert.
     *
     * ## What it is for, and what it is not
     *
     * Three things and no others (ADR-0016 §11): suppressing a re-send when a
     * source re-posts an identical notification, loop detection, and
     * diagnostics that must not log content. It is **not** authentication —
     * TLS 1.3 with pinned identities and the capability grant are what make a
     * message trustworthy — and it is **not** identity.
     *
     * ## Why it is computed only here
     *
     * The sink never recomputes it. `omnibridge_core::notifications` checks the
     * *width* and nothing else, and the sink's de-duplication keys on the
     * value it was sent rather than on one it derived. So this construction
     * cannot drift between the two implementations, because only one of them
     * performs it. N2 must keep it that way: a sink that recomputed this would
     * turn a source-side optimisation into a wire-format contract.
     *
     * ## The canonical encoding
     *
     * Domain-separated, every variable-length part length-prefixed, and every
     * field that is *sent* included so that no two sendable messages share a
     * digest:
     *
     * ```text
     * SHA-256( "omnibridge/notifications.v1/content/v1"
     *        || len32(app_id)    || app_id
     *        || len32(app_label) || app_label
     *        || len32(title)     || title
     *        || len32(body)      || body
     *        || len32(importance) || len32(privacy) || len32(category)
     *        || len32(flags)
     *        || len32(progress.size) || progress
     *        || len32(group_id.size) || group_id )
     * ```
     *
     * `posted_at_unix_ms` and `notification_id` are excluded on purpose. The
     * post time changes when an app re-posts identical content, which is
     * exactly the case this digest exists to collapse; the id is the thing the
     * digest is compared *within*.
     */
    @Suppress("LongParameterList")
    fun contentHash(
        appId: String,
        appLabel: String,
        title: String,
        body: String,
        importance: Int,
        privacy: Int,
        category: Int,
        flags: Int,
        progress: ByteArray,
        groupId: ByteArray,
    ): ByteArray {
        val digest = MessageDigest.getInstance("SHA-256")
        digest.update(CONTENT_DOMAIN.toByteArray(Charsets.UTF_8))
        for (text in listOf(appId, appLabel, title, body)) {
            val bytes = text.toByteArray(Charsets.UTF_8)
            digest.update(len32(bytes.size))
            digest.update(bytes)
        }
        digest.update(len32(importance))
        digest.update(len32(privacy))
        digest.update(len32(category))
        digest.update(len32(flags))
        digest.update(len32(progress.size))
        digest.update(progress)
        digest.update(len32(groupId.size))
        digest.update(groupId)
        return digest.digest()
    }

    /** The canonical bytes of a progress bar, or empty when there is none. */
    fun progressRepr(current: Int, max: Int, indeterminate: Boolean): ByteArray =
        len32(current) + len32(max) + len32(if (indeterminate) 1 else 0)

    /** The `flags` word [contentHash] folds the booleans into. */
    @Suppress("LongParameterList")
    fun flags(
        ongoing: Boolean,
        dismissible: Boolean,
        groupSummary: Boolean,
        secondaryProfile: Boolean,
        redacted: Boolean,
    ): Int =
        (if (ongoing) 1 else 0) or
            (if (dismissible) 2 else 0) or
            (if (groupSummary) 4 else 0) or
            (if (secondaryProfile) 8 else 0) or
            (if (redacted) 16 else 0)
}

/**
 * `notification_id → platform key`, on the source, in memory only.
 *
 * ## Why this exists
 *
 * `DismissRequest` is the only message that travels sink → source and causes
 * an effect here, and it names a notification by its derived id. **This map is
 * the one reverse path in the whole design**, and it must live on the source
 * because the destination must never hold the raw key.
 *
 * So it is also the narrowest part of the security boundary: a peer's sixteen
 * opaque bytes become a platform key here and nowhere else, and only if this
 * device put them in the map itself. A key that is not in the map cannot be
 * reached, which is what makes "no remote field can identify an Android
 * notification" true structurally rather than by a check.
 *
 * ## What it holds, and what it must never hold
 *
 * Identities and a platform key. **No title, no body, no subtext, no content
 * of any kind** — the type has no field that could carry one. It is memory
 * only: ADR-0016 §2 makes it *reconstructible* by re-deriving over
 * `getActiveNotifications()`, which is precisely what lets dismissal survive a
 * process restart without persisting anything.
 *
 * ## Bounds
 *
 * Capped at [NotificationLimits.MAX_TRACKED_NOTIFICATIONS], oldest evicted
 * first. Entries are removed when the notification disappears and the whole
 * map is cleared on an identity reset, so a device that has been running for a
 * month holds no more than a device that just started.
 *
 * Not thread-safe by itself: the ordered producer owns one and serialises
 * access, which is the same discipline `ClipboardSync` applies to its caches.
 */
class SourceIdMap(
    private val capacity: Int = NotificationLimits.MAX_TRACKED_NOTIFICATIONS,
) {
    /** Keyed by the id's hex, because a `ByteArray` hashes by reference. */
    private val byId = LinkedHashMap<String, String>()

    /** The reverse direction, so a removal can drop its entry by key. */
    private val byKey = LinkedHashMap<String, String>()

    fun remember(notificationId: ByteArray, platformKey: String) {
        val hex = NotificationRedact.hex(notificationId)
        // A re-posted notification keeps its identity; re-inserting refreshes
        // its position so the oldest evicted is the least recently seen.
        byId.remove(hex)
        byKey.remove(platformKey)
        byId[hex] = platformKey
        byKey[platformKey] = hex
        while (byId.size > capacity) {
            val oldest = byId.keys.first()
            val key = byId.remove(oldest)
            if (key != null) byKey.remove(key)
        }
    }

    /** The platform key for a derived id, or null when it is not tracked. */
    fun platformKey(notificationId: ByteArray): String? =
        byId[NotificationRedact.hex(notificationId)]

    /** The derived id for a platform key, or null. */
    fun notificationId(platformKey: String): ByteArray? =
        byKey[platformKey]?.let { hex ->
            ByteArray(hex.length / 2) { hex.substring(it * 2, it * 2 + 2).toInt(16).toByte() }
        }

    /** Drops the entry for a notification that is gone. */
    fun forgetKey(platformKey: String) {
        val hex = byKey.remove(platformKey) ?: return
        byId.remove(hex)
    }

    /**
     * Replaces the map with exactly the currently-active set.
     *
     * Called after a snapshot: a notification the platform no longer lists is
     * no longer dismissible, so keeping its entry would only grow the map.
     */
    fun retainOnly(platformKeys: Collection<String>) {
        val keep = platformKeys.toSet()
        for (key in byKey.keys.toList()) {
            if (key !in keep) forgetKey(key)
        }
    }

    /** An identity reset clears it. The old ids can never be mapped again. */
    fun clear() {
        byId.clear()
        byKey.clear()
    }

    fun size(): Int = byId.size

    /** Counts only. Never renders a key or an id. */
    override fun toString(): String = "SourceIdMap(entries=${byId.size})"
}
