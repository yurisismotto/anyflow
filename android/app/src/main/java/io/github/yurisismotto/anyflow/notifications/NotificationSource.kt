package io.github.yurisismotto.anyflow.notifications

import android.util.Log
import com.google.protobuf.ByteString
import io.github.yurisismotto.anyflow.identity.Fingerprint
import io.github.yurisismotto.anyflow.proto.capabilities.NotificationControl
import io.github.yurisismotto.anyflow.proto.capabilities.NotificationOutcome
import io.github.yurisismotto.anyflow.proto.capabilities.NotificationResult
import io.github.yurisismotto.anyflow.proto.capabilities.NotificationRole
import io.github.yurisismotto.anyflow.proto.capabilities.SyncMarker
import java.security.SecureRandom
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch

/**
 * All `notifications.v1` source state for this phone, and the one coroutine
 * that produces its outbound traffic.
 *
 * The Kotlin counterpart of `ClipboardSync`, and it follows the same rules for
 * the same reasons: the grant and the per-peer policy are asked freshly on
 * every question rather than captured at handshake time, a peer's own claims
 * never widen anything, a notification received from one peer is never
 * forwarded to another, and nothing is written to disk.
 *
 * ## Three independent permissions
 *
 * Nothing leaves this device unless all three agree, and each is checked in a
 * different place so that no single mistake can collapse them:
 *
 * | Question | Checked by |
 * | --- | --- |
 * | May this app read notifications on this phone? | [AccessControl.isAccessGranted] and the listener actually being connected |
 * | May this computer be sent them? | [Authorizer], re-read from the trust store per notification |
 * | Can this device source at all right now? | the `SOURCE` role, announced from the same state |
 *
 * **Holding Android notification access alone sends nothing to anyone.** With
 * no granted peer there is no session, so there is nothing to send to; with a
 * session but no grant the authorizer returns [NotificationPolicy.DENIED] and
 * the filter drops every notification before it is encoded.
 *
 * ## One ordered producer
 *
 * Every message this capability sends — role announcements, upserts, removals,
 * snapshot markers and results — is written by [producer], a single coroutine
 * draining a single [NotificationOutboundQueue]. The session's outbound
 * channel guarantees FIFO per *producer*, not across producers, so two senders
 * could let a removal overtake the upsert it refers to and strand a mirror on
 * a desktop for ever. One producer makes that impossible by construction.
 *
 * It is also what keeps a snapshot coherent: `BEGIN`, its items and `END` are
 * emitted from one pass of one coroutine and cannot interleave with live
 * traffic in the wrong order.
 *
 * ## Nothing is persisted
 *
 * The id map, the per-peer de-duplication digests and the role state are in
 * memory and die with the process. The only thing this capability writes to
 * disk is the per-peer policy — settings, chosen by the user — and the only
 * thing it keeps in the keystore is the notification secret. No title, no
 * body, no platform key, no snapshot, and no history in any form.
 */
