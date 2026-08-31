package io.github.yurisismotto.anyflow

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.github.yurisismotto.anyflow.capability.ClipboardCapability
import io.github.yurisismotto.anyflow.clipboard.ClipboardPolicy
import io.github.yurisismotto.anyflow.clipboard.ClipboardSync
import io.github.yurisismotto.anyflow.clipboard.ClipboardTarget
import io.github.yurisismotto.anyflow.clipboard.ClipboardText
import io.github.yurisismotto.anyflow.identity.Fingerprint
import io.github.yurisismotto.anyflow.store.TrustStore
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

/**
 * CLIP-SEC-15 on Android: what does and does not survive on disk.
 *
 * Instrumented rather than JVM because it reads the **real** trust-store file
 * from the app's real `filesDir`, using the platform's own `org.json`. A JVM
 * test with a stubbed `Context` would be asserting against a file this app
 * never writes.
 *
 * Two claims are made here, and they pull in opposite directions on purpose:
 * clipboard *content* must not be findable anywhere in the app's private
 * storage, and clipboard *policy* must survive a restart. A store that
 * persisted neither would pass a naive "no content" test while quietly losing
 * the person's settings.
 */
@RunWith(AndroidJUnit4::class)
class ClipboardPersistenceTest {

    private val context = InstrumentationRegistry.getInstrumentation().targetContext

    /** A string that appears nowhere else in the app or its resources. */
    private val canary = "CANARY-ANDROID-PERSIST-${System.nanoTime()}"

    private fun fingerprint(byte: Byte) = Fingerprint(ByteArray(32) { byte })

    /** Every byte of the app's private storage, as text. */
    private fun everythingOnDisk(): List<Pair<String, String>> =
        context.filesDir.walkTopDown()
            .plus(context.cacheDir.walkTopDown())
            .plus(
                context.applicationInfo.dataDir
                    ?.let { java.io.File(it, "shared_prefs").walkTopDown() }
                    ?: emptySequence(),
            )
            .filter { it.isFile }
            .map { it.absolutePath to runCatching { it.readText() }.getOrDefault("") }
            .toList()

    @Test
    fun clipboard_content_is_never_written_to_the_trust_store_or_anywhere_else() = runBlocking {
        val store = TrustStore(context)
        val peer = fingerprint(0x5c)

        store.addPeer(
            TrustStore.TrustedPeer(
                deviceId = "persistence-test-device",
                deviceName = "Test Computer",
                fingerprint = peer,
                pairedAtUnix = 1_700_000_000,
                grantedCapabilities = setOf(ClipboardCapability.ID),
                addresses = emptyList(),
            ),
        )

        // A clip that is applied, and a clip that is held — the two states in
        // which clipboard text exists at all.
        val clipboard = RecordingTarget()
        val sync = ClipboardSync(
            systemClipboard = clipboard,
            authorizer = { store.clipboardPolicyFor(it) },
            localDeviceId = store.deviceId,
        )

        // auto-receive off: held in memory.
        sync.handleControl(
            peer,
            "desktop",
            "Test Computer",
            updatePayload(canary),
        )
        assertEquals(1, sync.pendingClips.value.size)

        // Then applied.
        store.setClipboardPolicy(peer, ClipboardPolicy(autoReceive = true))
        sync.applyPending(peer)

        // Force a store write, so the assertion is against a file that has
        // actually been rewritten since the clip existed.
        store.setClipboardPolicy(
            peer,
            ClipboardPolicy(allowSend = false, autoReceive = true),
        )

        val files = everythingOnDisk()
        assertTrue("no files were inspected; the audit proved nothing", files.isNotEmpty())
        for ((path, text) in files) {
            assertFalse("$path contains clipboard content", text.contains(canary))
        }

        // And the policy *is* there: settings persist, content does not.
        val reloaded = TrustStore(context).peer(peer)
        assertFalse("the policy must survive", reloaded!!.clipboardPolicy.allowSend)
        assertTrue(reloaded.clipboardPolicy.autoReceive)
        assertTrue(reloaded.allows(ClipboardCapability.ID))

        // Clean up so a later run starts from a known state.
        TrustStore(context).removePeer(peer)
    }

    @Test
    fun a_forgotten_computer_leaves_no_clipboard_policy_behind() = runBlocking {
        val store = TrustStore(context)
        val peer = fingerprint(0x6d)
        store.addPeer(
            TrustStore.TrustedPeer(
                deviceId = "forget-test-device",
                deviceName = "Temporary",
                fingerprint = peer,
                pairedAtUnix = 1_700_000_000,
                grantedCapabilities = setOf(ClipboardCapability.ID),
                addresses = emptyList(),
                clipboardPolicy = ClipboardPolicy(autoReceive = true, autoSend = true),
            ),
        )
        assertTrue(store.clipboardPolicyFor(peer).mayAutoReceive())

        store.removePeer(peer)

        // Forgotten means denied, not "denied unless the record comes back".
        assertEquals(ClipboardPolicy.DENIED, TrustStore(context).clipboardPolicyFor(peer))
    }

    @Test
    fun a_peer_without_the_grant_is_denied_whatever_its_stored_policy_says() = runBlocking {
        val store = TrustStore(context)
        val peer = fingerprint(0x7e)
        store.addPeer(
            TrustStore.TrustedPeer(
                deviceId = "ungranted-device",
                deviceName = "Ungranted",
                fingerprint = peer,
                pairedAtUnix = 1_700_000_000,
                // Everything the policy can express, and no grant.
                grantedCapabilities = emptySet(),
                addresses = emptyList(),
                clipboardPolicy = ClipboardPolicy(
                    allowSend = true,
                    allowReceive = true,
                    autoSend = true,
                    autoReceive = true,
                ),
            ),
        )

        assertEquals(
            "the grant gates the policy, not the other way round",
            ClipboardPolicy.DENIED,
            store.clipboardPolicyFor(peer),
        )

        store.removePeer(peer)
    }

    /** A clipboard that never touches the real one. */
    private class RecordingTarget : ClipboardTarget {
        var current: ClipboardTarget.ReadClip? = null

        override fun read(): Result<ClipboardTarget.ReadClip> =
            current?.let { Result.success(it) }
                ?: Result.failure(IllegalStateException("empty"))

        override fun write(
            text: ClipboardText,
            sensitive: Boolean,
            label: String,
        ): ClipboardTarget.WriteResult {
            current = ClipboardTarget.ReadClip(text, sensitive)
            return ClipboardTarget.WriteResult.Applied
        }
    }

    private fun updatePayload(text: String) =
        io.github.yurisismotto.anyflow.proto.capabilities.ClipboardControl.newBuilder()
            .setUpdate(
                io.github.yurisismotto.anyflow.proto.capabilities.ClipboardUpdate.newBuilder()
                    .setEventId(
                        com.google.protobuf.ByteString.copyFrom(ByteArray(16) { 0x42 }),
                    )
                    .setOriginDeviceId("desktop")
                    .setTextUtf8(text)
                    .setContentHash(
                        com.google.protobuf.ByteString.copyFrom(
                            ClipboardText.contentHash(text),
                        ),
                    )
                    .setSensitiveHint(false)
                    .setTimestampUnixMs(1_700_000_000_000),
            )
            .build()
            .toByteString()
}
