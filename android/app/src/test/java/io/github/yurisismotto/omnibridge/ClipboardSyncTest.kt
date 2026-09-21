package io.github.yurisismotto.omnibridge

import com.google.protobuf.ByteString
import io.github.yurisismotto.omnibridge.clipboard.ClipboardLimits
import io.github.yurisismotto.omnibridge.clipboard.ClipboardPolicy
import io.github.yurisismotto.omnibridge.clipboard.ClipboardSendFailed
import io.github.yurisismotto.omnibridge.clipboard.ClipboardSync
import io.github.yurisismotto.omnibridge.clipboard.ClipboardTarget
import io.github.yurisismotto.omnibridge.clipboard.ClipboardText
import io.github.yurisismotto.omnibridge.clipboard.Clock
import io.github.yurisismotto.omnibridge.identity.Fingerprint
import io.github.yurisismotto.omnibridge.proto.capabilities.ClipboardControl
import io.github.yurisismotto.omnibridge.proto.capabilities.ClipboardOutcome
import io.github.yurisismotto.omnibridge.proto.capabilities.ClipboardResult
import io.github.yurisismotto.omnibridge.proto.capabilities.ClipboardUpdate
import java.security.SecureRandom
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The clipboard decision logic, on the JVM.
 *
 * The fake below implements [ClipboardTarget] and nothing else: it stands in
 * for the *storage* so that policy, de-duplication, loop suppression and the
 * sensitive-clip rule can be tested exhaustively. It makes no claim about how
 * Android's real `ClipboardManager` behaves — `ClipboardInstrumentedTest`
 * does that, on a device, because a fake that agreed with our assumptions
 * would prove only that we are self-consistent.
 */
class ClipboardSyncTest {

    // -----------------------------------------------------------------------
    // Harness
    // -----------------------------------------------------------------------

    private class FakeClock(var now: Long = 0) : Clock {
        override fun nowMillis(): Long = now
    }

    /** An in-memory clipboard that records what it was asked to store. */
    private class FakeClipboard : ClipboardTarget {
        var current: ClipboardTarget.ReadClip? = null
        val writes = mutableListOf<Pair<String, Boolean>>()
        var failWrites = false
        var readFailure: ClipboardTarget.ReadFailure? = null

        override fun read(): Result<ClipboardTarget.ReadClip> {
            readFailure?.let { return Result.failure(ClipboardReadFailedForTest(it)) }
            val clip = current
                ?: return Result.failure(
                    ClipboardReadFailedForTest(ClipboardTarget.ReadFailure.Empty),
                )
            return Result.success(clip)
        }

        override fun write(
            text: ClipboardText,
            sensitive: Boolean,
            label: String,
        ): ClipboardTarget.WriteResult {
            if (failWrites) return ClipboardTarget.WriteResult.Failed("test")
            writes += text.text to sensitive
            current = ClipboardTarget.ReadClip(text, sensitive)
            return ClipboardTarget.WriteResult.Applied
        }

        /** Simulates a person copying, with the platform's sensitivity hint. */
        fun userCopies(text: String, sensitive: Boolean = false) {
            current = ClipboardTarget.ReadClip(ClipboardText.validate(text).getOrThrow(), sensitive)
        }
    }

    /** The production type is internal to the clipboard package. */
    private class ClipboardReadFailedForTest(val failure: ClipboardTarget.ReadFailure) :
        Exception(failure.describe())

    private class FakeAuthorizer : ClipboardSync.Authorizer {
        private val policies = mutableMapOf<String, ClipboardPolicy>()

        fun grant(peer: Fingerprint, policy: ClipboardPolicy = ClipboardPolicy()) {
            policies[peer.toHex()] = policy
        }

        fun revoke(peer: Fingerprint) {
            policies.remove(peer.toHex())
        }

        // Unknown means denied — the same rule the trust store follows, so a
        // test cannot pass by having a more permissive default than production.
        override fun policyFor(peer: Fingerprint): ClipboardPolicy =
            policies[peer.toHex()] ?: ClipboardPolicy.DENIED
    }

