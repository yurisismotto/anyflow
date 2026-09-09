package io.github.yurisismotto.anyflow.notifications

import com.google.protobuf.ByteString
import io.github.yurisismotto.anyflow.proto.capabilities.NotificationControl
import io.github.yurisismotto.anyflow.proto.capabilities.NotificationRemove
import io.github.yurisismotto.anyflow.proto.capabilities.NotificationRoles
import io.github.yurisismotto.anyflow.proto.capabilities.NotificationUpsert
import io.github.yurisismotto.anyflow.proto.capabilities.Progress
import io.github.yurisismotto.anyflow.proto.capabilities.SyncMarker

/**
 * Turning an already-filtered, already-reduced notification into the wire
 * message.
 *
 * ## This is the last step, and it is the only one
 *
 * By the time anything here runs, every decision has been made: the
 * own-package rule, the allow-list, the work-profile switch, the lock policy
 * and the length limits. That is the whole point of the ordering — content
 * withheld by policy is not present in the [ReducedNotification] this function
 * is handed, so there is no field it could be written into and no later step
 * that could leak it.
 *
 * ## What is deliberately absent
 *
 * No icon, no image, no `PendingIntent`, no action, no `RemoteInput`, no
 * `RemoteViews`, no serialized `Notification` or `Bundle`, no removal reason,
 * no `sub_text`, no `big_text`, no channel id, no ranking, no `people`, no raw
 * platform key, no uid and no numeric user id. Most of those have no field in
 * the schema to write them into; the rest are simply not read.
 */
object NotificationWire {

    /**
     * One `NotificationUpsert`, wrapped in its control envelope.
     *
     * `posted_at_unix_ms` is carried informationally and is never an input to
     * authorization, ordering, expiry or de-duplication — the same rule the
     * `Envelope`'s own timestamp follows.
     */
    fun upsert(
        notificationId: ByteArray,
        originDeviceId: String,
        notification: PlatformNotification,
        appLabel: String,
        reduced: ReducedNotification,
    ): NotificationControl {
        val appId = NotificationText.reduce(
            notification.packageName,
            NotificationLimits.MAX_APP_ID_BYTES,
        )
        val label = NotificationText.reduce(appLabel, NotificationLimits.MAX_APP_LABEL_BYTES)
        val importance = NotificationMapping.importance(notification.androidImportance)
        val privacy = NotificationMapping.privacy(notification.visibility)
        val category = NotificationMapping.category(notification.category)
        val groupId = notification.groupKey?.takeIf { it.isNotEmpty() }
            ?.let { NotificationIdentity.groupId(it) }
            ?: ByteArray(0)
        val progress = if (notification.hasProgress) {
            NotificationIdentity.progressRepr(
                notification.progressCurrent,
                notification.progressMax,
                notification.progressIndeterminate,
            )
        } else {
            ByteArray(0)
        }

        val contentHash = NotificationIdentity.contentHash(
            appId = appId,
            appLabel = label,
            title = reduced.title,
            body = reduced.body,
            importance = importance.number,
            privacy = privacy.number,
            category = category.number,
            flags = NotificationIdentity.flags(
                ongoing = notification.ongoing,
                dismissible = notification.clearable,
                groupSummary = notification.groupSummary,
                secondaryProfile = notification.secondaryProfile,
                redacted = reduced.redacted,
            ),
            progress = progress,
            groupId = groupId,
        )

        val builder = NotificationUpsert.newBuilder()
            .setNotificationId(ByteString.copyFrom(notificationId))
            .setOriginDeviceId(originDeviceId)
            .setAppId(appId)
            .setAppLabel(label)
            .setTitle(reduced.title)
            .setBody(reduced.body)
            .setPostedAtUnixMs(notification.postedAtUnixMs)
            .setImportance(importance)
            .setPrivacy(privacy)
            .setCategory(category)
            .setOngoing(notification.ongoing)
            .setDismissible(notification.clearable)
            .setGroupSummary(notification.groupSummary)
            .setSecondaryProfile(notification.secondaryProfile)
            .setRedacted(reduced.redacted)
            .setContentHash(ByteString.copyFrom(contentHash))

        if (notification.hasProgress) {
            builder.setProgress(
                Progress.newBuilder()
                    .setCurrent(notification.progressCurrent)
                    .setMax(notification.progressMax)
                    .setIndeterminate(notification.progressIndeterminate),
            )
        }
        if (groupId.isNotEmpty()) {
            builder.setGroupId(ByteString.copyFrom(groupId))
        }

        return NotificationControl.newBuilder().setUpsert(builder).build()
    }

    /**
     * One `NotificationRemove`.
     *
     * Carries an identity and nothing else. **No reason code**: all 23 Android
     * removal reasons mean the same thing to a mirror, and shipping the reason
     * would leak facts about the user's device — `REASON_PACKAGE_BANNED`,
     * `REASON_CLEAR_DATA` — while inviting an implementation to treat some
     * removals as "soft". There is no reason for which the correct action is
     * to keep showing it.
     */
    fun remove(notificationId: ByteArray, originDeviceId: String): NotificationControl =
        NotificationControl.newBuilder()
            .setRemove(
                NotificationRemove.newBuilder()
                    .setNotificationId(ByteString.copyFrom(notificationId))
                    .setOriginDeviceId(originDeviceId),
            )
            .build()

    /** A role announcement, already built by [SourceRoleState]. */
    fun roles(announcement: NotificationRoles): NotificationControl =
        NotificationControl.newBuilder().setRoles(announcement).build()

    /** One end of a snapshot bracket. */
    fun syncMarker(syncId: ByteArray, phase: SyncMarker.Phase): NotificationControl =
        NotificationControl.newBuilder()
            .setSync(
                SyncMarker.newBuilder()
                    .setSyncId(ByteString.copyFrom(syncId))
                    .setPhase(phase),
            )
            .build()

    /**
     * Whether an encoded message is within the capability's own ceiling.
     *
     * Checked at the source so an oversized message is never sent rather than
     * being refused at the far end. The limits above make it unreachable in
     * practice — 255 + 128 + 512 + 4096 bytes of text plus fixed-width
     * identifiers is well under 8 KiB — which is precisely why a check that
     * fails here means a bug rather than a large notification.
     */
    fun withinCeiling(control: NotificationControl): Boolean =
        control.serializedSize <= NotificationLimits.MAX_NOTIFICATION_BYTES
}
