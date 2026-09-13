package io.github.yurisismotto.anyflow

import io.github.yurisismotto.anyflow.notifications.PeerRoleState
import io.github.yurisismotto.anyflow.notifications.SourceRoleState
import io.github.yurisismotto.anyflow.proto.capabilities.NotificationRole
import io.github.yurisismotto.anyflow.proto.capabilities.NotificationRoles
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * ADR-0017 — roles inside the capability, and the epoch that protects them.
 *
 * The epoch exists for exactly one failure: a reordered, duplicated or
 * replayed announcement re-widening a set that has already narrowed. Concretely
 * — the phone announces `{SOURCE}`, the user revokes access, the phone
 * announces `{}`, and a duplicate of the first message arrives afterwards.
 * Without the epoch, the desktop resumes accepting notifications from a device
 * whose user just said no.
 */
class NotificationRolesTest {

    // -- what this device announces ------------------------------------------

    @Test
    fun `the first announcement starts at epoch one`() {
        val state = SourceRoleState()
        val announcement = state.announce(SourceRoleState.SOURCING)!!
        assertEquals(1, announcement.epoch)
        assertEquals(
            // Sorted by wire number, so two runs of the same state produce the
            // same bytes — which is what makes a cross-language vector work.
            listOf(
                NotificationRole.NOTIFICATION_ROLE_SOURCE,
                NotificationRole.NOTIFICATION_ROLE_DISMISS_TARGET,
            ),
            announcement.rolesList,
        )
    }

    /**
     * ADR-0017 §1's v1 assignment for Android, complete as of N4: `SOURCE` and
     * `DISMISS_TARGET`, announced together because they are true together —
     * both need a bound listener, the OS grant, and a usable notification
     * secret, and the secret is what makes the id map that a dismissal is
     * resolved through.
     */
    @Test
    fun `a sourcing phone claims both of its v1 roles`() {
        assertEquals(
            setOf(
                NotificationRole.NOTIFICATION_ROLE_SOURCE,
                NotificationRole.NOTIFICATION_ROLE_DISMISS_TARGET,
            ),
            SourceRoleState.SOURCING,
        )
    }

    /**
     * And never the sink-side ones. Android displays nobody else's
     * notifications and reports nothing about them, so claiming either would
     * be a promise no code here could keep.
     */
    @Test
    fun `a phone never claims a sink side role`() {
        assertFalse(NotificationRole.NOTIFICATION_ROLE_SINK in SourceRoleState.SOURCING)
        assertFalse(
            NotificationRole.NOTIFICATION_ROLE_DISMISS_REPORTER in SourceRoleState.SOURCING,
        )
    }

    /**
     * Revoking notification access takes **both** roles away at once.
     *
     * It has to: without a listener there is nothing to observe and nothing to
     * cancel. A narrowing that dropped `SOURCE` and kept `DISMISS_TARGET`
     * would leave a computer sending dismissals into a device that answers
     * `UNKNOWN_NOTIFICATION` for ever.
     */
    @Test
    fun `losing the listener narrows both roles together`() {
        val state = SourceRoleState()
        assertEquals(2, state.announce(SourceRoleState.SOURCING)!!.rolesCount)
        val narrowed = state.announce(SourceRoleState.NONE)!!
        assertEquals(0, narrowed.rolesCount)
        assertEquals(2, narrowed.epoch)
    }

    /**
     * Narrowing is expressed as an empty set with a **higher** epoch, which is
     * exactly what the phone sends the instant notification access is revoked.
     */
    @Test
    fun `narrowing raises the epoch and empties the set`() {
        val state = SourceRoleState()
        state.announce(SourceRoleState.SOURCING)

        val narrowed = state.announce(SourceRoleState.NONE)!!
        assertEquals(2, narrowed.epoch)
        assertEquals(0, narrowed.rolesCount)
    }