    private fun fp(byte: Byte) = Fingerprint(ByteArray(32) { byte })

    private fun eventId(seed: Byte) = ByteArray(16) { seed }

    private fun update(
        eventId: ByteArray,
        origin: String = "desktop",
        text: String,
        sensitive: Boolean = false,
        hash: ByteArray? = null,
    ): ByteString = ClipboardControl.newBuilder()
        .setUpdate(
            ClipboardUpdate.newBuilder()
                .setEventId(ByteString.copyFrom(eventId))
                .setOriginDeviceId(origin)
                .setTextUtf8(text)
                .setContentHash(ByteString.copyFrom(hash ?: ClipboardText.contentHash(text)))
                .setSensitiveHint(sensitive)
                .setTimestampUnixMs(1_700_000_000_000),
        )
        .build()
        .toByteString()

    private class Rig {
        val clipboard = FakeClipboard()
        val authorizer = FakeAuthorizer()
        val clock = FakeClock()
        val sent = mutableListOf<ByteString>()

        /**
         * The envelope message ids the fake session minted, in send order.
         *
         * The real `PeerConnection` answers one per frame; a peer refusing the
         * capability echoes it back as `Envelope.correlation_id`. Recording
         * them here is what lets a test drive that path.
         */
        val messageIds = mutableListOf<ByteString>()
        val sync = ClipboardSync(
            systemClipboard = clipboard,
            authorizer = authorizer,
            localDeviceId = "phone-device-id",
            clock = clock,
            random = SecureRandom(),
        )
    }

    private suspend fun Rig.connect(peer: Fingerprint) {
        sync.attachSession(peer) { payload ->
            sent += payload
            // Unique per frame, as the envelope factory's are.
            val id = ByteString.copyFrom(ByteArray(16).also { SecureRandom().nextBytes(it) })
            messageIds += id
            id
        }
    }

    private suspend fun Rig.handle(
        peer: Fingerprint,
        payload: ByteString,
        name: String = "Fedora",
    ): ClipboardOutcome? {
        val reply = sync.handleControl(peer, "desktop", name, payload) ?: return null
        return ClipboardControl.parseFrom(reply).result.outcome
    }

    private fun ByteString.asUpdate(): ClipboardUpdate =
        ClipboardControl.parseFrom(this).update

    // -----------------------------------------------------------------------
    // Authorization
    // -----------------------------------------------------------------------

    @Test
    fun `an ungranted computer is refused and its clip is not even held`() = runTest {
        val rig = Rig()
        val peer = fp(1)

        val outcome = rig.handle(peer, update(eventId(1), text = "secret"))

        assertEquals(ClipboardOutcome.CLIPBOARD_OUTCOME_NOT_AUTHORIZED, outcome)
        assertNull(rig.clipboard.current)
        assertTrue(rig.clipboard.writes.isEmpty())
        assertTrue(rig.sync.pendingClips.value.isEmpty())
    }

    @Test
    fun `a revoked computer is refused even with a permissive stored policy`() = runTest {
        val rig = Rig()
        val peer = fp(2)
        rig.authorizer.grant(
            peer,
            ClipboardPolicy(
                allowSend = true,
                allowReceive = true,
                autoSend = true,
                autoReceive = true,
            ),
        )
        rig.authorizer.revoke(peer)

        assertEquals(
            ClipboardOutcome.CLIPBOARD_OUTCOME_NOT_AUTHORIZED,
            rig.handle(peer, update(eventId(2), text = "after revocation")),
        )
        assertNull(rig.clipboard.current)
    }

    @Test
    fun `policy follows the pinned identity not the claimed device id`() = runTest {
        val rig = Rig()
        rig.authorizer.grant(fp(3))

        // A different pinned identity, claiming the trusted one's device id.
        val outcome = rig.handle(fp(4), update(eventId(3), origin = "desktop", text = "spoofed"))

        assertEquals(ClipboardOutcome.CLIPBOARD_OUTCOME_NOT_AUTHORIZED, outcome)
    }

