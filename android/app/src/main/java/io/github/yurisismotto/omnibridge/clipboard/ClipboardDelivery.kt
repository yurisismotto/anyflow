package io.github.yurisismotto.omnibridge.clipboard

/**
 * What is actually known about one clipboard this phone sent.
 *
 * ## The defect this exists to correct — GitHub #8
 *
 * [ClipboardSync.sendText] used to answer `Result.success(bytes)` the instant
 * the frame was handed to the session's writer, and `MainActivity` turned
 * that into **"Sent 42 bytes to fedora."** Writing bytes into a TLS session
 * is not delivery, and on real hardware the two came apart: the desktop had
 * no `clipboard.v1` grant for this phone, answered
 * `ERROR_CODE_UNSUPPORTED_CAPABILITY`, dropped the clip — and the phone said
 * it had been sent. The person had every reason to believe a password was on
 * their computer when it was not.
 *
 * So the four states are kept apart, permanently, and nothing collapses them:
 *
 *  * [Enqueued] — the frame is on the session. **Local knowledge only.**
 *  * [Confirmed] — the peer said what it did with it, and it arrived.
 *  * [Rejected] — the peer said what it did with it, and it did not.
 *  * [Refused] — the peer refused the *capability*, so the clip was never
 *    looked at.
 *  * [Unconfirmed] — no verdict came back. Not a success and not a failure,
 *    and said as exactly that.
 *
 * ## Nothing here can carry content
 *
 * By shape, not by convention: every field is a byte count, an enum or a
 * device name. There is no `String` in this file that a clip could reach,
 * which is what makes "the outcome model never holds clipboard text" a
 * property of the type rather than a rule someone has to remember. The same
 * reasoning as `ClipboardSync.PendingClipInfo`.
 */
sealed interface ClipboardDelivery {

    /** Bytes of UTF-8 this delivery is about. Never the text. */
    val bytes: Int

    /**
     * The frame is on the session and nothing has come back yet.
     *
     * The honest name for what the old `Result.success` meant. It is a
     * *pending* state, never a success, and the wording says so.
     */
    data class Enqueued(override val bytes: Int) : ClipboardDelivery

    /**
     * The peer reported a verdict and the clip reached it.
     *
     * `DUPLICATE` counts: the peer already had this exact event, which means
     * an earlier copy of it arrived. Reporting that as a failure would make a
     * retry look like a fault.
     */
    data class Confirmed(
        val outcome: ClipboardSync.Outcome,
        override val bytes: Int,
    ) : ClipboardDelivery

    /** The peer reported a verdict and refused the clip. */
    data class Rejected(
        val outcome: ClipboardSync.Outcome,
        override val bytes: Int,
    ) : ClipboardDelivery

    /**
     * The peer refused the whole capability, so the clip was never examined.
     *
     * Reached through the session's `ERROR` envelope correlated back to the
     * frame that caused it — not through a `ClipboardResult`, because a peer
     * that will not speak `clipboard.v1` does not answer in `clipboard.v1`.
     */
    data class Refused(
        val reason: Refusal,
        override val bytes: Int,
    ) : ClipboardDelivery

    /**
     * No verdict, and there will not be one.
     *
     * The state the product had no word for. It is not a failure — the clip
     * may well have arrived — and it is emphatically not a success.
     */
    data class Unconfirmed(
        val reason: Reason,
        override val bytes: Int,
    ) : ClipboardDelivery {
        enum class Reason { DISCONNECTED, TIMED_OUT }
    }

    /**
     * Why a peer refused the capability rather than the clip.
     *
     * A closed vocabulary, deliberately: the peer's `Error.message` is a free
     * string from the other end of the wire and is never put on this screen.
     */
    enum class Refusal {
        /** The peer's session did not negotiate `clipboard.v1`. */
        NOT_NEGOTIATED,

        /** The peer has no `clipboard.v1` grant for this phone. */
        NOT_AUTHORIZED,

        /** The peer is shedding load. */
        RATE_LIMITED,

        /** Anything else the peer said. Never shown in the peer's words. */
        OTHER,
    }

