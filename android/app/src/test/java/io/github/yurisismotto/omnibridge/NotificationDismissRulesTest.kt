package io.github.yurisismotto.omnibridge

import com.google.protobuf.ByteString
import io.github.yurisismotto.omnibridge.notifications.EchoSuppression
import io.github.yurisismotto.omnibridge.notifications.LockPolicy
import io.github.yurisismotto.omnibridge.notifications.NotificationDismissRules
import io.github.yurisismotto.omnibridge.notifications.NotificationDismissRules.DismissRefusal
import io.github.yurisismotto.omnibridge.notifications.NotificationMapping
import io.github.yurisismotto.omnibridge.notifications.NotificationPolicy
import io.github.yurisismotto.omnibridge.notifications.PlatformNotification
import io.github.yurisismotto.omnibridge.proto.capabilities.DismissRequest
import io.github.yurisismotto.omnibridge.proto.capabilities.NotificationOutcome
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The decision that guards the only remote effect in `notifications.v1`.
 *
 * `DismissRequest` is the single message that travels sink → source and
 * changes something on this phone, so the rule that admits one is written as a
 * pure function and enumerated here rather than inspected on a tablet. Every
 * refusal below is a state a real device reaches, and the *order* is asserted
 * as well as the outcomes: a refusal that fired later than it should would let
 * an ungranted peer learn something from a lookup it was never entitled to
 * reach.
 */
class NotificationDismissRulesTest {

    private val localDeviceId = "0123456789abcdef0123456789abcdef"
    private val otherDeviceId = "ffffffffffffffffffffffffffffffff"

    private val permissive = NotificationPolicy(
        allowedApps = setOf("example.fixture.app"),
        allowDismissSync = true,
    )

    private fun request(
        id: ByteArray = ByteArray(16) { it.toByte() },
        origin: String = localDeviceId,
    ): DismissRequest = DismissRequest.newBuilder()
        .setNotificationId(ByteString.copyFrom(id))
        .setOriginDeviceId(origin)
        .build()

    private fun screen(
        request: DismissRequest = request(),
        policy: NotificationPolicy = permissive,
        canDismiss: Boolean = true,
    ) = NotificationDismissRules.screen(request, localDeviceId, policy, canDismiss)

    private fun refusal(verdict: NotificationDismissRules.Verdict): DismissRefusal =
        (verdict as NotificationDismissRules.Verdict.Refuse).reason

    private fun outcome(verdict: NotificationDismissRules.Verdict): NotificationOutcome =
        (verdict as NotificationDismissRules.Verdict.Refuse).outcome

    // -- the one that passes -------------------------------------------------

    @Test
    fun `a well formed request from a permitted peer proceeds`() {
        assertEquals(NotificationDismissRules.Verdict.Proceed, screen())
    }

    // -- identifiers ---------------------------------------------------------

    /**
     * A bad-width identifier is refused **and not answered**: a
     * `NotificationResult` echoes the id, so a malformed one leaves nothing
     * coherent to correlate a reply with (ADR-0016 §9).
     */
    @Test
    fun `a bad width identifier is unanswerable`() {
        for (width in listOf(0, 1, 8, 15, 17, 32, 64)) {
            assertEquals(
                "$width bytes must be refused without an answer",
                NotificationDismissRules.Verdict.Unanswerable,
                screen(request = request(id = ByteArray(width))),
            )
        }
    }

    /**
     * And it is refused **first**, before the grant.
     *
     * Not because an ungranted peer deserves the better error — it deserves
     * none — but because the width decides whether an answer is possible at
     * all, and that question has to be settled before an answer is composed.
     */
    @Test
    fun `the width is decided before anything else`() {
        assertEquals(
            NotificationDismissRules.Verdict.Unanswerable,
            screen(
                request = request(id = ByteArray(4), origin = "nonsense"),
                policy = NotificationPolicy.DENIED,
                canDismiss = false,
            ),
        )
    }

    @Test
    fun `an origin device id must be exactly thirty two lowercase hex`() {
        val bad = listOf(
            "",
            "0123456789abcdef",
            "0123456789abcdef0123456789abcdef0",
            "0123456789ABCDEF0123456789ABCDEF",
            "0123456789abcdef0123456789abcdeg",
            " 123456789abcdef0123456789abcdef",
        )
        for (value in bad) {
            val verdict = screen(request = request(origin = value))
            assertEquals("'$value'", DismissRefusal.MALFORMED_ORIGIN, refusal(verdict))
            assertEquals(NotificationOutcome.NOTIFICATION_OUTCOME_INVALID, outcome(verdict))
        }
    }

