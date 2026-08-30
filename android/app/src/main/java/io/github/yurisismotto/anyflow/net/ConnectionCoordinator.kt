package io.github.yurisismotto.anyflow.net

import java.net.InetSocketAddress
import kotlin.coroutines.cancellation.CancellationException
import kotlin.math.max
import kotlin.math.min
import kotlin.math.pow
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.withTimeoutOrNull

/** What one dial produced. */
sealed interface DialResult {
    /**
     * The connection is up. [session] runs it until it ends and returns a
     * short reason why, which is only ever used for logging and display.
     */
    data class Established(val session: suspend () -> String) : DialResult

    /** The network did not cooperate. Ordinary, expected, and retried. */
    data class Transient(val reason: String) : DialResult

    /**
     * The peer failed to prove it is the pinned identity. Retried, but on a
     * much slower ladder, and *never* by relaxing the pin or re-pairing.
     */
    data class Security(val reason: String) : DialResult

    /**
     * Retrying cannot help: the computer revoked this device, or no longer
     * knows it. Only a human can change that, so the loop stops.
     */
    data class Terminal(val reason: String) : DialResult
}

/** What the coordinator is doing, for the UI and the notification. */
sealed interface LinkState {
    data object Stopped : LinkState
    data object Connecting : LinkState
    data object Connected : LinkState
    data class RetryWait(val reason: String, val delayMs: Long) : LinkState
    data class GaveUp(val reason: String) : LinkState
}

/**
 * Capped exponential backoff with jitter, on two separate ladders.
 *
 * The transient ladder is the one from before this file existed — 2s doubling
 * to 128s under a 5-minute ceiling — deliberately unchanged, because a
 * shorter one is a battery bug. The security ladder is much slower: a peer
 * that fails the pinning check is not going to start passing it because we
 * asked more often, and hammering it would be the closest thing this design
 * has to an online oracle.
 *
 * Jitter exists so that several devices coming back after the same Wi-Fi
 * outage do not all dial in the same instant.
 */
class Backoff(
    private val baseMs: Long = 2_000,
    private val ceilingMs: Long = 300_000,
    private val shiftCap: Int = 6,
    private val securityBaseMs: Long = 30_000,
    private val securityCeilingMs: Long = 900_000,
    private val securityShiftCap: Int = 5,
    private val jitterRatio: Double = 0.2,
    /** Injectable so tests are deterministic. Returns 0.0..1.0. */
    private val random: () -> Double = { java.util.concurrent.ThreadLocalRandom.current().nextDouble() },
) {
    /** Which ladder a failure lands on. */
    enum class Ladder { TRANSIENT, SECURITY }

    /**
     * Delay before attempt number [failures] + 1, where `failures` counts the
     * consecutive failures so far. `failures == 0` never happens: the caller
     * increments before asking.
     */
    fun delayMs(ladder: Ladder, failures: Int): Long {
        val (base, ceiling, cap) = when (ladder) {
            Ladder.TRANSIENT -> Triple(baseMs, ceilingMs, shiftCap)
            Ladder.SECURITY -> Triple(securityBaseMs, securityCeilingMs, securityShiftCap)
        }
        val steps = (failures - 1).coerceIn(0, cap)
        val plain = min(ceiling, (base * 2.0.pow(steps)).toLong())
        val spread = (plain * jitterRatio * (random() * 2 - 1)).toLong()
        // Never below half the base: jitter must not turn into a fast retry.
        return max(base / 2, min(ceiling, plain + spread))
    }
}

/**
 * The single owner of "should we be connected, and if not, when do we try
 * again".
 *
 * ## The defect this replaces
 *
 * Reconnection used to be a `while (true)` loop inside a coroutine started by
 * `ensureConnecting()`, which started one only `if (connectionJob?.isActive
 * != true)`. Any exception escaping that loop — and one did, from mDNS
 * discovery failing to start — completed the job normally as far as the
 * supervising scope was concerned. Nothing crashed, nothing logged, and
 * `isActive` was then permanently `false`, so no later call started anything.
 * The result was observed on real hardware: eight minutes, zero retries, and
 * recovery only when a Wi-Fi `onAvailable` happened to call
 * `ensureConnecting()` again.
 *
 * ## The rules that replace it
 *
 *  * **One owner.** Only this class starts connections. Network callbacks and
 *    service starts are inputs to it, never parallel initiators, so two
 *    sources can never produce two sessions.
 *  * **A round cannot kill the loop.** Every attempt is wrapped: anything
 *    thrown that is not cancellation becomes an ordinary failure with a
 *    scheduled retry. The loop is additionally restarted if it ever unwinds.
 *  * **The timer is not the network callback.** `NETWORK_AVAILABLE` can wake
 *    a waiting retry early, but it is never the only thing that schedules
 *    one. Silence is not a state this machine can rest in.
 *  * **Stopping is explicit and final.** [stop] is the only way to reach
 *    [LinkState.Stopped], and nothing — including a later network event —
 *    starts it again.
 *
 * ## State
 *
 * ```text
 *   Stopped ──start()──► Connecting ──dial ok──► Connected
 *      ▲                    ▲   │                    │
 *      │                    │   └─ dial failed ──┐   │ session ended
 *      │              wake  │                    ▼   ▼
 *      └──stop()────────────┴──────────────  RetryWait
 *                                                │
 *                            terminal failure ───┴──► GaveUp
 * ```
 */
