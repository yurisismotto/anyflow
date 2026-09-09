package io.github.yurisismotto.anyflow

import io.github.yurisismotto.anyflow.notifications.NotificationIdentity
import io.github.yurisismotto.anyflow.notifications.NotificationLimits
import io.github.yurisismotto.anyflow.notifications.NotificationSecret
import javax.crypto.SecretKey
import javax.crypto.spec.SecretKeySpec
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * `device_notification_secret` — the lifecycle, ADR-0016 §5.
 *
 * The production store is the Android Keystore and cannot run here; what runs
 * here is every rule *about* the store, against a fake that can be made to
 * behave like a keystore that is empty, populated, broken or wiped. Those
 * rules are the part that decides whether a person's mirrors survive a restart
 * or all change for no reason, so they are the part that is tested.
 *
 * The keystore itself is exercised on hardware — see the instrumented suite —
 * because the only thing a JVM stand-in could prove about a TEE is that a JVM
 * stand-in is not one.
 */
class NotificationSecretTest {

    /**
     * A keystore that can be empty, populated, or broken in each direction.
     *
     * Deliberately not a mock: the behaviours that matter — "there is no key"
     * versus "I cannot tell you" — are exactly the two a loose mock would let
     * a test conflate.
     */
    private class FakeStore : NotificationSecret.Store {
        var stored: SecretKey? = null
        var failLoad = false
        var failCreate = false
        var creations = 0

        override fun load(): SecretKey? {
            if (failLoad) throw IllegalStateException("keystore unavailable")
            return stored
        }

        override fun create(): SecretKey {
            if (failCreate) throw IllegalStateException("keystore unavailable")
            creations += 1
            // Stands in for the keystore's own CSPRNG. Distinct per creation,
            // so a test can tell a replacement from a reload.
            val bytes = ByteArray(NotificationLimits.SECRET_LENGTH) {
                ((it + creations * 31) % 256).toByte()
            }
            val key = SecretKeySpec(bytes, "HmacSHA256")
            stored = key
            return key
        }

        override fun destroy() {
            stored = null
        }
    }

    private class FakeMetadata(override var notificationSecretGeneration: Int = 0) :
        NotificationSecret.Metadata

    private val platformKey = "0|example.fixture.app|1|null|10123"

    private fun idUnder(secret: NotificationSecret): String =
        NotificationIdentity.derive(secret.newMac(), platformKey)
            .joinToString("") { "%02x".format(it) }

    // -- first creation ------------------------------------------------------

    @Test
    fun `first creation reports CREATED and records generation one`() {
        val store = FakeStore()
        val metadata = FakeMetadata()

        val secret = NotificationSecret.loadOrCreate(store, metadata)

        assertNotNull(secret)
        assertEquals(NotificationSecret.State.CREATED, secret!!.state)
        assertEquals(1, secret.generation)
        assertEquals(1, metadata.notificationSecretGeneration)
        assertEquals(1, store.creations)
    }

    @Test
    fun `a created secret is thirty-two bytes of key material`() {
        val store = FakeStore()
        NotificationSecret.loadOrCreate(store, FakeMetadata())
        assertEquals(NotificationLimits.SECRET_LENGTH, store.stored!!.encoded.size)
        assertEquals(32, NotificationLimits.SECRET_LENGTH)
    }

    // -- ordinary reload -----------------------------------------------------

    @Test
    fun `a normal reload returns the same secret and the same ids`() {
        val store = FakeStore()
        val metadata = FakeMetadata()

        val first = NotificationSecret.loadOrCreate(store, metadata)!!
        val second = NotificationSecret.loadOrCreate(store, metadata)!!

        assertEquals(NotificationSecret.State.LOADED, second.state)
        assertEquals(1, store.creations)
        assertEquals(idUnder(first), idUnder(second))
    }

    /**
     * A process restart is exactly a fresh `loadOrCreate` against the same
     * store, and it must change nothing. This is the whole reason the id is
     * derived rather than random: without it a restart would make every
     * notification arrive at the desktop as a new one and the reconnect
     * snapshot would duplicate the entire shade.
     */
    @Test
    fun `a process restart keeps the same secret`() {
        val store = FakeStore()
        val before = NotificationSecret.loadOrCreate(store, FakeMetadata())!!
        val beforeId = idUnder(before)

        // A new process: new metadata object reading the same persisted count,
        // same keystore.
        val after = NotificationSecret.loadOrCreate(store, FakeMetadata(1))!!

        assertEquals(NotificationSecret.State.LOADED, after.state)
        assertEquals(beforeId, idUnder(after))
    }

    @Test
    fun `a reload does not touch the generation counter`() {
        val store = FakeStore()
        val metadata = FakeMetadata()
        NotificationSecret.loadOrCreate(store, metadata)
        NotificationSecret.loadOrCreate(store, metadata)
        NotificationSecret.loadOrCreate(store, metadata)
        assertEquals(1, metadata.notificationSecretGeneration)
    }

    // -- the two failure modes, which are not the same fact -------------------