    /**
     * A peer cannot dismiss a third device's notification through us.
     *
     * Refused as `INVALID` rather than as unknown, because it is a statement
     * about the *message* — the sender addressed the wrong device — and not
     * about any notification this phone may or may not hold.
     */
    @Test
    fun `a request naming another device is refused`() {
        val verdict = screen(request = request(origin = otherDeviceId))
        assertEquals(DismissRefusal.NOT_THE_ORIGIN, refusal(verdict))
        assertEquals(NotificationOutcome.NOTIFICATION_OUTCOME_INVALID, outcome(verdict))
    }

    // -- the three permissions ----------------------------------------------

    @Test
    fun `an ungranted peer is not authorized`() {
        val verdict = screen(policy = NotificationPolicy.DENIED)
        assertEquals(DismissRefusal.NOT_GRANTED, refusal(verdict))
        assertEquals(
            NotificationOutcome.NOTIFICATION_OUTCOME_NOT_AUTHORIZED,
            outcome(verdict),
        )
    }

    @Test
    fun `a phone that cannot act refuses with rejected role`() {
        val verdict = screen(canDismiss = false)
        assertEquals(DismissRefusal.NOT_A_DISMISS_TARGET, refusal(verdict))
        assertEquals(
            NotificationOutcome.NOTIFICATION_OUTCOME_REJECTED_ROLE,
            outcome(verdict),
        )
    }

    /** The default, and the most common refusal there will ever be. */
    @Test
    fun `dismiss sync off is a policy refusal`() {
        val verdict = screen(
            policy = NotificationPolicy(allowedApps = setOf("example.fixture.app")),
        )
        assertEquals(DismissRefusal.DISMISS_SYNC_OFF, refusal(verdict))
        assertEquals(
            NotificationOutcome.NOTIFICATION_OUTCOME_REJECTED_POLICY,
            outcome(verdict),
        )
    }

    @Test
    fun `mirroring off defeats a stale dismiss sync flag`() {
        val verdict = screen(
            policy = NotificationPolicy(allowMirror = false, allowDismissSync = true),
        )
        assertEquals(DismissRefusal.DISMISS_SYNC_OFF, refusal(verdict))
    }

    /**
     * The order that keeps a dismiss from becoming a lookup oracle.
     *
     * A peer that fails the grant must be told it failed the grant, not that
     * its origin was wrong or its notification unknown — every earlier refusal
     * is a fact the peer already knows about itself, and the later ones are
     * facts about this phone.
     */
    @Test
    fun `an earlier refusal always wins over a later one`() {
        // Ungranted AND wrong origin AND policy off AND cannot act.
        val everything = screen(
            request = request(origin = otherDeviceId),
            policy = NotificationPolicy.DENIED,
            canDismiss = false,
        )
        assertEquals(DismissRefusal.NOT_GRANTED, refusal(everything))

        // Granted, cannot act, policy off, wrong origin: the role wins.
        val roleFirst = screen(
            request = request(origin = otherDeviceId),
            policy = NotificationPolicy(allowedApps = setOf("a")),
            canDismiss = false,
        )
        assertEquals(DismissRefusal.NOT_A_DISMISS_TARGET, refusal(roleFirst))

        // Granted, can act, policy off, wrong origin: the policy wins, so a
        // peer with the setting off never learns whether it named this device.
        val policyFirst = screen(
            request = request(origin = otherDeviceId),
            policy = NotificationPolicy(allowedApps = setOf("a")),
        )
        assertEquals(DismissRefusal.DISMISS_SYNC_OFF, refusal(policyFirst))
    }

    /** Every refusal reason is safe to log: a name, and nothing about a user. */
    @Test
    fun `no refusal reason could name a notification`() {
        for (reason in DismissRefusal.entries) {
            val name = reason.name
            assertTrue(name.isNotEmpty())
            assertTrue(
                "$name looks like it could carry data",
                name.all { it.isUpperCase() || it == '_' },
            )
        }
    }

    // -- clearability, read from the live platform --------------------------

    private fun platform(
        clearable: Boolean = true,
        ongoing: Boolean = false,
    ) = PlatformNotification(
        platformKey = "0|example.fixture.app|1|null|10123",
        packageName = "example.fixture.app",
        secondaryProfile = false,
        postedAtUnixMs = 1_700_000_000_000L,
        ongoing = ongoing,
        clearable = clearable,
        visibility = NotificationMapping.VISIBILITY_PRIVATE,
        androidImportance = NotificationMapping.ANDROID_IMPORTANCE_DEFAULT,
        category = "msg",
        groupKey = null,
        groupSummary = false,
        title = "FIXTURE-TITLE-CANARY",
        body = "FIXTURE-BODY-CANARY",
        hasProgress = false,
        progressCurrent = 0,
        progressMax = 0,
        progressIndeterminate = false,
    )

    @Test
    fun `a clearable notification may be removed`() {
        assertEquals(
            NotificationOutcome.NOTIFICATION_OUTCOME_REMOVED,
            NotificationDismissRules.decideClearable(platform()),
        )
    }

