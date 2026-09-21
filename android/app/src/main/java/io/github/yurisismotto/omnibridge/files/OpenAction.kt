package io.github.yurisismotto.omnibridge.files

/**
 * Whether a finished transfer can be handed to Android to open, and if not,
 * what is honestly wrong.
 *
 * ## Why this is a type and not an `if` inside a Composable
 *
 * "Can this be opened" has four different negative answers and they are not
 * interchangeable — a file the person deleted, a source whose permission
 * grant has lapsed, a transfer that never succeeded, and a device with
 * nothing installed that can read the type all produce a different sentence
 * on screen. Collapsing them into a disabled button is how a UI ends up
 * saying "File unavailable" about a file that is sitting in Downloads.
 *
 * So the decision is a plain function over plain facts ([FilesOpen.decide]),
 * testable on the JVM in microseconds, and the Composable's only job is to
 * render whichever answer came back.
 *
 * ## Why no `Uri` is in here
 *
 * The functional brief sketches this as `Available(uri, mime)`. It carries
 * neither, on purpose, and the difference is load-bearing twice over:
 *
 *  * a `content://` URI from another app's provider is a capability and
 *    routinely embeds a document id, an account, or a path. Putting one in
 *    the type the *display row* holds is how it reaches a log line, a
 *    screenshot, or a crash report. The row is built from this type, so the
 *    row cannot contain a URI that this type does not have;
 *  * a URI captured when the row was drawn is a stale answer by the time it
 *    is tapped. The grant behind an outgoing file can lapse in between. The
 *    URI is therefore fetched from [FileTransferManager] at the moment of the
 *    tap and re-checked then, which is the only moment at which the answer is
 *    true.
 *
 * [Available] means "structurally openable" — the transfer really succeeded
 * and OmniBridge really retained something to open. That is exactly the
 * condition the brief asks the button to be drawn on.
 */
sealed interface OpenAction {

    /**
     * There is a file, and OmniBridge still holds a way to reach it.
     *
     * Not a promise that the tap will succeed: nothing can promise that
     * across the gap between drawing a button and someone pressing it. It is
     * a promise that offering the button is not a lie right now.
     */
    data object Available : OpenAction

    /**
     * Outgoing only. OmniBridge no longer holds a read grant for the file it
     * sent, so it cannot open it and must not pretend otherwise.
     *
     * This is the *normal* end state for a share-sheet send, not an error:
     * the grant belonged to the Activity that received the share and died
     * with it. See [FileTransferManager.resolveOpen].
     */
    data object SourceUnavailable : OpenAction

    /** The file is gone — deleted from Downloads, or on removed storage. */
    data object FileMissing : OpenAction

    /**
     * Android resolved no application that will accept the type.
     *
     * Only ever produced by an actual attempt, never predicted: asking the
     * package manager in advance would need a query for every row and would
     * still be answering a different question than `startActivity` asks.
     */
    data object NoViewer : OpenAction

    /**
     * There is nothing to open — the transfer was declined, cancelled,
     * failed, or has not finished yet.
     */
    data object NotApplicable : OpenAction

    /** Whether a button should be offered at all. */
    val isOfferable: Boolean get() = this == Available
}

/**
 * The rules about opening, with every Android API taken out of them.
 *
 * Each parameter is a fact somebody else established — the state machine, a
 * permission check, a MediaStore query. Nothing here calls into the platform,
 * which is what lets the whole decision table be asserted on the JVM.
 */
object FilesOpen {

    /**
     * @param sending which direction this transfer ran in.
     * @param state where the state machine left it. Only [TransferState.COMPLETED]
     *   is a success; nothing infers one from bytes moved or a file existing.
     * @param hasTarget whether OmniBridge retained anything to open at all — the
     *   published MediaStore item for a received file, the shared source URI
     *   for a sent one.
     * @param accessLost whether a check has since *proved* the target
     *   unreachable. Defaults to false: this is a negative that has to be
     *   established, never assumed, or every row would claim to be broken
     *   before anyone had looked.
     */
    fun decide(
        sending: Boolean,
        state: TransferState,
        hasTarget: Boolean,
        accessLost: Boolean = false,
    ): OpenAction = when {
        // Anything that is not a real, terminal success has nothing to open.
        // Note that this reads `state`, not `bytesTransferred` and not
        // `failure == null`: a transfer that moved every byte and then failed
        // its hash check is FAILED, and must stay that way on this screen.
        state != TransferState.COMPLETED -> OpenAction.NotApplicable

        // A success with nothing retained. For a sent file that is the usual
        // case once the share-sheet Activity has gone.
        !hasTarget -> if (sending) OpenAction.SourceUnavailable else OpenAction.FileMissing

        // Something looked, and the target was not there. The two directions
        // fail for different reasons and get different words: the sender lost
        // a *permission*, the receiver lost a *file*.
        accessLost -> if (sending) OpenAction.SourceUnavailable else OpenAction.FileMissing

        else -> OpenAction.Available
    }
}