    @Test
    fun `an equal but distinct fingerprint resolves to the same peer`() = runTest {
        // Guards the value-class trap: `Fingerprint` wraps a ByteArray, whose
        // equality is by reference. If any map here were keyed by the type
        // rather than by its hex, this lookup would miss and the clip would
        // be refused as if the computer were unknown.
        val rig = Rig()
        val granted = fp(5)
        val sameBytes = Fingerprint(granted.bytes.copyOf())
        rig.authorizer.grant(granted, ClipboardPolicy(autoReceive = true))
        rig.connect(granted)

        assertEquals(
            ClipboardOutcome.CLIPBOARD_OUTCOME_APPLIED,
            rig.handle(sameBytes, update(eventId(5), text = "same peer")),
        )
        assertTrue(rig.sync.isConnected(sameBytes))
    }

    // -----------------------------------------------------------------------
    // Policy directions
    // -----------------------------------------------------------------------

    @Test
    fun `receive off drops the clip without holding it`() = runTest {
        val rig = Rig()
        val peer = fp(6)
        rig.authorizer.grant(peer, ClipboardPolicy(allowReceive = false))

        assertEquals(
            ClipboardOutcome.CLIPBOARD_OUTCOME_REJECTED_POLICY,
            rig.handle(peer, update(eventId(6), text = "unwanted")),
        )
        assertNull(rig.clipboard.current)
        assertTrue(
            "a refused clip must not be retained in memory",
            rig.sync.pendingClips.value.isEmpty(),
        )
    }

    @Test
    fun `auto receive off holds the clip instead of applying it`() = runTest {
        val rig = Rig()
        val peer = fp(7)
        rig.authorizer.grant(peer) // autoReceive off by default

        assertEquals(
            ClipboardOutcome.CLIPBOARD_OUTCOME_PENDING_USER,
            rig.handle(peer, update(eventId(7), text = "held for a human")),
        )
        assertNull("a pending clip must not touch the clipboard", rig.clipboard.current)

        val pending = rig.sync.pendingClips.value
        assertEquals(1, pending.size)
        assertEquals("held for a human".length, pending[0].bytes)
        assertEquals(8, pending[0].hashPrefix.length)

        assertEquals("held for a human".length, rig.sync.applyPending(peer).getOrThrow())
        assertEquals("held for a human", rig.clipboard.current?.text?.text)
        assertTrue(rig.sync.pendingClips.value.isEmpty())
    }

    @Test
    fun `a held clip cannot be applied after the grant is withdrawn`() = runTest {
        val rig = Rig()
        val peer = fp(8)
        rig.authorizer.grant(peer)
        rig.handle(peer, update(eventId(8), text = "stale permission"))

        rig.authorizer.revoke(peer)

        assertTrue(
            "the grant is re-checked at apply time, not only on arrival",
            rig.sync.applyPending(peer).isFailure,
        )
        assertNull(rig.clipboard.current)
    }

    @Test
    fun `a held clip expires on its own`() = runTest {
        val rig = Rig()
        val peer = fp(9)
        rig.authorizer.grant(peer)
        rig.handle(peer, update(eventId(9), text = "forgotten"))
        assertEquals(1, rig.sync.pendingClips.value.size)

        rig.clock.now += ClipboardLimits.PENDING_CLIP_TTL_MS + 1
        // Publishing happens on the next mutation; force one.
        rig.sync.dismissPending(fp(99))

        assertTrue(
            "clipboard content must not sit in memory indefinitely",
            rig.sync.pendingClips.value.isEmpty(),
        )
        assertTrue(rig.sync.applyPending(peer).isFailure)
    }

    @Test
    fun `a pending clip does not survive the session that delivered it`() = runTest {
        val rig = Rig()
        val peer = fp(10)
        rig.authorizer.grant(peer)
        rig.connect(peer)
        rig.handle(peer, update(eventId(10), text = "in memory only"))
        assertEquals(1, rig.sync.pendingClips.value.size)

        rig.sync.detachSession(peer)

        assertTrue(rig.sync.pendingClips.value.isEmpty())
    }

    // -----------------------------------------------------------------------
    // De-duplication and replay
    // -----------------------------------------------------------------------