class ConnectionCoordinator(
    private val scope: CoroutineScope,
    private val endpoints: suspend (round: Int) -> List<InetSocketAddress>,
    private val dial: suspend (InetSocketAddress) -> DialResult,
    private val backoff: Backoff = Backoff(),
    private val log: (ConnectionEvent) -> Unit = {},
    private val onState: (LinkState) -> Unit = {},
) {

    /**
     * Conflated: several wake-ups while one retry is pending mean the same
     * thing as one, and a wake-up with nobody waiting must not be queued up
     * to short-circuit a *later* backoff.
     */
    private val wake = Channel<Unit>(Channel.CONFLATED)

    private var job: Job? = null

    /** Guarded by `this`; read by the loop, written by callers. */
    private var stopped = false
    private var failures = 0
    private var ladder = Backoff.Ladder.TRANSIENT

    /** True while a connection loop is running. */
    @Synchronized
    fun isRunning(): Boolean = job?.isActive == true

    /**
     * Starts the loop if it is not already running.
     *
     * Synchronised because the callers are not on one thread: the service
     * starts it from the main thread and `ConnectivityManager` calls back on
     * its own. An unsynchronised check-then-launch is how two loops — and
     * therefore two sessions — would appear.
     */
    @Synchronized
    fun start() {
        if (stopped || job?.isActive == true) return
        job = scope.launch { runLoop() }
    }

    /**
     * Stops for good. A user pressing Disconnect, or the service being
     * destroyed, is not a failure and must not schedule a retry.
     */
    @Synchronized
    fun stop(reason: String = "stopped") {
        if (stopped) return
        stopped = true
        val running = job
        job = null
        log(ConnectionEvent.of(ConnectionEvent.Kind.RETRY_CANCELLED, "reason" to reason))
        log(ConnectionEvent.of(ConnectionEvent.Kind.STOPPED, "reason" to reason))
        running?.cancel()
        onState(LinkState.Stopped)
    }

    /**
     * A usable network appeared. Worth trying immediately, so the pending
     * backoff is cut short and the ladder reset — but note that this only
     * *shortens* a wait that already existed. It is not what creates it.
     */
    fun onNetworkAvailable(transport: String? = null) {
        log(ConnectionEvent.of(ConnectionEvent.Kind.NETWORK_AVAILABLE, "transport" to transport))
        synchronized(this) {
            if (stopped) return
            failures = 0
            ladder = Backoff.Ladder.TRANSIENT
        }
        wake.trySend(Unit)
        // Belt and braces: if the loop is somehow not running, this is a
        // moment where starting it is exactly right.
        start()
    }

    fun onNetworkLost(transport: String? = null) {
        log(ConnectionEvent.of(ConnectionEvent.Kind.NETWORK_LOST, "transport" to transport))
        // Deliberately does not cancel anything. The session's own read will
        // fail, the round will end, and a retry will be scheduled by the one
        // component allowed to schedule retries. Cancelling the job here is
        // how a lost network used to leave nothing behind to restart it.
    }

    private suspend fun runLoop() {
        log(ConnectionEvent.of(ConnectionEvent.Kind.CONNECT_JOB_START))
        var round = 0
        try {
            while (currentlyWanted()) {
                round += 1

                val outcome = try {
                    runRound(round)
                } catch (e: CancellationException) {
                    throw e
                } catch (e: Throwable) {
                    // The heart of the fix. Discovery failing to start, a
                    // trust-store read throwing, anything at all: it becomes
                    // an ordinary failure that still schedules a retry,
                    // instead of quietly ending the job forever.
                    log(
                        ConnectionEvent.of(
                            ConnectionEvent.Kind.CONNECT_FAILURE,
                            "round" to round,
                            "kind" to "unexpected",
                            "error" to e.javaClass.simpleName,
                        ),
                    )
                    Outcome.Failed(Backoff.Ladder.TRANSIENT, e.javaClass.simpleName)
                }

                when (outcome) {
                    is Outcome.Terminal -> {
                        onState(LinkState.GaveUp(outcome.reason))
                        log(
                            ConnectionEvent.of(
                                ConnectionEvent.Kind.STOPPED,
                                "reason" to outcome.reason,
                            ),
                        )
                        synchronized(this) { stopped = true }
                        return
                    }

                    is Outcome.SessionEnded -> synchronized(this) {
                        // A session that ran is proof the endpoint works, so
                        // the ladder starts over rather than punishing a link
                        // that has been up for hours.
                        failures = 0
                        ladder = Backoff.Ladder.TRANSIENT
                    }

                    is Outcome.Failed -> synchronized(this) {
                        failures += 1
                        ladder = outcome.ladder
                    }
                }

                if (!currentlyWanted()) break

                val (delayMs, attemptNo, currentLadder) = synchronized(this) {
                    val attempts = if (failures == 0) 1 else failures
                    Triple(backoff.delayMs(ladder, attempts), failures, ladder)
                }
                log(
                    ConnectionEvent.of(
                        ConnectionEvent.Kind.RETRY_SCHEDULED,
                        "round" to round,
                        "failures" to attemptNo,
                        "ladder" to currentLadder,
                        "delay_ms" to delayMs,
                    ),
                )
                onState(LinkState.RetryWait(outcome.reason, delayMs))
                waitBeforeRetry(delayMs)
            }
        } finally {
            log(ConnectionEvent.of(ConnectionEvent.Kind.CONNECT_JOB_END, "rounds" to round))
        }
    }

    /** One pass over the endpoints. Never throws except for cancellation. */
    private suspend fun runRound(round: Int): Outcome {
        onState(LinkState.Connecting)

        val candidates = Endpoints.order(endpoints(round))
        log(
            ConnectionEvent.of(
                ConnectionEvent.Kind.DISCOVERY,
                "round" to round,
                "endpoints" to candidates.size,
            ),
        )
        if (candidates.isEmpty()) {
            return Outcome.Failed(Backoff.Ladder.TRANSIENT, "no reachable address")
        }

        var worst: Outcome.Failed? = null

        for ((index, endpoint) in candidates.withIndex()) {
            log(
                ConnectionEvent.of(
                    ConnectionEvent.Kind.CONNECT_ATTEMPT,
                    "round" to round,
                    "endpoint" to Endpoints.format(endpoint),
                    "index" to index,
                    "of" to candidates.size,
                ),
            )

            when (val result = dial(endpoint)) {
                is DialResult.Established -> {
                    log(
                        ConnectionEvent.of(
                            ConnectionEvent.Kind.CONNECT_SUCCESS,
                            "round" to round,
                            "endpoint" to Endpoints.format(endpoint),
                        ),
                    )
                    onState(LinkState.Connected)
                    log(ConnectionEvent.of(ConnectionEvent.Kind.SESSION_START))
                    val reason = result.session()
                    log(ConnectionEvent.of(ConnectionEvent.Kind.SESSION_END, "reason" to reason))
                    return Outcome.SessionEnded(reason)
                }

                is DialResult.Terminal -> {
                    log(
                        ConnectionEvent.of(
                            ConnectionEvent.Kind.CONNECT_FAILURE,
                            "round" to round,
                            "kind" to "terminal",
                            "reason" to result.reason,
                        ),
                    )
                    return Outcome.Terminal(result.reason)
                }

                is DialResult.Security -> {
                    // The remaining endpoints are still tried: discovery is
                    // attacker-controlled, so one impostor record must not
                    // hide the real computer. The pin protects each attempt
                    // independently. What the failure *does* change is the
                    // ladder, so a peer failing the pinning check is dialled
                    // rarely rather than hammered.
                    log(
                        ConnectionEvent.of(
                            ConnectionEvent.Kind.CONNECT_FAILURE,
                            "round" to round,
                            "kind" to "security",
                            "endpoint" to Endpoints.format(endpoint),
                            "reason" to result.reason,
                        ),
                    )
                    worst = Outcome.Failed(Backoff.Ladder.SECURITY, result.reason)
                }

                is DialResult.Transient -> {
                    log(
                        ConnectionEvent.of(
                            ConnectionEvent.Kind.CONNECT_FAILURE,
                            "round" to round,
                            "kind" to "transient",
                            "endpoint" to Endpoints.format(endpoint),
                            "reason" to result.reason,
                        ),
                    )
                    if (worst == null) {
                        worst = Outcome.Failed(Backoff.Ladder.TRANSIENT, result.reason)
                    }
                }
            }
        }

        return worst ?: Outcome.Failed(Backoff.Ladder.TRANSIENT, "no endpoint answered")
    }

    /**
     * Waits out the backoff, or less if something wakes us.
     *
     * `withTimeoutOrNull` around a channel receive rather than `delay`: the
     * timeout is what guarantees the retry happens, and the channel is what
     * lets a network event bring it forward. Losing the channel would cost
     * responsiveness; losing the timeout would be the original defect.
     */
    private suspend fun waitBeforeRetry(delayMs: Long) {
        val woken = withTimeoutOrNull(delayMs) { wake.receive() } != null
        if (woken) {
            log(ConnectionEvent.of(ConnectionEvent.Kind.RETRY_CANCELLED, "reason" to "woken early"))
        }
    }

    @Synchronized
    private fun currentlyWanted(): Boolean = !stopped && scope.isActive

    private sealed interface Outcome {
        val reason: String

        data class SessionEnded(override val reason: String) : Outcome
        data class Failed(val ladder: Backoff.Ladder, override val reason: String) : Outcome
        data class Terminal(override val reason: String) : Outcome
    }
}
