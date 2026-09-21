package io.github.yurisismotto.omnibridge

import io.github.yurisismotto.omnibridge.net.Backoff
import io.github.yurisismotto.omnibridge.net.ConnectionCoordinator
import io.github.yurisismotto.omnibridge.net.ConnectionEvent
import io.github.yurisismotto.omnibridge.net.DialResult
import io.github.yurisismotto.omnibridge.net.LinkState
import java.net.InetSocketAddress
import java.util.concurrent.CopyOnWriteArrayList
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The reconnect state machine, on a virtual clock.
 *
 * ## What these are actually protecting
 *
 * A defect observed on real hardware: after a run of failures the connection
 * job ended and nothing ever scheduled another attempt. Eight minutes passed
 * with zero retries, and the phone only recovered because a Wi-Fi
 * `onAvailable` happened to restart it. The cause was an exception escaping
 * the loop — the loop's own supervisor treated that as "the job finished".
 *
 * So the property under test is not "does it retry" but "can it stop
 * retrying by accident". Every test below therefore drives *time*, not
 * events, and asserts that attempts keep happening with nothing but the clock
 * moving. Anything relying on a network callback to make progress would pass
 * even with the original bug present.
 */
@OptIn(ExperimentalCoroutinesApi::class)
class ReconnectTest {

    private val endpoint = InetSocketAddress.createUnresolved("192.168.0.10", 47635)
    private val other = InetSocketAddress.createUnresolved("192.168.0.11", 47635)

    /** No jitter, so delays are exactly the documented ladder. */
    private fun backoff() = Backoff(random = { 0.5 })

    private class Recorder {
        val events = CopyOnWriteArrayList<ConnectionEvent>()
        val states = CopyOnWriteArrayList<LinkState>()

        fun count(kind: ConnectionEvent.Kind) = events.count { it.kind == kind }
        fun kinds() = events.map { it.kind }
        fun field(kind: ConnectionEvent.Kind, name: String): List<Any?> =
            events.filter { it.kind == kind }
                .mapNotNull { e -> e.fields.firstOrNull { it.first == name }?.second }
    }

    private fun TestScope.coordinator(
        recorder: Recorder,
        endpoints: suspend (Int) -> List<InetSocketAddress> = { listOf(endpoint) },
        dial: suspend (InetSocketAddress) -> DialResult,
    ) = ConnectionCoordinator(
        scope = this,
        endpoints = endpoints,
        dial = dial,
        backoff = backoff(),
        log = { recorder.events += it },
        onState = { recorder.states += it },
    )

    // -- R1 -----------------------------------------------------------------

    @Test
    fun `R1 a first connect failure is followed by another attempt`() = runTest {
        val recorder = Recorder()
        var attempts = 0
        val c = coordinator(recorder) {
            attempts += 1
            DialResult.Transient("connection refused")
        }

        c.start()
        advanceTimeBy(1)
        assertEquals(1, attempts)

        // Nothing happens here except time passing. No network event, no
        // second start() — which is precisely the case that used to hang.
        advanceTimeBy(3_000)
        assertTrue("a failed connect must schedule a retry", attempts >= 2)

        c.stop()
    }

    // -- R2 -----------------------------------------------------------------

    @Test
    fun `R2 repeated failures back off progressively`() = runTest {
        val recorder = Recorder()
        val c = coordinator(recorder) { DialResult.Transient("no route to host") }

        c.start()
        advanceTimeBy(600_000)
        c.stop()

        val delays = recorder.field(ConnectionEvent.Kind.RETRY_SCHEDULED, "delay_ms")
            .map { it as Long }

        assertTrue("expected several retries, got ${delays.size}", delays.size >= 6)
        assertEquals(listOf(2_000L, 4_000L, 8_000L, 16_000L, 32_000L, 64_000L), delays.take(6))
    }

    // -- R3 -----------------------------------------------------------------