    @Test
    fun `a notification the platform no longer has converges as unknown`() {
        assertEquals(
            NotificationOutcome.NOTIFICATION_OUTCOME_UNKNOWN_NOTIFICATION,
            NotificationDismissRules.decideClearable(null),
        )
    }

    /**
     * Both flags are consulted, and not one standing in for the other.
     *
     * `isClearable()` is already false for an ongoing notification on every
     * Android OmniBridge supports, but they are separate platform concepts and a
     * rule that relies on one implying the other is a rule that breaks on the
     * release where it stops.
     */
    @Test
    fun `an ongoing or non clearable notification is never force cancelled`() {
        for (n in listOf(
            platform(clearable = false),
            platform(ongoing = true),
            platform(clearable = false, ongoing = true),
            // The case the re-check exists for: still clearable by the flag,
            // but the app has made it ongoing since the mirror went out.
            platform(clearable = true, ongoing = true),
        )) {
            assertEquals(
                NotificationOutcome.NOTIFICATION_OUTCOME_NOT_DISMISSIBLE,
                NotificationDismissRules.decideClearable(n),
            )
        }
    }
}

/**
 * The cache that stops a dismissal echoing a removal back to the computer that
 * asked for it.
 *
 * What is asserted here is mostly what it must **not** do: it must not swallow
 * a removal it was not armed for, must not survive its window, and must not
 * hold an entry past its single use. Each of those failures would strand a
 * mirror on a desktop that nothing could ever take off, which is worse than
 * the redundant message this exists to remove.
 */
class EchoSuppressionTest {

    private var clock = 0L
    private fun cache(capacity: Int = EchoSuppression.CAPACITY) =
        EchoSuppression(capacity = capacity, now = { clock })

    private val peerA = "aa".repeat(32)
    private val peerB = "bb".repeat(32)

    @Test
    fun `an armed entry names the peer that asked, once`() {
        val echo = cache()
        echo.arm("id-1", peerA)
        assertEquals(peerA, echo.consume("id-1"))
        assertNull("single use: the entry is gone", echo.consume("id-1"))
    }

    @Test
    fun `an id nothing armed suppresses nothing`() {
        val echo = cache()
        echo.arm("id-1", peerA)
        assertNull(echo.consume("id-2"))
        // And arming one does not consume the other.
        assertEquals(peerA, echo.consume("id-1"))
    }

    @Test
    fun `entries expire so a callback that never arrives swallows nothing`() {
        val echo = cache()
        echo.arm("id-1", peerA)
        clock += EchoSuppression.TTL_MS + 1
        assertNull(
            "a removal ten seconds later is a genuine one",
            echo.consume("id-1"),
        )
        assertEquals(0, echo.size())
    }

    @Test
    fun `an entry inside the window survives`() {
        val echo = cache()
        echo.arm("id-1", peerA)
        clock += EchoSuppression.TTL_MS - 1
        assertEquals(peerA, echo.consume("id-1"))
    }

    /** Expiry sweeps the old without touching the new. */
    @Test
    fun `expiry is ordered and stops at the first live entry`() {
        val echo = cache()
        echo.arm("old", peerA)
        clock += EchoSuppression.TTL_MS - 1
        echo.arm("new", peerB)
        clock += 2

        assertNull(echo.consume("old"))
        assertEquals(peerB, echo.consume("new"))
    }

    @Test
    fun `releasing an entry leaves nothing to swallow a later removal`() {
        val echo = cache()
        echo.arm("id-1", peerA)
        echo.release("id-1")
        assertNull(echo.consume("id-1"))
    }

    @Test
    fun `re-arming the same id replaces rather than duplicates`() {
        val echo = cache()
        echo.arm("id-1", peerA)
        echo.arm("id-1", peerB)
        assertEquals(1, echo.size())
        assertEquals(peerB, echo.consume("id-1"))
    }

    @Test
    fun `the cache is bounded and evicts the least recently armed`() {
        val echo = cache(capacity = 3)
        echo.arm("a", peerA)
        echo.arm("b", peerA)
        echo.arm("c", peerA)
        echo.arm("d", peerA)

        assertEquals(3, echo.size())
        assertNull("the oldest was evicted", echo.consume("a"))
        assertEquals(peerA, echo.consume("d"))
    }

    @Test
    fun `clearing drops everything`() {
        val echo = cache()
        echo.arm("id-1", peerA)
        echo.arm("id-2", peerB)
        echo.clear()
        assertEquals(0, echo.size())
        assertNull(echo.consume("id-1"))
    }

    /** Counts only. Never an id, never a fingerprint. */
    @Test
    fun `rendering carries no identity`() {
        val echo = cache()
        echo.arm("DISTINCTIVE-ID-HEX", peerA)
        val rendered = echo.toString()
        assertEquals("EchoSuppression(pending=1)", rendered)
        assertFalse(rendered.contains("DISTINCTIVE"))
        assertFalse(rendered.contains("aa"))
    }
}
