package io.github.yurisismotto.anyflow.net

/**
 * One thing that happened in the connection lifecycle.
 *
 * ## Why structured, and why it is not debug-only
 *
 * The reconnect loop was silently dying: eight minutes with zero attempts and
 * nothing in the log to say whether the job had ended, was waiting, or had
 * never been scheduled. Free-text logging could not answer "why did this job
 * stop", because the interesting moment was the *absence* of a line.
 *
 * These events name every transition, so a `logcat` capture can be read as a
 * transcript: every job start has an end, every failure is followed by either
 * a `RETRY_SCHEDULED` or a terminal reason, and a gap is now a fact rather
 * than an inference.
 *
 * ## What must never appear here
 *
 * Pairing tokens, HMAC secrets, private key material, capability payloads.
 * Fingerprints are fine — they are public and are the only way to say which
 * peer a line is about. Addresses are fine: they are operational data, and
 * they are the thing being diagnosed.
 */
data class ConnectionEvent(
    val kind: Kind,
    val fields: List<Pair<String, Any?>> = emptyList(),
) {
    enum class Kind {
        CONNECT_JOB_START,
        CONNECT_ATTEMPT,
        CONNECT_SUCCESS,
        CONNECT_FAILURE,
        SESSION_START,
        SESSION_END,
        RETRY_SCHEDULED,
        RETRY_CANCELLED,
        NETWORK_AVAILABLE,
        NETWORK_LOST,

        /**
         * The person chose a different computer.
         *
         * Named separately from `NETWORK_AVAILABLE` even though both shorten a
         * pending backoff, because they answer different questions in a log:
         * one says the network came back, this says the destination changed.
         * Carries a fingerprint prefix — public, and the only way to say which
         * computer a line is about.
         */
        TARGET_CHANGED,
        DISCOVERY,
        STOPPED,
        CONNECT_JOB_END,
    }

    /** `KIND key=value key=value`, which greps well and reads well. */
    override fun toString(): String = buildString {
        append(kind.name)
        for ((key, value) in fields) {
            if (value == null) continue
            append(' ')
            append(key)
            append('=')
            append(value)
        }
    }

    companion object {
        fun of(kind: Kind, vararg fields: Pair<String, Any?>) =
            ConnectionEvent(kind, fields.toList())
    }
}