    @Test
    fun `R3 at maximum backoff it keeps trying rather than dying`() = runTest {
        val recorder = Recorder()
        var attempts = 0
        val c = coordinator(recorder) {
            attempts += 1
            DialResult.Transient("timeout")
        }

        c.start()
        // Long past the point where the ladder saturates at 128s.
        advanceTimeBy(60 * 60_000)

        val saturated = attempts
        assertTrue("expected many attempts in an hour, got $saturated", saturated > 25)

        // The job is still alive and still scheduling. This is the assertion
        // the original defect would fail: there, attempts stopped forever.
        advanceTimeBy(10 * 60_000)
        assertTrue("retries must not stop at max backoff", attempts > saturated)
        assertTrue(c.isRunning())

        assertEquals(
            "the job must not have ended",
            0,
            recorder.count(ConnectionEvent.Kind.CONNECT_JOB_END),
        )
        c.stop()
    }

    // -- R4 -----------------------------------------------------------------

    @Test
    fun `R4 a successful session resets the backoff`() = runTest {
        val recorder = Recorder()
        var round = 0
        val c = coordinator(recorder) {
            round += 1
            if (round <= 3) {
                DialResult.Transient("refused")
            } else {
                DialResult.Established { "peer closed the connection" }
            }
        }

        c.start()
        advanceTimeBy(120_000)
        c.stop()

        val delays = recorder.field(ConnectionEvent.Kind.RETRY_SCHEDULED, "delay_ms")
            .map { it as Long }
        // 2s, 4s, 8s while failing; then the session runs and ends, and the
        // next wait is back at the bottom of the ladder rather than 16s.
        assertEquals(listOf(2_000L, 4_000L, 8_000L, 2_000L), delays.take(4))
    }

    // -- R5 -----------------------------------------------------------------

    @Test
    fun `R5 a session that drops schedules a retry`() = runTest {
        val recorder = Recorder()
        var sessions = 0
        val c = coordinator(recorder) {
            DialResult.Established {
                sessions += 1
                "peer stopped responding"
            }
        }

        c.start()
        advanceTimeBy(30_000)
        c.stop()

        assertTrue("a dropped session must reconnect, got $sessions", sessions >= 3)
        assertEquals(
            recorder.count(ConnectionEvent.Kind.SESSION_START),
            recorder.count(ConnectionEvent.Kind.SESSION_END),
        )
    }

    // -- R6 -----------------------------------------------------------------

    @Test
    fun `R6 a network becoming available brings a pending retry forward`() = runTest {
        val recorder = Recorder()
        var attempts = 0
        val c = coordinator(recorder) {
            attempts += 1
            DialResult.Transient("refused")
        }

        c.start()
        // Fail enough times to be sitting on a long backoff.
        advanceTimeBy(200_000)
        val before = attempts

        // Deep inside the wait, nothing should have happened yet...
        advanceTimeBy(1_000)
        assertEquals(before, attempts)

        // ...until the network comes back. A bounded advance, not
        // `advanceUntilIdle`: a healthy reconnect loop is never idle, which
        // is the whole point of it.
        c.onNetworkAvailable("wifi")
        advanceTimeBy(100)
        assertTrue("onAvailable must shorten the wait", attempts > before)

        c.stop()
    }

    // -- R7 -----------------------------------------------------------------

    @Test
    fun `R7 losing the network leaves no zombie and retries resume`() = runTest {
        val recorder = Recorder()
        var attempts = 0
        val c = coordinator(recorder) {
            attempts += 1
            DialResult.Transient("network unreachable")
        }

        c.start()
        advanceTimeBy(5_000)
        c.onNetworkLost("wifi")

        val before = attempts
        // onLost must not cancel the loop: time alone has to keep it going.
        advanceTimeBy(300_000)
        assertTrue("a lost network must not end the retry loop", attempts > before)
        assertTrue(c.isRunning())

        c.stop()
    }

    // -- R8 -----------------------------------------------------------------

