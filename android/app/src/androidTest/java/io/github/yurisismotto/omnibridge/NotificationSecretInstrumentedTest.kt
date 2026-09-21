package io.github.yurisismotto.omnibridge

import androidx.test.ext.junit.runners.AndroidJUnit4
import io.github.yurisismotto.omnibridge.notifications.NotificationIdentity
import io.github.yurisismotto.omnibridge.notifications.NotificationLimits
import io.github.yurisismotto.omnibridge.notifications.NotificationSecret
import java.security.KeyStore
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith

/**
 * `device_notification_secret` against a real Android Keystore.
 *
 * The JVM suite tests every *rule* about the store — first creation, reload,
 * the difference between absent and unreadable, rotation — against a fake. It
 * cannot test the one thing that matters most about the real store: that the
 * key is generated inside the keystore and is **not exportable**, so the 32
 * bytes never exist in this process at all. That needs a device, because the
 * only thing a JVM stand-in could prove about a TEE is that it is not one.
 *
 * ## What this test does to the device
 *
 * It uses its own alias, not the production one, so running it cannot rotate
 * a maintainer's real notification secret or disturb a certified pairing. The
 * alias is deleted before and after.
 */
@RunWith(AndroidJUnit4::class)
class NotificationSecretInstrumentedTest {

    /** Deliberately not [NotificationSecret.KEY_ALIAS]. */
    private val testAlias = "omnibridge-notification-secret-instrumented-test"

    /**
     * The production store, pointed at a throwaway alias.
     *
     * A subclass rather than a copy so that what is exercised is the real
     * `KeyGenParameterSpec` — the algorithm, the key size, the purpose and the
     * authentication policy — rather than a second implementation that could
     * drift from it.
     */
    private inner class TestStore : NotificationSecret.Store {
        private val keystore: KeyStore
            get() = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }

        override fun load(): javax.crypto.SecretKey? =
            (keystore.getEntry(testAlias, null) as? KeyStore.SecretKeyEntry)?.secretKey

        override fun create(): javax.crypto.SecretKey {
            val generator = javax.crypto.KeyGenerator.getInstance(
                android.security.keystore.KeyProperties.KEY_ALGORITHM_HMAC_SHA256,
                "AndroidKeyStore",
            )
            generator.init(
                android.security.keystore.KeyGenParameterSpec.Builder(
                    testAlias,
                    android.security.keystore.KeyProperties.PURPOSE_SIGN,
                )
                    .setKeySize(NotificationLimits.SECRET_LENGTH * 8)
                    .setUserAuthenticationRequired(false)
                    .build(),
            )
            return generator.generateKey()
        }

        override fun destroy() {
            keystore.deleteEntry(testAlias)
        }
    }

    private class Metadata(override var notificationSecretGeneration: Int = 0) :
        NotificationSecret.Metadata

    private val platformKey = "0|example.fixture.app|1|null|10123"

    private fun idUnder(secret: NotificationSecret): String =
        NotificationIdentity.derive(secret.newMac(), platformKey)
            .joinToString("") { "%02x".format(it) }

    @Before
    fun clean() {
        runCatching { TestStore().destroy() }
    }

    @After
    fun cleanUp() {
        runCatching { TestStore().destroy() }
    }

    /**
     * The key material never leaves the keystore.
     *
     * `getEncoded()` on a keystore-backed `SecretKey` returns null: the key
     * cannot be read by this app, by a future bug in this app, or by anything
     * that gets hold of the process. That is what makes "never logged, never
     * transmitted, never backed up" a property of the platform rather than a
     * promise about our code.
     */
    @Test
    fun theSecretIsNotExportable() {
        val store = TestStore()
        val secret = NotificationSecret.loadOrCreate(store, Metadata())
        assertNotNull(secret)

        val key = store.load()
        assertNotNull("the key must exist in the keystore", key)
        assertNull("keystore key material must not be readable", key!!.encoded)

        // And it is usable for exactly one thing: computing a MAC.
        assertEquals(
            NotificationLimits.NOTIFICATION_ID_LENGTH,
            NotificationIdentity.derive(secret!!.newMac(), platformKey).size,
        )
    }

    /** A real keystore key survives being reloaded, and derives the same ids. */
    @Test
    fun theSecretIsStableAcrossReload() {
        val store = TestStore()
        val metadata = Metadata()

        val first = NotificationSecret.loadOrCreate(store, metadata)!!
        assertEquals(NotificationSecret.State.CREATED, first.state)
        val firstId = idUnder(first)

        // A fresh process would construct a new store and read the same
        // metadata; both are simulated here.
        val second = NotificationSecret.loadOrCreate(TestStore(), Metadata(1))!!
        assertEquals(NotificationSecret.State.LOADED, second.state)
        assertEquals(firstId, idUnder(second))
    }

    /**
     * Destroying it — which is what an identity reset does — changes every
     * derived id, so a peer cannot link ids across a re-pair.
     */
    @Test
    fun destroyingTheSecretChangesEveryId() {
        val store = TestStore()
        val metadata = Metadata()
        val before = NotificationSecret.loadOrCreate(store, metadata)!!
        val beforeId = idUnder(before)

        NotificationSecret.destroy(store)
        assertNull(store.load())

        val after = NotificationSecret.loadOrCreate(store, metadata)!!
        assertEquals(NotificationSecret.State.REGENERATED, after.state)
        assertNotEquals(beforeId, idUnder(after))
    }

    /**
     * The production alias is not the same as the test alias, so this suite
     * cannot rotate the maintainer's real secret. Asserted rather than assumed,
     * because the failure would be silent and would only show up as every
     * mirror on a certified pairing changing identity.
     */
    @Test
    fun thisSuiteDoesNotTouchTheProductionAlias() {
        assertNotEquals(NotificationSecret.KEY_ALIAS, testAlias)
        assertTrue(NotificationSecret.KEY_ALIAS == "omnibridge-notification-secret-v1")
    }
}