    @Test
    fun `a replayed event is idempotent`() = runTest {
        val rig = Rig()
        val peer = fp(11)
        rig.authorizer.grant(peer, ClipboardPolicy(autoReceive = true))

        assertEquals(
            ClipboardOutcome.CLIPBOARD_OUTCOME_APPLIED,
            rig.handle(peer, update(eventId(11), text = "applied once")),
        )
        assertEquals(
            ClipboardOutcome.CLIPBOARD_OUTCOME_DUPLICATE,
            rig.handle(peer, update(eventId(11), text = "applied once")),
        )
        assertEquals(
            ClipboardOutcome.CLIPBOARD_OUTCOME_DUPLICATE,
            rig.handle(peer, update(eventId(11), text = "different text, same id")),
        )

        assertEquals(1, rig.clipboard.writes.size)
        assertEquals("applied once", rig.clipboard.writes[0].first)
    }

    @Test
    fun `an event id replayed through a second computer is still a duplicate`() = runTest {
        val rig = Rig()
        val a = fp(12)
        val b = fp(13)
        rig.authorizer.grant(a, ClipboardPolicy(autoReceive = true))
        rig.authorizer.grant(b, ClipboardPolicy(autoReceive = true))

        assertEquals(
            ClipboardOutcome.CLIPBOARD_OUTCOME_APPLIED,
            rig.handle(a, update(eventId(14), text = "one")),
        )
        assertEquals(
            "an id already handled must not be reusable by a different peer",
            ClipboardOutcome.CLIPBOARD_OUTCOME_DUPLICATE,
            rig.handle(b, update(eventId(14), text = "two")),
        )
        assertEquals(1, rig.clipboard.writes.size)
    }

    // -----------------------------------------------------------------------
    // Validation
    // -----------------------------------------------------------------------

    @Test
    fun `an oversized clip is refused and never truncated`() = runTest {
        val rig = Rig()
        val peer = fp(15)
        rig.authorizer.grant(peer, ClipboardPolicy(autoReceive = true))

        val huge = "A".repeat(ClipboardLimits.MAX_TEXT_BYTES + 1)
        assertEquals(
            ClipboardOutcome.CLIPBOARD_OUTCOME_TOO_LARGE,
            rig.handle(peer, update(eventId(16), text = huge)),
        )
        assertNull("not even a prefix may reach the clipboard", rig.clipboard.current)

        // The boundary itself is accepted, so the limit is a limit and not an
        // off-by-one.
        assertEquals(
            ClipboardOutcome.CLIPBOARD_OUTCOME_APPLIED,
            rig.handle(peer, update(eventId(17), text = "A".repeat(ClipboardLimits.MAX_TEXT_BYTES))),
        )
    }

    @Test
    fun `nul bearing text is refused`() = runTest {
        val rig = Rig()
        val peer = fp(18)
        rig.authorizer.grant(peer, ClipboardPolicy(autoReceive = true))

        assertEquals(
            ClipboardOutcome.CLIPBOARD_OUTCOME_INVALID_TEXT,
            rig.handle(peer, update(eventId(18), text = "abc\u0000def")),
        )
        assertNull(rig.clipboard.current)
    }

    @Test
    fun `a content hash that does not match is refused`() = runTest {
        val rig = Rig()
        val peer = fp(19)
        rig.authorizer.grant(peer, ClipboardPolicy(autoReceive = true))

        assertEquals(
            ClipboardOutcome.CLIPBOARD_OUTCOME_INVALID_TEXT,
            rig.handle(peer, update(eventId(19), text = "real text", hash = ByteArray(32))),
        )
        assertNull(rig.clipboard.current)

        // An absent hash is allowed: the field is optional and the hash is
        // not authentication, so a minimal peer still interoperates.
        assertEquals(
            ClipboardOutcome.CLIPBOARD_OUTCOME_APPLIED,
            rig.handle(peer, update(eventId(20), text = "no hash", hash = ByteArray(0))),
        )
    }

