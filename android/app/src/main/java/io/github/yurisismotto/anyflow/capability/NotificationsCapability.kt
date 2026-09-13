package io.github.yurisismotto.anyflow.capability

import com.google.protobuf.ByteString
import io.github.yurisismotto.anyflow.identity.Fingerprint
import io.github.yurisismotto.anyflow.notifications.NotificationLimits
import io.github.yurisismotto.anyflow.notifications.NotificationSource
import io.github.yurisismotto.anyflow.proto.capabilities.NotificationControl

/**
 * `notifications.v1` — mirroring this phone's notifications to a paired
 * computer.
 *
 * Thin on purpose, exactly as [FilesCapability] and [ClipboardCapability] are:
 * it decodes, it bounds, and it routes. Every policy question has one answer
 * in one place ([NotificationSource]), and every outbound message is produced
 * by that class's single ordered coroutine rather than from here — which is
 * what keeps a removal from overtaking the upsert it refers to.
 *
 * ## Registered, and inert until three things are true
 *
 * N0 defined the capability id and registered nobody. N1 registers this, so
 * `notifications.v1` now appears in `HELLO` and can be negotiated. That is
 * deliberately **not** conditional on the current Android permission state:
 * roles exist precisely so that a capability can be supported while being
 * unable to do anything right now (ADR-0017), and making the handshake depend
 * on a permission the user can toggle at 14:32 would mean a reconnect were
 * needed to pick up a grant made in Settings.
 *
 * Support is therefore not permission. Nothing is sent until, independently:
 *
 *  * Android has granted this app notification access **and** the listener is
 *    bound;
 *  * the peer holds a `notifications.v1` grant in the local trust store, which
 *    is never automatic and is re-read per notification;
 *  * the peer has announced a `SINK` role, and this device has announced
 *    `SOURCE`.
 *
 * ## The one thing that travels the other way
 *
 * Android announces `SOURCE` **and** `DISMISS_TARGET`, which is ADR-0017 §1's
 * v1 assignment. `DismissRequest` is the only message that arrives here and
 * causes an effect on this device, and it maps to exactly one platform call:
 * `cancelNotification(key)`, with a key this device looked up in its own
 * in-memory map. Its own gate is
 * [io.github.yurisismotto.anyflow.notifications.NotificationDismissRules].
 *
 * ## What this capability cannot do, at all
 *
 * There is no `PendingIntent`, no `RemoteInput`, no `RemoteViews`, no action
 * execution, no reply, no clear-all and no snooze anywhere in it — and not
 * because a check refuses them: `notifications_v1.proto` has no field that
 * could carry any of them, and there is no method on the platform seam that
 * could invoke one. The guarantee is a shape, not a condition.
 */
class NotificationsCapability(
    private val source: NotificationSource,
) : Capability {

    override val id: String = ID

    override suspend fun onPeerConnected(context: CapabilityContext) {
        // Recorded so the source can announce roles and send a snapshot from
        // its own coroutine, in order, ahead of any live traffic.
        source.attachSession(context.peer, context.peerDeviceId) { payload ->
            context.send(ID, payload)
        }
    }

    /**
     * Decodes one inbound message and hands it on.
     *
     * The payload is untrusted, attacker-controlled bytes. The size ceiling is
     * checked before parsing so a peer cannot make this device decode
     * something the design says is malformed, and a parse failure throws —
     * which the session logs as a type with no content and does not treat as
     * fatal, because one capability misbehaving must not cost the user
     * everything else.
     */
    override suspend fun onMessage(context: CapabilityContext, payload: ByteString) {
        require(payload.size() <= NotificationLimits.MAX_NOTIFICATION_BYTES) {
            "notifications.v1 payload over the ceiling"
        }
        val control = NotificationControl.parseFrom(payload)
        source.onInbound(context.peer, control)
    }

    override suspend fun onPeerDisconnected(peer: Fingerprint) {
        source.detachSession(peer)
    }

    companion object {
        const val ID = "notifications.v1"
    }
}