    /**
     * **The no-silent-regeneration rule.** A store that throws is not a store
     * that is empty, and replacing a live secret on the strength of one failed
     * read would invalidate every mirror on every peer for nothing.
     */
    @Test
    fun `an unreadable store never replaces the secret`() {
        val store = FakeStore()
        val metadata = FakeMetadata()
        val original = NotificationSecret.loadOrCreate(store, metadata)!!
        val originalId = idUnder(original)

        store.failLoad = true
        assertNull(NotificationSecret.loadOrCreate(store, metadata))

        // Nothing was created, nothing was counted, nothing was destroyed.
        assertEquals(1, store.creations)
        assertEquals(1, metadata.notificationSecretGeneration)

        store.failLoad = false
        val recovered = NotificationSecret.loadOrCreate(store, metadata)!!
        assertEquals(NotificationSecret.State.LOADED, recovered.state)
        assertEquals(originalId, idUnder(recovered))
    }

    /**
     * A secret that is genuinely **absent** after one existed is regenerated —
     * fail forward, because it is not a credential and losing it must not
     * disable the capability — and the regeneration is reported, not silent.
     */
    @Test
    fun `an absent secret after one existed regenerates and says so`() {
        val store = FakeStore()
        val metadata = FakeMetadata()
        val original = NotificationSecret.loadOrCreate(store, metadata)!!
        val originalId = idUnder(original)

        store.destroy()

        val replacement = NotificationSecret.loadOrCreate(store, metadata)!!
        assertEquals(NotificationSecret.State.REGENERATED, replacement.state)
        assertEquals(2, replacement.generation)
        assertEquals(2, metadata.notificationSecretGeneration)
        assertNotEquals(originalId, idUnder(replacement))
    }

    /**
     * The distinction the counter exists for: an empty keystore on a fresh
     * install is a first creation, and an empty keystore on an install that
     * has had a secret is a loss. They must not report the same thing.
     */
    @Test
    fun `a first creation and a regeneration are distinguishable`() {
        val fresh = NotificationSecret.loadOrCreate(FakeStore(), FakeMetadata(0))!!
        val lost = NotificationSecret.loadOrCreate(FakeStore(), FakeMetadata(7))!!

        assertEquals(NotificationSecret.State.CREATED, fresh.state)
        assertEquals(NotificationSecret.State.REGENERATED, lost.state)
        assertEquals(8, lost.generation)
    }

    @Test
    fun `a store that cannot create anything yields no secret`() {
        val store = FakeStore().apply { failCreate = true }
        val metadata = FakeMetadata()
        assertNull(NotificationSecret.loadOrCreate(store, metadata))
        // The counter is only advanced by a creation that happened.
        assertEquals(0, metadata.notificationSecretGeneration)
    }

    // -- identity reset ------------------------------------------------------

    /**
     * ADR-0016 §5: the secret must not outlive an identity reset, because a
     * peer that saw the same ids before and after a re-pair could link the old
     * identity to the new one.
     */
    @Test
    fun `an identity reset rotates the secret and every id`() {
        val store = FakeStore()
        val metadata = FakeMetadata()
        val before = NotificationSecret.loadOrCreate(store, metadata)!!
        val beforeId = idUnder(before)

        NotificationSecret.destroy(store)
        assertNull(store.stored)

        val after = NotificationSecret.loadOrCreate(store, metadata)!!
        assertEquals(NotificationSecret.State.REGENERATED, after.state)
        assertNotEquals(beforeId, idUnder(after))
    }

    /**
     * Revocation is not an identity event. Granting, revoking and re-granting
     * touches nothing here, and the ids a peer already knows stay valid.
     */
    @Test
    fun `revoking and re-granting leaves the secret alone`() {
        val store = FakeStore()
        val metadata = FakeMetadata()
        val secret = NotificationSecret.loadOrCreate(store, metadata)!!
        val id = idUnder(secret)

        // Nothing in a grant change calls into this class at all; the closest
        // a test can come is asserting that a reload after one is unchanged.
        val afterRegrant = NotificationSecret.loadOrCreate(store, metadata)!!
        assertEquals(id, idUnder(afterRegrant))
        assertEquals(1, store.creations)
    }

    // -- the secret never escapes --------------------------------------------

    /**
     * There is no accessor that returns the key material, which is what makes
     * "never logged, never transmitted" structural rather than a promise.
     */
    @Test
    fun `the secret exposes no way to read its bytes`() {
        val exposed = NotificationSecret::class.java.methods
            .filter { it.declaringClass == NotificationSecret::class.java }
            .map { it.name }
            .toSet()
        assertTrue("newMac" in exposed)
        assertFalse("bytes" in exposed)
        assertFalse("getBytes" in exposed)
        assertFalse("getKey" in exposed)
        assertFalse("getEncoded" in exposed)
    }

    @Test
    fun `the rendering carries no key material`() {
        val secret = NotificationSecret.loadOrCreate(FakeStore(), FakeMetadata())!!
        val rendered = secret.toString()
        assertEquals("NotificationSecret(state=CREATED, generation=1)", rendered)
    }

    /** Two `Mac` instances from one secret are independent and agree. */
    @Test
    fun `each derivation gets its own mac`() {
        val secret = NotificationSecret.loadOrCreate(FakeStore(), FakeMetadata())!!
        val first = secret.newMac()
        val second = secret.newMac()
        assertTrue(first !== second)
        assertArrayEquals(
            NotificationIdentity.derive(first, platformKey),
            NotificationIdentity.derive(second, platformKey),
        )
    }
}