    @Test
    fun `R8 an explicit stop is final and is not undone by a network event`() = runTest {
        val recorder = Recorder()
        var attempts = 0
        val c = coordinator(recorder) {
            attempts += 1
            DialResult.Transient("refused")
        }

        c.start()
        advanceTimeBy(5_000)
        c.stop("user asked to disconnect")
        advanceUntilIdle()

        val afterStop = attempts
        c.onNetworkAvailable("wifi")
        c.start()
        advanceTimeBy(600_000)

        assertEquals("stopping must mean stopped", afterStop, attempts)
        assertFalse(c.isRunning())
        assertEquals(LinkState.Stopped, recorder.states.last())
    }

    // -- R9 -----------------------------------------------------------------

    @Test
    fun `R9 a terminated coordinator does not resurrect itself`() = runTest {
        // The unit-test analogue of force-stop: once the owner is gone,
        // nothing in this class brings it back. The platform half of that
        // claim is the manifest — no boot receiver, START_NOT_STICKY — and
        // is verified on hardware, not here.
        val recorder = Recorder()
        var attempts = 0
        val c = coordinator(recorder) {
            attempts += 1
            DialResult.Transient("refused")
        }

        c.start()
        advanceTimeBy(1)
        c.stop("service destroyed")
        advanceUntilIdle()
        val afterStop = attempts

        repeat(5) {
            c.start()
            c.onNetworkAvailable("wifi")
            advanceTimeBy(60_000)
        }
        assertEquals(afterStop, attempts)
    }

    // -- R10 ----------------------------------------------------------------

    @Test
    fun `R10 two reconnect sources do not produce two sessions`() = runTest {
        val recorder = Recorder()
        var live = 0
        var maxLive = 0
        val release = CompletableDeferred<Unit>()

        val c = coordinator(recorder) {
            DialResult.Established {
                live += 1
                maxLive = maxOf(maxLive, live)
                release.await()
                live -= 1
                "closed"
            }
        }

        // Every way a connection can be asked for, at once.
        c.start()
        c.start()
        c.onNetworkAvailable("wifi")
        c.start()
        advanceTimeBy(10_000)

        assertEquals("only one session may be live at a time", 1, maxLive)
        assertEquals(1, recorder.count(ConnectionEvent.Kind.CONNECT_JOB_START))
        assertEquals(1, recorder.count(ConnectionEvent.Kind.SESSION_START))

        release.complete(Unit)
        advanceTimeBy(100)
        c.stop()
    }

    // -- R11 ----------------------------------------------------------------

    @Test
    fun `R11 a transient TLS error is retried on the normal ladder`() = runTest {
        val recorder = Recorder()
        val c = coordinator(recorder) { DialResult.Transient("SSLException") }

        c.start()
        advanceTimeBy(10_000)
        c.stop()

        val delays = recorder.field(ConnectionEvent.Kind.RETRY_SCHEDULED, "delay_ms")
            .map { it as Long }
        assertEquals(listOf(2_000L, 4_000L), delays.take(2))
    }

    // -- R12 ----------------------------------------------------------------

    @Test
    fun `R12 a pinning failure is retried slowly and never abandoned`() = runTest {
        val recorder = Recorder()
        var attempts = 0
        val c = coordinator(recorder) {
            attempts += 1
            DialResult.Security("the computer did not present the pinned identity")
        }

        c.start()
        advanceTimeBy(10 * 60_000)
        c.stop()

        val delays = recorder.field(ConnectionEvent.Kind.RETRY_SCHEDULED, "delay_ms")
            .map { it as Long }

        // Slow from the first failure, not after several. Hammering a peer
        // that fails the pinning check achieves nothing and is the closest
        // thing this design has to an online oracle.
        assertEquals(listOf(30_000L, 60_000L, 120_000L, 240_000L), delays.take(4))
        assertTrue("a security failure must still be retried", attempts >= 4)
        assertEquals(
            "and must never end the loop",
            0,
            recorder.count(ConnectionEvent.Kind.CONNECT_JOB_END),
        )
    }

