package io.github.yurisismotto.anyflow.store

import android.content.Context
import io.github.yurisismotto.anyflow.clipboard.ClipboardPolicy
import io.github.yurisismotto.anyflow.identity.Fingerprint
import io.github.yurisismotto.anyflow.notifications.NotificationPolicy
import io.github.yurisismotto.anyflow.notifications.NotificationSecret
import java.io.File
import java.security.SecureRandom
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import org.json.JSONArray
import org.json.JSONObject

/**
 * Paired computers, on disk.
 *
 * ## What is stored
 *
 * Identity metadata, pinned public-key fingerprints, per-capability grants,
 * settings. Nothing else. No message content and no transfer history.
 *
 * ## Where
 *
 * A JSON file in the app's private `filesDir`. On Android that directory is
 * already isolated per-app by the platform sandbox, and the file contains no
 * secrets: the private key lives in the Keystore and never touches this file,
 * and a fingerprint is a public value. Encrypting it would add a key
 * management problem without protecting anything that is not already public.
 *
 * The `schemaVersion` field exists from the first commit so later migrations
 * are possible.
 *
 * ## Observability
 *
 * [peersFlow] is the single source of truth for the UI. Reading [peers] once
 * during composition is what made the previous release's device card ignore a
 * grant that changed elsewhere: the value was correct when read and never
 * read again. Every mutator below publishes, so a screen that collects the
 * flow cannot show stale trust state.
 */
class TrustStore(context: Context) : NotificationSecret.Metadata {

    private val file = File(context.filesDir, FILE_NAME)
    private var state: JSONObject = load()

    private val _peersFlow = MutableStateFlow(readPeers())

    /**
     * Every known computer, republished on every change.
     *
     * The UI must observe this rather than calling [peers], so that a grant
     * or a clipboard policy changed on one screen is visible on every other.
     */
    val peersFlow: StateFlow<List<TrustedPeer>> = _peersFlow.asStateFlow()

    val deviceId: String get() = state.getString(KEY_DEVICE_ID)

    var deviceName: String
        get() = state.optString(KEY_DEVICE_NAME, android.os.Build.MODEL ?: "Android")
        set(value) {
            state.put(KEY_DEVICE_NAME, sanitizeDeviceName(value))
            persist()
        }

    data class TrustedPeer(
        val deviceId: String,
        val deviceName: String,
        val fingerprint: Fingerprint,
        val pairedAtUnix: Long,
        val grantedCapabilities: Set<String>,
        val addresses: List<String>,
        /**
         * Per-peer `clipboard.v1` direction and automation settings.
         *
         * Stored next to the grant but deliberately separate from it: the
         * grant says whether this computer may speak clipboard at all, this
         * says in which directions and how automatically. Both are decided
         * locally — no protocol message writes either — and neither holds
         * clipboard content.
         */
        val clipboardPolicy: ClipboardPolicy = ClipboardPolicy(),
        /**
         * Per-peer `notifications.v1` mirroring settings.
         *
         * Beside the grant and deliberately separate from it, exactly as
         * [clipboardPolicy] is: the grant says whether this computer may
         * receive notifications at all, this says which ones and how much of
         * each. Both are decided locally — **no protocol message writes
         * either** — and neither holds a notification's content. The app list
         * is empty by default, so a freshly granted computer receives nothing
         * until a person names an application.
         */
        val notificationPolicy: NotificationPolicy = NotificationPolicy(),
    ) {
        fun allows(capabilityId: String): Boolean = capabilityId in grantedCapabilities
    }

    fun peers(): List<TrustedPeer> = _peersFlow.value

    private fun readPeers(): List<TrustedPeer> {
        val array = state.optJSONArray(KEY_PEERS) ?: return emptyList()
        return (0 until array.length()).mapNotNull { index ->
            val entry = array.optJSONObject(index) ?: return@mapNotNull null
            val fingerprint = Fingerprint.fromHex(entry.optString(KEY_FINGERPRINT))
                ?: return@mapNotNull null
            TrustedPeer(
                deviceId = entry.optString(KEY_DEVICE_ID),
                deviceName = entry.optString(KEY_DEVICE_NAME),
                fingerprint = fingerprint,
                pairedAtUnix = entry.optLong(KEY_PAIRED_AT),
                grantedCapabilities = entry.optJSONArray(KEY_GRANTS)
                    ?.let { grants -> (0 until grants.length()).map { grants.getString(it) } }
                    ?.toSet()
                    ?: emptySet(),
                addresses = entry.optJSONArray(KEY_ADDRESSES)
                    ?.let { list -> (0 until list.length()).map { list.getString(it) } }
                    ?: emptyList(),
                // A record written before this capability existed has no
                // policy object; `fromJson` supplies the documented defaults
                // rather than turning everything off — or, worse, on.
                clipboardPolicy = ClipboardPolicy.fromJson(
                    entry.optJSONObject(KEY_CLIPBOARD_POLICY),
                ),
                notificationPolicy = NotificationPolicy.fromJson(
                    entry.optJSONObject(KEY_NOTIFICATION_POLICY),
                ),
            )
        }
    }