    @Test
    fun `a wrong length event id is refused without a reply`() = runTest {
        val rig = Rig()
        val peer = fp(21)
        rig.authorizer.grant(peer, ClipboardPolicy(autoReceive = true))

        for (bad in listOf(ByteArray(0), ByteArray(8), ByteArray(32))) {
            assertNull(
                "an unusable event id cannot be answered",
                rig.handle(peer, update(bad, text = "text")),
            )
        }
        assertNull(rig.clipboard.current)
    }

    @Test
    fun `malformed payloads are refused without crashing`() = runTest {
        val rig = Rig()
        val peer = fp(22)
        rig.authorizer.grant(peer, ClipboardPolicy(autoReceive = true))

        val malformed = listOf(
            byteArrayOf(-1, -1, -1, -1),
            byteArrayOf(0x0a, -1),
            byteArrayOf(0x08),
            ByteArray(64) { -1 },
        )
        for (bytes in malformed) {
            var threw = false
            try {
                rig.sync.handleControl(peer, "desktop", "Fedora", ByteString.copyFrom(bytes))
            } catch (e: IllegalArgumentException) {
                threw = true
            }
            assertTrue("a malformed payload must be reported, not accepted", threw)
        }
        assertNull(rig.clipboard.current)
    }

    @Test
    fun `an unknown body is accepted and ignored`() = runTest {
        val rig = Rig()
        val peer = fp(23)
        rig.authorizer.grant(peer, ClipboardPolicy(autoReceive = true))

        // Well-formed but empty, and a field number from a hypothetical
        // future: a newer peer must not break this one.
        for (bytes in listOf(ByteArray(0), byteArrayOf(-6, 0x06, 0x02, 0x01, 0x02))) {
            assertNull(
                rig.sync.handleControl(peer, "desktop", "Fedora", ByteString.copyFrom(bytes)),
            )
        }
        assertNull(rig.clipboard.current)
    }

    @Test
    fun `a result never produces a reply`() = runTest {
        val rig = Rig()
        val peer = fp(24)
        rig.authorizer.grant(peer)

        val result = ClipboardControl.newBuilder()
            .setResult(
                ClipboardResult.newBuilder()
                    .setEventId(ByteString.copyFrom(eventId(24)))
                    .setOutcome(ClipboardOutcome.CLIPBOARD_OUTCOME_APPLIED),
            )
            .build()
            .toByteString()

        assertNull(
            "answering an answer is how two correct peers build a loop",
            rig.sync.handleControl(peer, "desktop", "Fedora", result),
        )
        // Nothing is recorded: the result names an event id this device never
        // minted, so there is no send for it to be the verdict *of*. Before
        // the correlation existed this wrote "APPLIED" against the peer, which
        // is how a result for one clip could describe another.
        assertNull(
            "a result must not resolve a send that was never made",
            rig.sync.lastDelivery.value[peer.toHex()],
        )
    }

    // -----------------------------------------------------------------------
    // Sensitive handling
    // -----------------------------------------------------------------------

    @Test
    fun `a sensitive hint reaches the platform as a hint`() = runTest {
        val rig = Rig()
        val peer = fp(25)
        rig.authorizer.grant(peer, ClipboardPolicy(autoReceive = true))

        rig.handle(peer, update(eventId(25), text = "one-time code", sensitive = true))
        assertEquals(1, rig.clipboard.writes.size)
        assertTrue("the hint must be passed through", rig.clipboard.writes[0].second)

        // And it is per clip, not sticky.
        rig.handle(peer, update(eventId(26), text = "ordinary", sensitive = false))
        assertFalse(rig.clipboard.writes[1].second)
    }

    @Test
    fun `a sensitive clip is not sent until the person confirms`() = runTest {
        val rig = Rig()
        val peer = fp(27)
        rig.authorizer.grant(peer)
        rig.connect(peer)
        rig.clipboard.userCopies("hunter2", sensitive = true)

        val refused = rig.sync.sendCurrentClipboard(peer, confirmedSensitive = false)
        val failure = (refused.exceptionOrNull() as ClipboardSendFailed).failure
        assertTrue(
            "a sensitive clip must not leave on one tap",
            failure is ClipboardSync.SendFailure.NeedsConfirmation,
        )
        assertTrue("nothing may have been sent", rig.sent.isEmpty())

        // The failure names the size, never the text.
        assertFalse(failure.describe().contains("hunter2"))

        // With the confirmation, it goes — and the hint travels with it.
        assertEquals(
            7,
            rig.sync.sendCurrentClipboard(peer, confirmedSensitive = true).getOrThrow().bytes,
        )
        assertEquals(1, rig.sent.size)
        assertTrue(rig.sent[0].asUpdate().sensitiveHint)
    }

