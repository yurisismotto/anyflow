package io.github.yurisismotto.anyflow.notifications

import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Log
import java.security.KeyStore
import javax.crypto.KeyGenerator
import javax.crypto.Mac
import javax.crypto.SecretKey

/**
 * `device_notification_secret` — the key that names a notification.
 *
 * ## What it is, and what it is not
 *
 * 32 CSPRNG bytes, generated once per install. **It is not a credential and it
 * authenticates nothing** (ADR-0016 §4): it is not the device identity key, it
 * is not derived from it, and no protocol message proves knowledge of it. Its
 * only job is to make the transmitted `notification_id` reveal nothing about
 * the platform key it was derived from.
 *
 * ## Where it lives
 *
 * In the **Android Keystore**, as a `HmacSHA256` key, beside the device
 * identity and under the same discipline. That is stronger than the "0600 in a
 * 0700 directory" the desktop uses and stronger than what ADR-0016 §4 asks
 * for, and it is stronger for a reason worth stating: a keystore HMAC key is
 * generated inside the keystore and is **not exportable**, so the 32 bytes
 * never exist in this process's heap at all. There is nothing for a log
 * statement, a crash dump, a backup or a future bug in this app to leak,
 * because the app cannot read the secret — it can only ask the keystore to
 * compute an HMAC with it.
 *
 * Concretely this means the secret is, by construction:
 *
 *  * **never logged** — there is no accessor that returns the bytes;
 *  * **never transmitted** — likewise, and no schema field could carry it;
 *  * **never backed up or exported** — keystore key material is not included
 *    in Android backup, and `android:allowBackup="false"` is set besides;
 *  * **stable across process restart and reboot** — which is the entire reason
 *    the id is derived rather than random (ADR-0016 §2).
 *
 * ## Lifecycle
 *
 * ADR-0016 §5, implemented exactly, with one refinement this class documents
 * rather than invents:
 *
 * | Event | Secret | Ids |
 * | --- | --- | --- |
 * | App or process restart, device reboot | unchanged | unchanged |
 * | Grant revoked, then re-granted | unchanged | unchanged — revocation is not an identity event |
 * | Secret **absent** | regenerated, [State.REGENERATED] | all change |
 * | Secret present but **unreadable** | **left alone**, capability inert | — |
 * | Device identity reset / re-pair | destroyed and regenerated | all change |
 *
 * The refinement is the fourth row. ADR-0016 says a *missing or unreadable*
 * secret is regenerated, and fails *forward* on the grounds that a lost secret
 * must not disable the capability. That is right for **missing** and it is the
 * behaviour here. But "unreadable" and "absent" are two different facts, and a
 * keystore that throws on read is not evidence that the key is gone — it may
 * be a transient failure, and replacing a live secret on the strength of one
 * failed read would invalidate every mirror for no reason. So a read that
 * *fails* leaves the existing key untouched and reports [State.UNAVAILABLE];
 * the source announces no `SOURCE` role for that session and tries again next
 * time. Nothing is silently replaced, and the fail-forward property ADR-0016
 * wanted still holds for the case it was about.
 *
 * A regeneration is never silent either: [Metadata.notificationSecretGeneration]
 * distinguishes a true first creation from a replacement, and a replacement is
 * logged at warning level as a reason code with no key material.
 */