    fun peer(fingerprint: Fingerprint): TrustedPeer? =
        peers().firstOrNull { it.fingerprint.contentEquals(fingerprint) }

    fun addPeer(peer: TrustedPeer) {
        val remaining = peers().filterNot { it.fingerprint.contentEquals(peer.fingerprint) }
        writePeers(remaining + peer)
    }

    /** Forgets a computer. It cannot reconnect without pairing again. */
    fun removePeer(fingerprint: Fingerprint) {
        writePeers(peers().filterNot { it.fingerprint.contentEquals(fingerprint) })
    }

    /**
     * Grants or withdraws one capability for one computer.
     *
     * Takes effect immediately: the grant is re-read from here on every
     * question, so withdrawing it stops traffic on a session that is already
     * connected rather than at the next reconnect.
     */
    fun setGrant(fingerprint: Fingerprint, capabilityId: String, granted: Boolean) {
        val peer = peer(fingerprint) ?: return
        val grants = peer.grantedCapabilities.toMutableSet()
        if (granted) grants += capabilityId else grants -= capabilityId
        addPeer(peer.copy(grantedCapabilities = grants))
    }

    /** Replaces one computer's clipboard policy. */
    fun setClipboardPolicy(fingerprint: Fingerprint, policy: ClipboardPolicy) {
        val peer = peer(fingerprint) ?: return
        addPeer(peer.copy(clipboardPolicy = policy))
    }

    /**
     * The effective clipboard policy for a computer, right now.
     *
     * One call answers the grant *and* the policy on purpose. A caller that
     * had to ask both separately could do the second and forget the first,
     * and the failure would be silent — a forgotten computer whose stored
     * policy still said `allowReceive`.
     */
    fun clipboardPolicyFor(fingerprint: Fingerprint): ClipboardPolicy {
        val peer = peer(fingerprint) ?: return ClipboardPolicy.DENIED
        if (!peer.allows(CLIPBOARD_CAPABILITY_ID)) return ClipboardPolicy.DENIED
        return peer.clipboardPolicy
    }

    /** Replaces one computer's notification policy. */
    fun setNotificationPolicy(fingerprint: Fingerprint, policy: NotificationPolicy) {
        val peer = peer(fingerprint) ?: return
        addPeer(peer.copy(notificationPolicy = policy))
    }

    /**
     * The effective notification policy for a computer, right now.
     *
     * One call answers the grant *and* the policy, for the reason
     * [clipboardPolicyFor] does: a caller that had to ask both separately
     * could do the second and forget the first, and the failure would be
     * silent — a forgotten computer whose stored policy still named twenty
     * applications.
     */
    fun notificationPolicyFor(fingerprint: Fingerprint): NotificationPolicy {
        val peer = peer(fingerprint) ?: return NotificationPolicy.DENIED
        if (!peer.allows(NOTIFICATIONS_CAPABILITY_ID)) return NotificationPolicy.DENIED
        return peer.notificationPolicy
    }

    /**
     * How many times a `device_notification_secret` has been created here.
     *
     * A counter, and nothing else. It is not the secret, it is not derived
     * from it, and it reveals nothing: its only job is to let
     * [NotificationSecret.loadOrCreate] tell "this install has never had a
     * secret" apart from "this install had one and it is gone", so that a
     * regeneration is reported rather than silent. See ADR-0016 §5.
     */
    override var notificationSecretGeneration: Int
        get() = state.optInt(KEY_NOTIFICATION_SECRET_GENERATION, 0)
        set(value) {
            state.put(KEY_NOTIFICATION_SECRET_GENERATION, value)
            persist()
        }