    @Test
    fun `an ordinary clip needs no confirmation`() = runTest {
        val rig = Rig()
        val peer = fp(28)
        rig.authorizer.grant(peer)
        rig.connect(peer)
        rig.clipboard.userCopies("ordinary text", sensitive = false)

        assertEquals(13, rig.sync.sendCurrentClipboard(peer).getOrThrow().bytes)
        assertEquals(1, rig.sent.size)
        assertFalse(rig.sent[0].asUpdate().sensitiveHint)
    }

    // -----------------------------------------------------------------------
    // Sending
    // -----------------------------------------------------------------------

    @Test
    fun `send off refuses even a manual send`() = runTest {
        val rig = Rig()
        val peer = fp(29)
        rig.authorizer.grant(peer, ClipboardPolicy(allowSend = false))
        rig.connect(peer)
        rig.clipboard.userCopies("nope")

        assertTrue(rig.sync.sendCurrentClipboard(peer).isFailure)
        assertTrue(rig.sent.isEmpty())
    }

    @Test
    fun `a send to a disconnected computer fails cleanly`() = runTest {
        val rig = Rig()
        val peer = fp(30)
        rig.authorizer.grant(peer)
        rig.clipboard.userCopies("text")

        val failure = (rig.sync.sendCurrentClipboard(peer).exceptionOrNull() as ClipboardSendFailed)
        assertEquals(ClipboardSync.SendFailure.NotConnected, failure.failure)
    }

    @Test
    fun `a refused read is reported as such rather than as an empty clipboard`() = runTest {
        val rig = Rig()
        val peer = fp(31)
        rig.authorizer.grant(peer)
        rig.connect(peer)
        rig.clipboard.readFailure = ClipboardTarget.ReadFailure.NotAllowed

        val failure = (rig.sync.sendCurrentClipboard(peer).exceptionOrNull() as ClipboardSendFailed)
        val reason = failure.failure as ClipboardSync.SendFailure.CannotRead
        assertEquals(ClipboardTarget.ReadFailure.NotAllowed, reason.failure)
        // The message tells the person what to do about it.
        assertTrue(failure.message!!.contains("Open OmniBridge"))
    }

    @Test
    fun `an outbound update carries a fresh 16 byte event id and this device id`() = runTest {
        val rig = Rig()
        val peer = fp(32)
        rig.authorizer.grant(peer)
        rig.connect(peer)

        val seen = mutableSetOf<String>()
        repeat(20) { i ->
            rig.clipboard.userCopies("clip $i")
            rig.sync.sendCurrentClipboard(peer).getOrThrow()
        }

        assertEquals(20, rig.sent.size)
        for (payload in rig.sent) {
            val u = payload.asUpdate()
            assertEquals(ClipboardLimits.EVENT_ID_LENGTH, u.eventId.size())
            assertEquals("phone-device-id", u.originDeviceId)
            assertTrue(
                "event ids must be unique",
                seen.add(u.eventId.toByteArray().joinToString("") { "%02x".format(it) }),
            )
            assertTrue(
                u.contentHash.toByteArray().contentEquals(ClipboardText.contentHash(u.textUtf8)),
            )
        }
    }

    // -----------------------------------------------------------------------
    // Loop suppression and no relay
    // -----------------------------------------------------------------------