    /** Widening again is a further epoch: the user granted access back. */
    @Test
    fun `widening again raises the epoch further`() {
        val state = SourceRoleState()
        state.announce(SourceRoleState.SOURCING)
        state.announce(SourceRoleState.NONE)
        assertEquals(3, state.announce(SourceRoleState.SOURCING)!!.epoch)
    }

    /**
     * An unchanged set produces no announcement. An epoch is only useful while
     * it strictly increases for a *changed* set, and re-announcing on every
     * event would burn epochs and add traffic that says nothing.
     */
    @Test
    fun `an unchanged set is not re-announced`() {
        val state = SourceRoleState()
        assertNotNull(state.announce(SourceRoleState.SOURCING))
        assertNull(state.announce(SourceRoleState.SOURCING))
        assertNull(state.announce(SourceRoleState.SOURCING))
        assertEquals(1L, state.epoch())
    }

    /** The first announcement is always made, including the empty one. */
    @Test
    fun `an initial empty announcement is still sent`() {
        val state = SourceRoleState()
        val announcement = state.announce(SourceRoleState.NONE)
        assertNotNull(announcement)
        assertEquals(1, announcement!!.epoch)
        assertEquals(0, announcement.rolesCount)
    }

    @Test
    fun `epochs never repeat within a connection`() {
        val state = SourceRoleState()
        val seen = mutableSetOf<Int>()
        repeat(10) { index ->
            val roles = if (index % 2 == 0) SourceRoleState.SOURCING else SourceRoleState.NONE
            seen += state.announce(roles)!!.epoch
        }
        assertEquals(10, seen.size)
    }

    // -- what a peer announces -----------------------------------------------

    private fun announcement(epoch: Int, vararg roles: NotificationRole): NotificationRoles =
        NotificationRoles.newBuilder().apply {
            for (role in roles) addRoles(role)
            this.epoch = epoch
        }.build()

    /**
     * **Absent roles mean no roles.** A peer that never announces gets nothing
     * sent to it, which is what lets a minimal or older implementation
     * interoperate harmlessly rather than by accident.
     */
    @Test
    fun `a peer that never announced has no roles`() {
        val peer = PeerRoleState()
        assertTrue(peer.isEmpty())
        assertFalse(peer.has(NotificationRole.NOTIFICATION_ROLE_SINK))
        assertEquals(0, peer.epoch())
    }

    @Test
    fun `an announcement applies`() {
        val peer = PeerRoleState()
        assertNull(
            peer.apply(
                announcement(
                    1,
                    NotificationRole.NOTIFICATION_ROLE_SINK,
                    NotificationRole.NOTIFICATION_ROLE_DISMISS_REPORTER,
                ),
            ),
        )
        assertTrue(peer.has(NotificationRole.NOTIFICATION_ROLE_SINK))
        assertTrue(peer.has(NotificationRole.NOTIFICATION_ROLE_DISMISS_REPORTER))
        assertFalse(peer.has(NotificationRole.NOTIFICATION_ROLE_SOURCE))
    }

    /** Epoch 0 is "unset" and is refused. */
    @Test
    fun `epoch zero is refused`() {
        val peer = PeerRoleState()
        assertEquals(
            PeerRoleState.Rejection.UNSET_EPOCH,
            peer.apply(announcement(0, NotificationRole.NOTIFICATION_ROLE_SINK)),
        )
        assertTrue(peer.isEmpty())
    }

    /**
     * The failure the epoch exists for: a replayed widening after a narrowing
     * must not take effect.
     */
    @Test
    fun `a replayed announcement cannot re-widen a narrowed set`() {
        val peer = PeerRoleState()
        val wide = announcement(1, NotificationRole.NOTIFICATION_ROLE_SINK)

        peer.apply(wide)
        assertTrue(peer.has(NotificationRole.NOTIFICATION_ROLE_SINK))

        peer.apply(announcement(2))
        assertTrue(peer.isEmpty())

        // The duplicate of the first message, arriving late.
        assertEquals(PeerRoleState.Rejection.STALE_EPOCH, peer.apply(wide))
        assertTrue(peer.isEmpty())
        assertEquals(2, peer.epoch())
    }