    /** Remembers where a peer was last reachable, to skip discovery next time. */
    fun rememberAddresses(fingerprint: Fingerprint, addresses: List<String>) {
        val peer = peer(fingerprint) ?: return
        addPeer(peer.copy(addresses = addresses.distinct().take(MAX_REMEMBERED_ADDRESSES)))
    }

    private fun writePeers(peers: List<TrustedPeer>) {
        val array = JSONArray()
        for (peer in peers) {
            array.put(
                JSONObject().apply {
                    put(KEY_DEVICE_ID, peer.deviceId)
                    put(KEY_DEVICE_NAME, sanitizeDeviceName(peer.deviceName))
                    put(KEY_FINGERPRINT, peer.fingerprint.toHex())
                    put(KEY_PAIRED_AT, peer.pairedAtUnix)
                    put(KEY_GRANTS, JSONArray(peer.grantedCapabilities.toList()))
                    put(KEY_ADDRESSES, JSONArray(peer.addresses))
                    // Settings, never content: the policy flags are stored,
                    // and no clipboard text ever reaches this file.
                    put(KEY_CLIPBOARD_POLICY, peer.clipboardPolicy.toJson())
                    // Settings, never content: the flags and the list of
                    // package names the person chose. No notification title,
                    // body, subtext or platform key reaches this file, and
                    // there is no field here that could hold one.
                    put(KEY_NOTIFICATION_POLICY, peer.notificationPolicy.toJson())
                },
            )
        }
        state.put(KEY_PEERS, array)
        persist()
        // Published after the write, so an observer that reacts by reading
        // the file sees what was published.
        _peersFlow.value = readPeers()
    }

    private fun load(): JSONObject {
        val existing = runCatching { JSONObject(file.readText()) }.getOrNull()
        if (existing != null && existing.optInt(KEY_SCHEMA) == SCHEMA_VERSION) {
            return existing
        }
        if (existing != null && existing.optInt(KEY_SCHEMA) > SCHEMA_VERSION) {
            // Refuse to reinterpret a newer file with older rules: silently
            // dropping fields we do not understand could drop a revocation.
            error("trust store schema is newer than this app supports")
        }
        return JSONObject().apply {
            put(KEY_SCHEMA, SCHEMA_VERSION)
            put(KEY_DEVICE_ID, randomDeviceId())
            put(KEY_DEVICE_NAME, android.os.Build.MODEL ?: "Android")
            put(KEY_PEERS, JSONArray())
        }.also { file.writeText(it.toString()) }
    }

    private fun persist() {
        // Write to a temp file and rename, so an interrupted write cannot
        // leave a truncated trust store behind.
        val temp = File(file.parentFile, "$FILE_NAME.tmp")
        temp.writeText(state.toString())
        temp.renameTo(file)
    }

    companion object {
        private const val FILE_NAME = "trust-store.json"
        const val SCHEMA_VERSION = 1
        private const val MAX_REMEMBERED_ADDRESSES = 4

        private const val KEY_SCHEMA = "schemaVersion"
        private const val KEY_DEVICE_ID = "deviceId"
        private const val KEY_DEVICE_NAME = "deviceName"
        private const val KEY_PEERS = "peers"
        private const val KEY_FINGERPRINT = "fingerprint"
        private const val KEY_PAIRED_AT = "pairedAtUnix"
        private const val KEY_GRANTS = "grantedCapabilities"
        private const val KEY_ADDRESSES = "addresses"
        private const val KEY_CLIPBOARD_POLICY = "clipboardPolicy"
        private const val KEY_NOTIFICATION_POLICY = "notificationPolicy"
        private const val KEY_NOTIFICATION_SECRET_GENERATION = "notificationSecretGeneration"

        /** Duplicated from `ClipboardCapability.ID` to avoid a cycle. */
        const val CLIPBOARD_CAPABILITY_ID = "clipboard.v1"

        /** Duplicated from `NotificationsCapability.ID`, for the same reason. */
        const val NOTIFICATIONS_CAPABILITY_ID = "notifications.v1"

        /** 128 random bits, hex. Not derived from any hardware identifier. */
        fun randomDeviceId(): String {
            val bytes = ByteArray(16)
            SecureRandom().nextBytes(bytes)
            return bytes.joinToString("") { "%02x".format(it) }
        }

        /**
         * A device name arrives from the network and is shown in the UI.
         * Control characters could forge notification text, so they go.
         */
        fun sanitizeDeviceName(name: String): String =
            name.filter { !it.isISOControl() }.take(64)
    }
}