    @Test
    fun `applying a remote clip does not produce an outbound update`() = runTest {
        val rig = Rig()
        val peer = fp(33)
        rig.authorizer.grant(
            peer,
            ClipboardPolicy(
                allowSend = true,
                allowReceive = true,
                autoSend = true,
                autoReceive = true,
            ),
        )
        rig.connect(peer)

        assertEquals(
            ClipboardOutcome.CLIPBOARD_OUTCOME_APPLIED,
            rig.handle(peer, update(eventId(33), text = "round trip")),
        )

        // The reply is returned to the capability, not queued here; what must
        // never appear is an outbound *update*.
        assertTrue(
            "handling an update must not push anything outbound",
            rig.sent.isEmpty(),
        )
    }

    @Test
    fun `a clip from one computer is never forwarded to another`() = runTest {
        // The phone is connected to two computers, both fully automatic. A
        // clip from A must not reach B — and on this platform it cannot,
        // because nothing but an explicit person-initiated send ever
        // transmits, and there is no code path from an inbound update to an
        // outbound one.
        val rig = Rig()
        val a = fp(34)
        val b = fp(35)
        val wideOpen = ClipboardPolicy(
            allowSend = true,
            allowReceive = true,
            autoSend = true,
            autoReceive = true,
        )
        rig.authorizer.grant(a, wideOpen)
        rig.authorizer.grant(b, wideOpen)
        rig.connect(a)
        rig.connect(b)

        rig.handle(a, update(eventId(36), origin = "desktop-a", text = "a secret from A"))

        assertEquals("a secret from A", rig.clipboard.current?.text?.text)
        assertTrue("nothing may have been relayed", rig.sent.isEmpty())
    }

    @Test
    fun `a failed apply releases the suppression entry`() = runTest {
        val rig = Rig()
        val peer = fp(37)
        rig.authorizer.grant(peer, ClipboardPolicy(autoReceive = true))
        rig.connect(peer)
        rig.clipboard.failWrites = true

        assertEquals(
            ClipboardOutcome.CLIPBOARD_OUTCOME_FAILED,
            rig.handle(peer, update(eventId(37), text = "never written")),
        )

        // The write never happened, so nothing should be suppressed: a person
        // copying that same text next must still be able to send it.
        rig.clipboard.failWrites = false
        rig.clipboard.userCopies("never written")
        assertEquals(
            "never written".length,
            rig.sync.sendCurrentClipboard(peer).getOrThrow().bytes,
        )
        assertEquals(1, rig.sent.size)
    }

    @Test
    fun `the caches stay bounded under a flood`() = runTest {
        val rig = Rig()
        val peer = fp(38)
        rig.authorizer.grant(peer, ClipboardPolicy(autoReceive = true))

        val flood = ClipboardLimits.EVENT_CACHE_ENTRIES * 5
        for (i in 0 until flood) {
            val id = ByteArray(16)
            id[0] = (i and 0xff).toByte()
            id[1] = ((i shr 8) and 0xff).toByte()
            rig.handle(peer, update(id, text = "clip number $i"))
        }

        val (events, suppression) = rig.sync.cacheSizes()
        assertTrue("event cache grew to $events", events <= ClipboardLimits.EVENT_CACHE_ENTRIES)
        assertTrue(
            "suppression cache grew to $suppression",
            suppression <= ClipboardLimits.SUPPRESSION_ENTRIES,
        )
    }

    @Test
    fun `no inbound message can widen local policy`() = runTest {
        val rig = Rig()
        val peer = fp(39)
        val restrictive = ClipboardPolicy(
            allowSend = true,
            allowReceive = true,
            autoSend = false,
            autoReceive = false,
        )
        rig.authorizer.grant(peer, restrictive)

        // Everything a peer can put on the wire. None of them is a policy
        // write, because the schema has no such message — this pins that the
        // absence is real.
        rig.handle(peer, update(eventId(40), text = "text", sensitive = true))
        rig.sync.handleControl(
            peer,
            "desktop",
            "Fedora",
            ClipboardControl.newBuilder()
                .setResult(
                    ClipboardResult.newBuilder()
                        .setEventId(ByteString.copyFrom(eventId(40)))
                        .setOutcome(ClipboardOutcome.CLIPBOARD_OUTCOME_APPLIED),
                )
                .build()
                .toByteString(),
        )

        assertEquals(restrictive, rig.authorizer.policyFor(peer))
    }
}