    // -- the root cause -----------------------------------------------------

    @Test
    fun `an exception from discovery does not kill the reconnect loop`() = runTest {
        // The exact shape of the defect: NsdManager refused to start, the
        // discovery flow closed with an exception, it propagated out of the
        // connection coroutine, the job completed, and nothing rescheduled.
        val recorder = Recorder()
        var attempts = 0
        val c = coordinator(
            recorder,
            endpoints = { round ->
                if (round % 2 == 1) error("could not start discovery: 3") else listOf(endpoint)
            },
        ) {
            attempts += 1
            DialResult.Transient("refused")
        }

        c.start()
        advanceTimeBy(300_000)

        assertTrue("the loop must survive a throwing endpoint source", attempts >= 3)
        assertTrue(c.isRunning())
        assertTrue(
            "the failure must be recorded, not swallowed",
            recorder.field(ConnectionEvent.Kind.CONNECT_FAILURE, "kind").contains("unexpected"),
        )
        c.stop()
    }

    @Test
    fun `an exception from a dial does not kill the reconnect loop`() = runTest {
        val recorder = Recorder()
        var attempts = 0
        val c = coordinator(recorder) {
            attempts += 1
            if (attempts % 2 == 1) throw IllegalStateException("boom")
            DialResult.Transient("refused")
        }

        c.start()
        advanceTimeBy(300_000)

        assertTrue("the loop must survive a throwing dial", attempts >= 4)
        assertTrue(c.isRunning())
        c.stop()
    }

    // -- multi-address ------------------------------------------------------

    @Test
    fun `the second endpoint is tried when the first fails`() = runTest {
        val recorder = Recorder()
        val tried = mutableListOf<InetSocketAddress>()
        val c = coordinator(recorder, endpoints = { listOf(endpoint, other) }) { address ->
            tried += address
            if (address == other) DialResult.Established { "closed" } else DialResult.Transient("refused")
        }

        c.start()
        advanceTimeBy(1)

        assertEquals(listOf(endpoint, other), tried.take(2))
        assertEquals(1, recorder.count(ConnectionEvent.Kind.SESSION_START))
        c.stop()
    }

    @Test
    fun `having no reachable address is a retryable failure, not the end`() = runTest {
        val recorder = Recorder()
        var rounds = 0
        val c = coordinator(recorder, endpoints = { rounds += 1; emptyList() }) {
            throw AssertionError("must not dial when there is nothing to dial")
        }

        c.start()
        advanceTimeBy(60_000)

        assertTrue("an empty endpoint list must still back off and retry", rounds >= 4)
        assertTrue(c.isRunning())
        c.stop()
    }

    // -- terminal -----------------------------------------------------------

    @Test
    fun `a revoked pairing stops the loop instead of retrying forever`() = runTest {
        val recorder = Recorder()
        var attempts = 0
        val c = coordinator(recorder) {
            attempts += 1
            DialResult.Terminal("this device's pairing was revoked")
        }

        c.start()
        advanceTimeBy(600_000)

        assertEquals("a revocation is not retried", 1, attempts)
        assertFalse(c.isRunning())
        assertTrue(recorder.states.any { it is LinkState.GaveUp })
    }

    @Test
    fun `every job start is matched by a job end`() = runTest {
        val recorder = Recorder()
        val c = coordinator(recorder) { DialResult.Transient("refused") }

        c.start()
        advanceTimeBy(20_000)
        c.stop()
        advanceUntilIdle()

        assertEquals(
            recorder.count(ConnectionEvent.Kind.CONNECT_JOB_START),
            recorder.count(ConnectionEvent.Kind.CONNECT_JOB_END),
        )
        assertTrue(recorder.kinds().contains(ConnectionEvent.Kind.STOPPED))
    }
}
