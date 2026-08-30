package dev.fedroid.bridge.store

import android.content.Context
import dev.fedroid.bridge.identity.Fingerprint
import java.io.File
import java.security.SecureRandom
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
 */
class TrustStore(context: Context) {

    private val file = File(context.filesDir, FILE_NAME)
    private var state: JSONObject = load()

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
    )

    fun peers(): List<TrustedPeer> {
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
                },
            )
        }
        state.put(KEY_PEERS, array)
        persist()
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