    /**
     * Whether this may be presented to a person as a success.
     *
     * A whitelist of one. Anything added to this file later is not a success
     * until somebody says so here, which is the safe direction for the
     * question "did my password reach my computer".
     */
    val succeeded: Boolean get() = this is Confirmed

    /** Whether a verdict is still theoretically coming. */
    val settled: Boolean get() = this !is Enqueued

    /**
     * One line for a person, naming the computer.
     *
     * The single mapping asked for by the brief's failure vocabulary: every
     * state this app can be in about a sent clip has its wording here and
     * nowhere else, so a screen cannot invent a cheerier one. No byte of the
     * clip, no capability id, no protocol enum and no peer-supplied text.
     */
    fun describe(peerName: String): String = when (this) {
        is Enqueued ->
            "Sent $bytes bytes to the connection; $peerName has not confirmed it yet."

        is Confirmed -> when (outcome) {
            ClipboardSync.Outcome.APPLIED ->
                "$peerName received $bytes bytes and copied them to its clipboard."
            ClipboardSync.Outcome.PENDING_USER ->
                "$peerName received $bytes bytes and is holding them until you apply them there."
            ClipboardSync.Outcome.DUPLICATE ->
                "$peerName already had that clipboard."
            // Unreachable: `Confirmed` is only built from the three above.
            // Stated rather than defaulted so that adding an outcome to
            // `Outcome` fails here instead of quietly reading as success.
            else -> "$peerName received $bytes bytes."
        }

        is Rejected -> when (outcome) {
            ClipboardSync.Outcome.NOT_AUTHORIZED ->
                "$peerName is not set up to accept your clipboard."
            ClipboardSync.Outcome.REJECTED_POLICY ->
                "$peerName is not accepting clipboard text from this phone."
            ClipboardSync.Outcome.REJECTED_SENSITIVE ->
                "$peerName refuses clipboard text marked sensitive."
            ClipboardSync.Outcome.TOO_LARGE ->
                "That clipboard is too large for $peerName. Send it as a file instead."
            ClipboardSync.Outcome.INVALID_TEXT ->
                "$peerName could not read that clipboard as text."
            else ->
                "$peerName could not use that clipboard."
        }

        is Refused -> when (reason) {
            Refusal.NOT_NEGOTIATED ->
                "This connection has not negotiated clipboard sharing yet. " +
                    "Nothing was sent to $peerName."
            Refusal.NOT_AUTHORIZED ->
                "$peerName is not set up to accept your clipboard. Nothing was sent."
            Refusal.RATE_LIMITED ->
                "$peerName is busy and did not take that clipboard. Try again."
            Refusal.OTHER ->
                "$peerName refused that clipboard."
        }

        is Unconfirmed -> when (reason) {
            Unconfirmed.Reason.DISCONNECTED ->
                "The connection to $peerName ended before it confirmed. " +
                    "Delivery of $bytes bytes is not confirmed."
            Unconfirmed.Reason.TIMED_OUT ->
                "Sent $bytes bytes to $peerName; delivery not confirmed."
        }
    }

    companion object {
        /**
         * Sorts one peer verdict into [Confirmed] or [Rejected].
         *
         * The one place that decides which outcomes mean the clip arrived.
         * `DUPLICATE` is on the arrived side because it *is* one: the peer
         * recognised the event id, which it could only have got from us.
         */
        fun of(outcome: ClipboardSync.Outcome, bytes: Int): ClipboardDelivery = when (outcome) {
            ClipboardSync.Outcome.APPLIED,
            ClipboardSync.Outcome.PENDING_USER,
            ClipboardSync.Outcome.DUPLICATE,
            -> Confirmed(outcome, bytes)

            ClipboardSync.Outcome.NOT_AUTHORIZED,
            ClipboardSync.Outcome.REJECTED_POLICY,
            ClipboardSync.Outcome.REJECTED_SENSITIVE,
            ClipboardSync.Outcome.TOO_LARGE,
            ClipboardSync.Outcome.INVALID_TEXT,
            ClipboardSync.Outcome.FAILED,
            -> Rejected(outcome, bytes)
        }
    }
}
