package io.github.yurisismotto.omnibridge.files

/**
 * Where a transfer is.
 *
 * Mirrors `omnibridge_capability_files::transfer::TransferState`, including the
 * transition table. State is never inferred from a socket, a file on disk or
 * a log line: [canTransitionTo] is the whole state machine, and there is
 * exactly one place that calls it.
 */
enum class TransferState {
    /** Sender side: the offer is out, no answer yet. */
    OFFERED,

    /** Receiver side: the offer arrived and a human has not answered. */
    WAITING_ACCEPT,

    /** Both sides agreed. The data stream is opening or open. */
    TRANSFERRING,

    /** Receiver side: every byte is in, and the hash is being checked. */
    VERIFYING,

    /**
     * Terminal. On the receiver this means, and only means, that the hash
     * matched and the file was promoted to a name the user can see.
     */
    COMPLETED,

    /** Terminal. */
    FAILED,

    /** Terminal. */
    CANCELLED,
    ;

    val isTerminal: Boolean
        get() = this == COMPLETED || this == FAILED || this == CANCELLED

    val isActive: Boolean
        get() = !isTerminal

    /**
     * The transition table.
     *
     * Note what is absent: nothing leaves a terminal state. A duplicate
     * FILE_COMPLETE, a late cancel, or a second data stream for a finished
     * transfer are all refused by this one function rather than by a check
     * remembered at each call site.
     */
    fun canTransitionTo(next: TransferState): Boolean = when {
        // Agreement.
        this == OFFERED && next == TRANSFERRING -> true
        this == WAITING_ACCEPT && next == TRANSFERRING -> true

        // Receiving finished; verify before claiming anything.
        this == TRANSFERRING && next == VERIFYING -> true
        this == VERIFYING && next == COMPLETED -> true

        // A sender has no verify step of its own: the receiver's
        // FILE_COMPLETE is what completes it.
        this == TRANSFERRING && next == COMPLETED -> true

        // Giving up is allowed from any live state.
        !isTerminal && (next == FAILED || next == CANCELLED) -> true

        else -> false
    }
}

/**
 * Why a transfer ended other than successfully.
 *
 * Every value is safe to show a user and safe to send a peer: no paths, no
 * filenames, no platform error text.
 */
enum class FailureReason(val display: String) {
    DECLINED_BY_USER("declined"),
    NOT_AUTHORIZED("this computer is not allowed to transfer files"),
    TOO_MANY_TRANSFERS("too many transfers at once"),
    TOO_LARGE("the file is larger than this device accepts"),
    BAD_METADATA("the offer was unusable"),
    TIMED_OUT("timed out"),
    INTEGRITY("the file did not arrive intact"),
    STORAGE("could not save the file"),
    TRANSPORT("the connection ended mid-transfer"),
    CANCELLED_BY_USER("cancelled"),
    REVOKED("the pairing was revoked"),
    UNKNOWN_TRANSFER("no such transfer"),
    ;

    /**
     * Whether this should be reported as a cancellation rather than a
     * failure. Keeping them apart is what stops "you pressed cancel" being
     * displayed as an error.
     */
    val isCancellation: Boolean
        get() = this == CANCELLED_BY_USER || this == DECLINED_BY_USER
}