class NotificationSource(
    /** This app's own package. The own-package rule is checked against it. */
    private val ownPackage: String,
    /** This device's id, stamped into outbound messages for display only. */
    private val localDeviceId: String,
    /** The grant and policy source. Re-asked per operation, never cached. */
    private val authorizer: Authorizer,
    /** Resolves a package name to a human label. Never on the main thread. */
    private val appLabels: AppLabels,
    /** The notification secret, or null when the store cannot provide one. */
    private val secretProvider: () -> NotificationSecret?,
    /** Whether this phone is locked. Unknown counts as locked. */
    private val lockState: LockState,
    /**
     * The OS notification-access grant, and the request to bind.
     *
     * Deliberately separate from [ListenerControl]: `requestRebind` is a
     * *static* platform method precisely because it must be callable when no
     * service instance exists, which is exactly the situation the first bind
     * happens in. Routing it through the service seam would make the initial
     * bind unreachable — the service cannot ask to be created.
     */
    private val access: AccessControl,
    private val random: SecureRandom = SecureRandom(),
    private val queue: NotificationOutboundQueue = NotificationOutboundQueue(),
) {

    /**
     * The grant and policy source.
     *
     * Must return [NotificationPolicy.DENIED] for a peer that is unknown,
     * forgotten, or has no `notifications.v1` grant. The grant check and the
     * policy lookup are one call so a caller cannot do one and forget the
     * other — the same reason `ClipboardSync.Authorizer` is shaped this way.
     */
    fun interface Authorizer {
        fun policyFor(peer: Fingerprint): NotificationPolicy
    }

    /**
     * Package name to human label.
     *
     * Separate because it needs the package manager, which is a binder call
     * and therefore must not happen on the listener callback thread.
     */
    fun interface AppLabels {
        fun labelFor(packageName: String): String
    }

    /**
     * What the listener service can do for us.
     *
     * The service registers itself here when it is created. Everything that
     * touches the platform listener goes through this seam, so the source is a
     * plain object that a JVM test can drive.
     */
    interface ListenerControl {
        /**
         * Ask the system to unbind it.
         *
         * Called when the last eligible peer goes away, so that "AnyFlow reads
         * your notifications only while a granted computer is connected" is
         * structurally true rather than a promise.
         */
        fun requestUnbind()

        /**
         * The notifications currently in the shade, or null when the listener
         * is not connected.
         *
         * **Active state, never history**: `getActiveNotifications` is defined
         * by the platform as the outstanding notifications visible to the
         * current user, which is what the person sees by picking up the phone.
         */
        fun activeNotifications(): List<PlatformNotification>?

        /**
         * The package names in the shade right now, or null when the listener
         * is not connected.
         *
         * Deliberately **not** `activeNotifications().map { it.packageName }`:
         * the app picker needs the names and nothing else, and going through
         * [PlatformNotification] would materialise every title and body in
         * this process to throw them away. A binder call, so not on the main
         * thread.
         */
        fun activePackages(): List<String>?
    }

    /**
     * The half of the platform listener that does not need a live service.
     *
     * `NotificationManager.isNotificationListenerAccessGranted` and
     * `NotificationListenerService.requestRebind` are both callable from any
     * context, and both have to be: the grant is read to decide whether a bind
     * is even possible, and the bind is requested when there is no service
     * instance to ask.
     */
    interface AccessControl {
        /** Whether the user has granted notification access in Settings. */
        fun isAccessGranted(): Boolean

        /**
         * Ask the system to bind the listener.
         *
         * `requestRebind` is the only method safe to call before
         * `onListenerConnected` or after `onListenerDisconnected`.
         */
        fun requestBind()
    }

    /** One connected peer, and everything held on its behalf. */
    private class Session(
        val peer: Fingerprint,
        val peerDeviceId: String,
        val send: suspend (ByteString) -> Unit,
    ) {
        /** What we have told this peer we can do. Per connection. */
        val roles = SourceRoleState()

        /** What it told us it can do. Recorded; never an authorization input. */
        val peerRoles = PeerRoleState()

        /**
         * `notification_id` hex to the `content_hash` hex last sent.
         *
         * Suppresses an identical re-send: an app that re-posts an unchanged
         * notification produces no wire traffic. Holds digests and never
         * content, and is bounded by the same ceiling as the id map.
         */
        val sent = LinkedHashMap<String, String>()

        /**
         * Whether this connection has been given its active-state snapshot.
         *
         * A snapshot can only be sent once the peer has claimed `SINK`, and a
         * peer announces its roles just after the session comes up rather than
         * before. So the snapshot is attempted at attach and again when the
         * announcement arrives, and this is what stops the second attempt from
         * duplicating the first.
         */
        var snapshotSent = false

        fun remember(idHex: String, hashHex: String) {
            sent.remove(idHex)
            sent[idHex] = hashHex
            while (sent.size > NotificationLimits.MAX_TRACKED_NOTIFICATIONS) {
                sent.remove(sent.keys.first())
            }
        }
    }

    private val doorbell = Channel<Unit>(Channel.CONFLATED)

    /** Keyed by `fingerprint.toHex()`; a `Fingerprint` hashes by reference. */
    private val sessions = LinkedHashMap<String, Session>()

    private val idMap = SourceIdMap()

    private var listenerControl: ListenerControl? = null

    /** Whether the system currently has our listener bound. */
    private var listenerConnected = false

    private var producer: Job? = null

    private val _status = MutableStateFlow(Status())

    /**
     * What this capability is doing, for diagnostics and for N3's UI.
     *
     * Counts and states only. There is no field here that could hold a
     * notification, and no screen anywhere lists received notifications —
     * a history is what the design forbids, not a feature deferred for time.
     */
    data class Status(
        val listenerConnected: Boolean = false,
        val accessGranted: Boolean = false,
        val secretAvailable: Boolean = false,
        val sessions: Int = 0,
        val trackedNotifications: Int = 0,
        val emitted: Long = 0,
        val dropped: Long = 0,
        /**
         * Per-connected-peer role state, keyed by `fingerprint.toHex()`.
         *
         * Booleans and counters. It exists so the consent UI can tell "this
         * computer is not connected" from "connected, and it has not said it
         * can display notifications" — two states with different fixes that a
         * single switch would render identically (ADR-0017 §6).
         */
        val peers: Map<String, PeerStatus> = emptyMap(),
    )

    /**
     * What one connected peer looks like from here.
     *
     * No field can hold a notification: this is a role claim, two epochs and
     * a boolean.
     */
    data class PeerStatus(
        /** This device has announced it can source, to this peer. */
        val localIsSource: Boolean,
        val localEpoch: Long,
        /** The peer has announced it can display notifications. */
        val peerIsSink: Boolean,
        val peerEpoch: Int,
    )

    /**
     * Whether this phone can currently source, for the consent UI.
     *
     * The same question [isSourcing] answers for the producer, exposed so the
     * UI does not have to reassemble it from three flags and get it wrong.
     */
    val sourceActive: Boolean get() = _status.value.let {
        it.listenerConnected && it.accessGranted && it.secretAvailable
    }

    /**
     * The package names currently in the notification shade.
     *
     * For the app picker, so that an application with no launcher entry that
     * is actually notifying can be chosen. Only names: no title, no body, no
     * key, nothing retained. Empty when the listener is not bound, which is
     * the ordinary state when no granted computer is connected.
     *
     * A binder call. Callers must not be on the main thread.
     */
    fun activePackages(): Set<String> {
        val names = runCatching { listenerControl?.activePackages() }.getOrNull()
            ?: return emptySet()
        return names.filterTo(LinkedHashSet()) { it.isNotBlank() && it != ownPackage }
    }

    val status: StateFlow<Status> = _status.asStateFlow()

    // -----------------------------------------------------------------------
    // Lifecycle
    // -----------------------------------------------------------------------

    /** Starts the single ordered producer. Idempotent. */
    @Synchronized
    fun start(scope: CoroutineScope) {
        if (producer?.isActive == true) return
        producer = scope.launch {
            while (isActive) {
                doorbell.receive()
                for (event in queue.drain()) {
                    // One misbehaving event must not kill the producer: doing
                    // so would silently stop every future notification with no
                    // symptom other than nothing happening.
                    runCatching { handle(event) }
                        .onFailure { e ->
                            Log.w(TAG, "event ${event.javaClass.simpleName} failed: ${e.javaClass.simpleName}")
                        }
                }
            }
        }
    }

    /** Registers the platform listener. Called when the service is created. */
    fun attachListener(control: ListenerControl) {
        listenerControl = control
        publishStatus()
    }

    /** Forgets it. The source becomes inert until one is attached again. */
    fun detachListener() {
        listenerControl = null
        offer(NotificationEvent.ListenerDisconnected)
    }

    // -----------------------------------------------------------------------
    // Entry points — all non-blocking, all safe on the main thread
    // -----------------------------------------------------------------------

    /**
     * A notification was posted or updated.
     *
     * Called from `onNotificationPosted`, which runs on the phone's **main
     * thread**. It does one thing and returns: no protobuf, no HMAC, no
     * package-manager call, no I/O and no logging of content.
     */
    fun onPosted(notification: PlatformNotification) {
        offer(NotificationEvent.Posted(notification))
    }

    /** A notification is gone. Same thread, same rule. */
    fun onRemoved(platformKey: String) {
        offer(NotificationEvent.Removed(platformKey))
    }

    /** The system bound the listener. */
    fun onListenerConnected() {
        offer(NotificationEvent.ListenerConnected)
    }

    /** The listener was unbound, or access was revoked. */
    fun onListenerDisconnected() {
        offer(NotificationEvent.ListenerDisconnected)
    }

    /** A session was established. */
    fun attachSession(
        peer: Fingerprint,
        peerDeviceId: String,
        send: suspend (ByteString) -> Unit,
    ) {
        offer(NotificationEvent.SessionAttached(peer, peerDeviceId, send))
    }

    /** A session ended. */
    fun detachSession(peer: Fingerprint) {
        offer(NotificationEvent.SessionDetached(peer))
    }

    /** A grant or a policy changed locally. */
    fun policyChanged() {
        offer(NotificationEvent.PolicyChanged)
    }

    /** An inbound message, already decoded by the capability. */
    fun onInbound(peer: Fingerprint, control: NotificationControl) {
        offer(NotificationEvent.Inbound(peer, control))
    }

    private fun offer(event: NotificationEvent) {
        val result = queue.offer(event)
        if (result == NotificationOutboundQueue.Result.DROPPED) {
            // A type name and a count. Never the notification.
            Log.w(TAG, "outbound queue full; dropped ${event.javaClass.simpleName}")
        }
        doorbell.trySend(Unit)
    }

    // -----------------------------------------------------------------------
    // The producer — everything below runs on one coroutine, in order
    // -----------------------------------------------------------------------

    private suspend fun handle(event: NotificationEvent) {
        when (event) {
            is NotificationEvent.SessionAttached -> {
                val session = Session(event.peer, event.peerDeviceId, event.send)
                sessions[event.peer.toHex()] = session
                announceRoles(session)
                updateBinding()
                sendSnapshot(session)
            }

            is NotificationEvent.SessionDetached -> {
                sessions.remove(event.peer.toHex())
                updateBinding()
            }

            NotificationEvent.ListenerConnected -> {
                listenerConnected = true
                Log.i(TAG, "notification listener connected")
                // A bind can land after the peer that asked for it has gone —
                // `requestRebind` is asynchronous and the system starts the
                // service in its own time. Releasing it here is what stops a
                // departed peer from leaving the listener bound and reading,
                // which is the whole property the unbind exists to provide.
                updateBinding()
                // Re-derive the id map over the current shade. This is what
                // makes ids survive a process restart without persisting
                // anything: the map is reconstructed, never restored.
                rebuildIdMap()
                for (session in sessions.values) {
                    announceRoles(session)
                    sendSnapshot(session)
                }
            }

            NotificationEvent.ListenerDisconnected -> {
                listenerConnected = false
                Log.i(TAG, "notification listener disconnected; narrowing source role")
                // Narrow first, then forget. The peer is told we are no longer
                // a source before anything else happens, on the session that
                // is already up — no reconnect, and no window in which the
                // desktop still believes it is being mirrored to.
                for (session in sessions.values) {
                    announceRoles(session)
                }
                idMap.clear()
            }

            NotificationEvent.PolicyChanged -> updateBinding()

            is NotificationEvent.Posted -> handlePosted(event.notification)

            is NotificationEvent.Removed -> handleRemoved(event.platformKey)

            is NotificationEvent.Inbound -> handleInbound(event.peer, event.control)
        }
        publishStatus()
    }

    private suspend fun handlePosted(notification: PlatformNotification) {
        // The hard rules first, once, before any per-peer state is consulted.
        val screened = NotificationFilter.screen(notification, ownPackage)
        if (screened is FilterVerdict.Drop) {
            countDrop(screened.reason)
            return
        }
        if (!isSourcing()) {
            countDrop(DropReason.NOT_A_SOURCE)
            return
        }
        val secret = secretProvider() ?: run {
            countDrop(DropReason.NOT_A_SOURCE)
            return
        }

        val notificationId = NotificationIdentity.derive(secret.newMac(), notification.platformKey)
        idMap.remember(notificationId, notification.platformKey)

        val locked = lockState.isLocked()
        val label = appLabels.labelFor(notification.packageName)
        val idHex = NotificationRedact.hex(notificationId)

        for (session in sessions.values.toList()) {
            val policy = policyFor(session.peer)
            val verdict = NotificationFilter.decideForPeer(notification, policy, locked)
            if (verdict is FilterVerdict.Drop) {
                countDrop(verdict.reason)
                continue
            }
            if (!session.peerRoles.has(NotificationRole.NOTIFICATION_ROLE_SINK)) {
                // A peer that never claimed SINK is not sent content. Absent
                // roles mean no roles, and that is the fail-closed default
                // that lets a minimal peer interoperate harmlessly.
                countDrop(DropReason.NOT_A_SOURCE)
                continue
            }

            val reduced = NotificationReduction.reduce(notification, policy, locked)
            if (reduced == null) {
                countDrop(DropReason.LOCK_SUPPRESSED)
                continue
            }

            val control = NotificationWire.upsert(
                notificationId = notificationId,
                originDeviceId = localDeviceId,
                notification = notification,
                appLabel = label,
                reduced = reduced,
            )
            if (!NotificationWire.withinCeiling(control)) {
                // Unreachable with the field limits above; refused rather than
                // trimmed, because trimming an already-limited message is how
                // a limit becomes advisory.
                Log.w(TAG, "encoded notification over the ceiling; dropped")
                continue
            }

            val hashHex = NotificationRedact.hex(control.upsert.contentHash.toByteArray())
            if (session.sent[idHex] == hashHex) {
                // An identical re-post. Nothing happened twice.
                continue
            }
            session.remember(idHex, hashHex)
            emit(session, control)
        }
    }

    private suspend fun handleRemoved(platformKey: String) {
        val notificationId = idMap.notificationId(platformKey)
        idMap.forgetKey(platformKey)
        if (notificationId == null) {
            // Never mirrored, so there is nothing to remove anywhere. This is
            // the ordinary case for every notification a filter dropped.
            return
        }
        val idHex = NotificationRedact.hex(notificationId)
        for (session in sessions.values.toList()) {
            // Only peers that were actually sent this notification are told it
            // is gone. A peer learns nothing about notifications it never
            // received, including that they existed.
            if (session.sent.remove(idHex) == null) continue
            emit(session, NotificationWire.remove(notificationId, localDeviceId))
        }
    }

    /**
     * An inbound `notifications.v1` message.
     *
     * Android is a source in v1 and nothing else. It records a peer's role
     * announcement, notes a result, and refuses everything else with
     * `REJECTED_ROLE` — including `DismissRequest`, because **N1 announces no
     * `DISMISS_TARGET` role and contains no path to `cancelNotification`**.
     * Refusing is not an error and never closes the session: one capability
     * misbehaving must not cost the user everything else.
     */
    private suspend fun handleInbound(peer: Fingerprint, control: NotificationControl) {
        val session = sessions[peer.toHex()] ?: return

        // The grant is re-read here rather than trusted from the handshake, so
        // a revocation is in force on a session that is already up.
        if (policyFor(peer) == NotificationPolicy.DENIED) {
            answer(session, control, NotificationOutcome.NOTIFICATION_OUTCOME_NOT_AUTHORIZED)
            return
        }

        when (control.bodyCase) {
            NotificationControl.BodyCase.ROLES -> {
                val rejection = session.peerRoles.apply(control.roles)
                if (rejection != null) {
                    Log.i(TAG, "peer roles refused: $rejection")
                } else {
                    Log.i(TAG, "peer roles epoch=${session.peerRoles.epoch()}")
                    // A peer announces its roles just after the session comes
                    // up, which is after this side attached. This is where a
                    // peer that has just claimed SINK gets the snapshot it
                    // could not be sent a moment ago.
                    if (!session.snapshotSent) sendSnapshot(session)
                }
            }

            NotificationControl.BodyCase.RESULT -> {
                // A verdict on something we sent. Logged as an enum name, with
                // the opaque id prefix that lets two devices' logs be lined up.
                val id = control.result.notificationId.toByteArray()
                Log.d(
                    TAG,
                    "peer outcome ${control.result.outcome.name} for " +
                        NotificationRedact.idPrefix(id),
                )
            }

            NotificationControl.BodyCase.DISMISS,
            NotificationControl.BodyCase.UPSERT,
            NotificationControl.BodyCase.REMOVE,
            NotificationControl.BodyCase.SYNC,
            -> answer(session, control, NotificationOutcome.NOTIFICATION_OUTCOME_REJECTED_ROLE)

            else -> Unit
        }
    }

    /**
     * Answers one message with an outcome.
     *
     * A bad-width identifier is refused **and not answered**: a
     * `NotificationResult` echoes the id, so a malformed one leaves nothing
     * coherent to correlate a reply with. Same rule `clipboard.v1` applies to
     * a bad-width `event_id`.
     */
    private suspend fun answer(
        session: Session,
        control: NotificationControl,
        outcome: NotificationOutcome,
    ) {
        val id = when (control.bodyCase) {
            NotificationControl.BodyCase.DISMISS -> control.dismiss.notificationId
            NotificationControl.BodyCase.UPSERT -> control.upsert.notificationId
            NotificationControl.BodyCase.REMOVE -> control.remove.notificationId
            else -> return
        }
        if (id.size() != NotificationLimits.NOTIFICATION_ID_LENGTH) return

        emit(
            session,
            NotificationControl.newBuilder()
                .setResult(
                    NotificationResult.newBuilder()
                        .setNotificationId(id)
                        .setOutcome(outcome),
                )
                .build(),
        )
    }

    // -----------------------------------------------------------------------
    // Roles, snapshot, binding
    // -----------------------------------------------------------------------

    /**
     * Whether this device can source right now.
     *
     * All three of: the listener is connected, the OS grant is in place, and
     * the secret is usable. The grant to a peer is deliberately **not** part
     * of it: a role is what the device can physically do, and holding a peer
     * grant does not manufacture a platform capability.
     */
    private fun isSourcing(): Boolean {
        if (listenerControl == null) return false
        if (!listenerConnected) return false
        if (!accessGranted()) return false
        return secretProvider() != null
    }

    private fun accessGranted(): Boolean =
        runCatching { access.isAccessGranted() }.getOrDefault(false)

    private suspend fun announceRoles(session: Session) {
        val roles = if (isSourcing()) SourceRoleState.SOURCING else SourceRoleState.NONE
        val announcement = session.roles.announce(roles) ?: return
        Log.i(TAG, "announcing roles=${roles.size} epoch=${announcement.epoch}")
        emit(session, NotificationWire.roles(announcement))
    }

    /**
     * The active-state snapshot for one peer.
     *
     * `BEGIN`, every currently-active notification that passes every filter a
     * live one passes, then `END`. Emitted from the producer coroutine, so the
     * bracket cannot be split by live traffic arriving in between.
     *
     * **It is not a history.** Only what is in the shade at this instant —
     * things the user can see by picking up their phone — and nothing is
     * written anywhere to build it.
     */
    private suspend fun sendSnapshot(session: Session) {
        if (!isSourcing()) return
        if (!session.peerRoles.has(NotificationRole.NOTIFICATION_ROLE_SINK)) return
        // A peer with no grant is sent no snapshot at all, not even an empty
        // bracket: a `BEGIN`/`END` pair is a statement about what this device
        // is mirroring, and an ungranted peer is owed no such statement.
        if (!policyFor(session.peer).allowMirror) return
        val active = runCatching { listenerControl?.activeNotifications() }.getOrNull() ?: return
        val secret = secretProvider() ?: return

        val syncId = ByteArray(NotificationLimits.SYNC_ID_LENGTH).also { random.nextBytes(it) }
        emit(session, NotificationWire.syncMarker(syncId, SyncMarker.Phase.PHASE_BEGIN))

        val policy = policyFor(session.peer)
        val locked = lockState.isLocked()
        var sent = 0
        for (notification in active.sortedByDescending { it.postedAtUnixMs }) {
            if (sent >= NotificationLimits.MAX_SNAPSHOT_ENTRIES) break
            if (NotificationFilter.screen(notification, ownPackage) is FilterVerdict.Drop) continue
            if (NotificationFilter.decideForPeer(notification, policy, locked)
                is FilterVerdict.Drop
            ) {
                continue
            }
            val reduced = NotificationReduction.reduce(notification, policy, locked) ?: continue
            val id = NotificationIdentity.derive(secret.newMac(), notification.platformKey)
            idMap.remember(id, notification.platformKey)
            val control = NotificationWire.upsert(
                notificationId = id,
                originDeviceId = localDeviceId,
                notification = notification,
                appLabel = appLabels.labelFor(notification.packageName),
                reduced = reduced,
            )
            if (!NotificationWire.withinCeiling(control)) continue
            session.remember(
                NotificationRedact.hex(id),
                NotificationRedact.hex(control.upsert.contentHash.toByteArray()),
            )
            emit(session, control)
            sent += 1
        }

        emit(session, NotificationWire.syncMarker(syncId, SyncMarker.Phase.PHASE_END))
        session.snapshotSent = true
        Log.i(TAG, "snapshot sent: $sent of ${active.size} active")
    }

    /**
     * Re-derives the id map over the current shade.
     *
     * ADR-0016 §2: the map is *rebuilt*, never restored, which is the whole
     * reason the id is derived from a persistent secret rather than being
     * random. A process restart therefore changes no identity and a reconnect
     * reconciles in place instead of duplicating the entire shade.
     */
    private fun rebuildIdMap() {
        val secret = secretProvider() ?: return
        val active = runCatching { listenerControl?.activeNotifications() }.getOrNull() ?: return
        for (notification in active) {
            if (NotificationFilter.screen(notification, ownPackage) is FilterVerdict.Drop) continue
            idMap.remember(
                NotificationIdentity.derive(secret.newMac(), notification.platformKey),
                notification.platformKey,
            )
        }
        idMap.retainOnly(active.map { it.platformKey })
    }

    /**
     * Binds or unbinds the listener according to live peer state.
     *
     * ADR-0015 §3. The system does not bind us merely because the app is
     * installed and access is granted — `META_DATA_DEFAULT_AUTOBIND` is
     * `false` — so the binding is a function of whether a granted, mirroring
     * peer is actually connected. That is what makes "AnyFlow reads your
     * notifications only while a granted computer is connected" structurally
     * true rather than a promise.
     */
    private fun updateBinding() {
        // No OS grant means nothing to bind and nothing to release: the system
        // will not bind an unapproved listener, and asking it to would be a
        // no-op that only made the logs misleading.
        if (!accessGranted()) return

        val eligible = sessions.values.any { policyFor(it.peer).allowMirror }
        runCatching {
            if (eligible && !listenerConnected) {
                Log.i(TAG, "requesting listener bind: an eligible peer is connected")
                // Static, and it has to be: there is no service instance yet.
                access.requestBind()
            } else if (!eligible && listenerConnected) {
                Log.i(TAG, "requesting listener unbind: no eligible peer")
                listenerControl?.requestUnbind()
            }
        }
    }

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    /**
     * The effective policy for a peer, right now.
     *
     * Failing closed on any error is deliberate: an unreadable trust store is
     * a reason to permit nothing, not a reason to permit everything.
     */
    private fun policyFor(peer: Fingerprint): NotificationPolicy =
        runCatching { authorizer.policyFor(peer) }.getOrDefault(NotificationPolicy.DENIED)

    private suspend fun emit(session: Session, control: NotificationControl) {
        session.send(control.toByteString())
        emitted += 1
    }

    private fun countDrop(reason: DropReason) {
        dropped += 1
        // A reason code and a package at most. Never a title, a body or a key.
        Log.d(TAG, "not mirrored: $reason")
    }

    private var emitted = 0L
    private var dropped = 0L

    private fun publishStatus() {
        _status.value = Status(
            listenerConnected = listenerConnected,
            accessGranted = accessGranted(),
            secretAvailable = secretProvider() != null,
            sessions = sessions.size,
            trackedNotifications = idMap.size(),
            emitted = emitted,
            dropped = dropped,
            peers = sessions.mapValues { (_, session) ->
                PeerStatus(
                    localIsSource = session.roles.current()
                        ?.contains(NotificationRole.NOTIFICATION_ROLE_SOURCE) == true,
                    localEpoch = session.roles.epoch(),
                    peerIsSink = session.peerRoles
                        .has(NotificationRole.NOTIFICATION_ROLE_SINK),
                    peerEpoch = session.peerRoles.epoch(),
                )
            },
        )
    }

    /** Counts only, and safe to log. */
    fun describeQueue(): String = queue.toString()

    companion object {
        private const val TAG = "NotificationSource"
    }
}
