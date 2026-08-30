package io.github.yurisismotto.anyflow

import io.github.yurisismotto.anyflow.files.FailureReason
import io.github.yurisismotto.anyflow.files.TransferState
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The transfer state machine.
 *
 * Mirrors `transfer::tests` in the Rust crate. State is never derived from a
 * socket, a file or a log line; this table is the whole machine, and both
 * sides must agree on it or one device will consider a transfer live while
 * the other has finished with it.
 */
class TransferStateTest {

    @Test
    fun `nothing leaves a terminal state`() {
        val terminals = listOf(
            TransferState.COMPLETED,
            TransferState.FAILED,
            TransferState.CANCELLED,
        )
        for (terminal in terminals) {
            for (next in TransferState.entries) {
                assertFalse(
                    "$terminal must not become $next",
                    terminal.canTransitionTo(next),
                )
            }
        }
    }

    @Test
    fun `a receiver must verify before completing`() {
        assertTrue(TransferState.TRANSFERRING.canTransitionTo(TransferState.VERIFYING))
        assertTrue(TransferState.VERIFYING.canTransitionTo(TransferState.COMPLETED))
        // Verifying cannot go back to moving bytes.
        assertFalse(TransferState.VERIFYING.canTransitionTo(TransferState.TRANSFERRING))
    }

    @Test
    fun `giving up is allowed from every live state`() {
        val live = listOf(
            TransferState.OFFERED,
            TransferState.WAITING_ACCEPT,
            TransferState.TRANSFERRING,
            TransferState.VERIFYING,
        )
        for (state in live) {
            assertTrue("$state", state.canTransitionTo(TransferState.FAILED))
            assertTrue("$state", state.canTransitionTo(TransferState.CANCELLED))
        }
    }

    @Test
    fun `a transfer cannot go backwards`() {
        assertFalse(TransferState.TRANSFERRING.canTransitionTo(TransferState.OFFERED))
        assertFalse(TransferState.TRANSFERRING.canTransitionTo(TransferState.WAITING_ACCEPT))
        assertFalse(TransferState.VERIFYING.canTransitionTo(TransferState.WAITING_ACCEPT))
        assertFalse(TransferState.OFFERED.canTransitionTo(TransferState.WAITING_ACCEPT))
    }

    @Test
    fun `a duplicate completion is refused by the table alone`() {
        // FILE-17: the second FILE_COMPLETE finds a terminal state.
        assertFalse(TransferState.COMPLETED.canTransitionTo(TransferState.COMPLETED))
        assertFalse(TransferState.COMPLETED.canTransitionTo(TransferState.CANCELLED))
    }

    @Test
    fun `a cancellation is not reported as a failure`() {
        assertTrue(FailureReason.CANCELLED_BY_USER.isCancellation)
        assertTrue(FailureReason.DECLINED_BY_USER.isCancellation)
        assertFalse(FailureReason.INTEGRITY.isCancellation)
        assertFalse(FailureReason.TRANSPORT.isCancellation)
    }

    @Test
    fun `no failure reason leaks a local detail`() {
        // These strings are shown to a user and sent to a peer, so they must
        // carry no path, filename or platform error text.
        for (reason in FailureReason.entries) {
            assertFalse(reason.display, reason.display.contains('/'))
            assertFalse(reason.display, reason.display.contains("Exception"))
        }
    }
}