    /**
     * *Equal* is refused too, so a duplicate of the current epoch carrying a
     * different set cannot take effect either.
     */
    @Test
    fun `an equal epoch is refused`() {
        val peer = PeerRoleState()
        peer.apply(announcement(5, NotificationRole.NOTIFICATION_ROLE_SINK))
        assertEquals(
            PeerRoleState.Rejection.STALE_EPOCH,
            peer.apply(announcement(5, NotificationRole.NOTIFICATION_ROLE_DISMISS_TARGET)),
        )
        assertTrue(peer.has(NotificationRole.NOTIFICATION_ROLE_SINK))
        assertFalse(peer.has(NotificationRole.NOTIFICATION_ROLE_DISMISS_TARGET))
    }

    /**
     * A future role value is dropped and the rest of the set still applies.
     * One unknown value must not discard an announcement, and it must not be
     * inferred into existence: a peer that has never heard of a role cannot
     * have implemented it.
     */
    @Test
    fun `an unknown role is ignored and the rest still applies`() {
        val peer = PeerRoleState()
        val wire = announcement(1, NotificationRole.NOTIFICATION_ROLE_SINK).toByteArray() +
            // Field 1, varint, value 4242 — a role from a future version.
            byteArrayOf(0x08, 0x92.toByte(), 0x21)

        assertNull(peer.apply(NotificationRoles.parseFrom(wire)))
        assertTrue(peer.has(NotificationRole.NOTIFICATION_ROLE_SINK))
        assertFalse(peer.has(NotificationRole.UNRECOGNIZED))
    }

    /** `UNSPECIFIED` is not a role either, and claiming it grants nothing. */
    @Test
    fun `the unspecified role is not a role`() {
        val peer = PeerRoleState()
        peer.apply(announcement(1, NotificationRole.NOTIFICATION_ROLE_UNSPECIFIED))
        assertTrue(peer.isEmpty())
    }

    /**
     * An empty announcement and never announcing must be indistinguishable —
     * that is the whole of ADR-0017 §2, and it is what makes a revocation and
     * a silent peer produce the same, safe behaviour.
     */
    @Test
    fun `an empty announcement means the same as never announcing`() {
        val silent = PeerRoleState()
        val revoked = PeerRoleState().apply {
            apply(announcement(1, NotificationRole.NOTIFICATION_ROLE_SINK))
            apply(announcement(2))
        }
        assertEquals(silent.isEmpty(), revoked.isEmpty())
        assertEquals(
            silent.has(NotificationRole.NOTIFICATION_ROLE_SINK),
            revoked.has(NotificationRole.NOTIFICATION_ROLE_SINK),
        )
    }

    /**
     * `epoch` is a `uint32` and arrives as a negative `Int` past 2^31. The
     * comparison is unsigned so a large epoch is still accepted in order
     * rather than being read as a rewind.
     */
    @Test
    fun `a large epoch is compared unsigned`() {
        val peer = PeerRoleState()
        peer.apply(announcement(Int.MAX_VALUE, NotificationRole.NOTIFICATION_ROLE_SINK))
        // 2^31, which is negative as a signed Int.
        assertNull(peer.apply(announcement(Int.MIN_VALUE)))
        assertTrue(peer.isEmpty())
        // And a rewind to the earlier, numerically-larger signed value fails.
        assertEquals(
            PeerRoleState.Rejection.STALE_EPOCH,
            peer.apply(announcement(Int.MAX_VALUE, NotificationRole.NOTIFICATION_ROLE_SINK)),
        )
    }

    @Test
    fun `peer role state renders counts and never a claim's content`() {
        val peer = PeerRoleState()
        peer.apply(announcement(3, NotificationRole.NOTIFICATION_ROLE_SINK))
        assertEquals("PeerRoleState(epoch=3, roles=1)", peer.toString())
    }
}