class NotificationSecret private constructor(
    private val key: SecretKey,
    /** How this instance came to exist. Diagnostics only; carries no bytes. */
    val state: State,
    /** How many times a secret has been created on this install. */
    val generation: Int,
) {

    enum class State {
        /** No secret had ever been created here. The ordinary first run. */
        CREATED,

        /** The existing secret was loaded. Ids are unchanged. */
        LOADED,

        /**
         * A secret had been created before and is now gone. It has been
         * replaced, every `notification_id` changes, and the next snapshot
         * reconciles the peer's mirrors (ADR-0016 §6).
         */
        REGENERATED,
    }

    /**
     * A `Mac` initialised with this secret, ready to derive one id.
     *
     * A fresh instance per call: `Mac` is stateful and not thread-safe, and
     * the derivation happens on the capability's own coroutine rather than on
     * the listener callback thread.
     */
    fun newMac(): Mac = Mac.getInstance(ALGORITHM).apply { init(key) }

    /** Never renders the key. There is no accessor that could. */
    override fun toString(): String =
        "NotificationSecret(state=$state, generation=$generation)"

    /**
     * Where the 32 bytes are kept.
     *
     * A seam rather than a direct keystore call, for one reason: the JVM test
     * suite has no Android Keystore, and the lifecycle rules above are exactly
     * the part that must be tested. [KeystoreSecretStore] is the only
     * implementation that ships.
     */
    interface Store {
        /**
         * The existing key, or null when there is none.
         *
         * Must **throw** rather than return null when the store cannot answer.
         * The difference decides whether a secret is replaced, so it may not
         * be collapsed: null means "there is no key", an exception means "I
         * cannot tell you".
         */
        fun load(): SecretKey?

        /** Generates and stores 32 fresh CSPRNG bytes, replacing any key. */
        fun create(): SecretKey

        /** Destroys the key. Every derived id changes after this. */
        fun destroy()
    }

    /**
     * The non-secret bookkeeping that makes a regeneration visible.
     *
     * Holds a counter and nothing else. It is not the secret, it is not
     * derived from it, and knowing it reveals nothing: its only job is to let
     * [loadOrCreate] tell "this install has never had a secret" apart from
     * "this install had one and it is gone".
     */
    interface Metadata {
        var notificationSecretGeneration: Int
    }

    companion object {
        private const val TAG = "NotificationSecret"
        private const val ALGORITHM = "HmacSHA256"
        private const val KEYSTORE = "AndroidKeyStore"

        /** The keystore alias. Versioned, as the identity alias is. */
        const val KEY_ALIAS = "anyflow-notification-secret-v1"

        /**
         * Loads the secret, creating one only when there is genuinely none.
         *
         * Returns null when the store could not be read. That is deliberately
         * *not* the same as "create a new one": see the class documentation.
         */
        fun loadOrCreate(store: Store, metadata: Metadata): NotificationSecret? {
            val existing = try {
                store.load()
            } catch (e: Exception) {
                // Not replaced. A store that cannot be read is not a store
                // that is empty, and the difference is every mirror on every
                // peer.
                Log.w(TAG, "notification secret unreadable (${e.javaClass.simpleName}); not replaced")
                return null
            }

            if (existing != null) {
                return NotificationSecret(
                    existing,
                    State.LOADED,
                    metadata.notificationSecretGeneration,
                )
            }

            val generation = metadata.notificationSecretGeneration
            val state = if (generation == 0) State.CREATED else State.REGENERATED
            if (state == State.REGENERATED) {
                // Loud, because every notification identity this device has
                // ever published has just changed. No key material, no
                // notification content: a state name and a count.
                Log.w(
                    TAG,
                    "notification secret was absent after generation $generation; " +
                        "regenerating, all notification ids change",
                )
            }

            val created = try {
                store.create()
            } catch (e: Exception) {
                Log.w(TAG, "cannot create a notification secret (${e.javaClass.simpleName})")
                return null
            }
            metadata.notificationSecretGeneration = generation + 1
            return NotificationSecret(created, state, generation + 1)
        }

        /**
         * Destroys the secret.
         *
         * Called with a device identity reset, never on its own: ADR-0016 §5
         * ties the two together because `notification_id` is
         * `HMAC(secret, key)` over a platform key that contains the app's
         * `pkg` and `uid`, both stable for the life of the install. A peer
         * that saw the same ids before and after a re-pair could link the old
         * identity to the new one, which is the correlation re-pairing exists
         * to break.
         */
        fun destroy(store: Store) {
            runCatching { store.destroy() }
        }
    }

    /**
     * The Android Keystore implementation.
     *
     * `HmacSHA256`, 256-bit, generated by the keystore's own CSPRNG inside the
     * keystore. `setUserAuthenticationRequired(false)` for the same reason the
     * identity key sets it: a notification arrives while the phone is in a
     * pocket, and a key that needed an unlock to use would make the capability
     * work only when it is least needed. The key is still confined to the TEE.
     */
    class KeystoreSecretStore : Store {

        override fun load(): SecretKey? {
            val keyStore = KeyStore.getInstance(KEYSTORE).apply { load(null) }
            val entry = keyStore.getEntry(KEY_ALIAS, null) ?: return null
            return (entry as? KeyStore.SecretKeyEntry)?.secretKey
        }

        override fun create(): SecretKey {
            val generator = KeyGenerator.getInstance(
                KeyProperties.KEY_ALGORITHM_HMAC_SHA256,
                KEYSTORE,
            )
            generator.init(
                KeyGenParameterSpec.Builder(KEY_ALIAS, KeyProperties.PURPOSE_SIGN)
                    .setKeySize(NotificationLimits.SECRET_LENGTH * 8)
                    .setUserAuthenticationRequired(false)
                    .build(),
            )
            return generator.generateKey()
        }

        override fun destroy() {
            KeyStore.getInstance(KEYSTORE).apply { load(null) }.deleteEntry(KEY_ALIAS)
        }
    }
}
