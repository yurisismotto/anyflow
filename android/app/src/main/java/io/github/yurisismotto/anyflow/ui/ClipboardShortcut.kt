package io.github.yurisismotto.anyflow.ui

/**
 * When a Quick Settings clipboard request may actually read the clipboard.
 *
 * ## The defect this exists to correct — GitHub #7
 *
 * The tile opens [MainActivity] with [MainActivity.ACTION_SEND_CLIPBOARD],
 * and the Activity used to act on it from `handleIntent`, which is called
 * from `onCreate` *before* `setContent` and long before the window is
 * focused. Android has refused `getPrimaryClip` to an unfocused app since
 * Android 10; on the Android 16 certification hardware the read therefore
 * came back empty and the person was told their clipboard was empty when it
 * was not. The report was false, and the cause — "we asked too early" — was
 * invisible.
 *
 * The invariant this class holds is the fix:
 *
 * > A Quick Settings clipboard action is executed only after the Activity
 * > actually has window focus.
 *
 * Not "after a delay", not "after `onResume`". `onResume` is not focus: an
 * Activity behind the Quick Settings shade, behind a dialog, or in the
 * split-second before the window is attached is resumed and unfocused, and
 * every one of those is a real state on the path the tile takes. The only
 * honest signal is `onWindowFocusChanged(true)`, so that is the only thing
 * that drains a request here.
 *
 * ## Why it is a request id and not a boolean
 *
 * `Activity.getIntent()` keeps returning the launch intent, so a
 * configuration change — a rotation, a theme switch, a font-size change —
 * recreates the Activity and replays `ACTION_SEND_CLIPBOARD` through
 * `onCreate` again. With a boolean, rotating the phone after a tile send
 * would send the clipboard a second time, silently, to the same computer.
 * The tile therefore stamps a fresh random id on every press
 * ([MainActivity.EXTRA_REQUEST_ID]), this class remembers the last one it
 * consumed, and the Activity carries that across recreation in its saved
 * state. A replayed intent names an id that is already spent and does
 * nothing; a genuine second press names a new one and sends again.
 *
 * ## What this class deliberately is not
 *
 * It is not an authorization. Draining a request means "read the clipboard
 * and start the ordinary send", and that send still asks the trust store for
 * the grant and the per-peer policy, still resolves the target by
 * fingerprint, and still refuses a sensitive clip until the person confirms
 * it. A tile press is a shortcut past two taps, never past a permission.
 *
 * It holds no Android types on purpose, so the rule above is a JVM test
 * rather than an instrumented run that would cost the device its pairing.
 */
class ClipboardShortcut {

    /** What the Activity should do about the call it just made. */
    enum class Action {
        /** Nothing at all. No clipboard read, no send, no message. */
        NOTHING,

        /** Read the clipboard now and start the ordinary send. */
        SEND,
    }

    /**
     * The request waiting for focus, as its id. Null when none is.
     *
     * At most one: two presses before focus arrives are the same instruction
     * about the same clipboard, so the newer id replaces the older and
     * exactly one send happens. Queueing them would send the same clip twice.
     */
    private var pendingId: String? = null

    /**
     * The last id that was actually drained.
     *
     * Survives recreation through [consumedId] / [restore], which is what
     * makes a replayed launch intent inert.
     */
    private var consumedId: String? = null

    /** For `onSaveInstanceState`. */
    fun consumedId(): String? = consumedId

    /** For `onCreate`, before the launch intent is handled. */
    fun restore(consumedId: String?) {
        this.consumedId = consumedId
    }

    /** Whether a request is waiting for focus. Diagnostics and tests. */
    fun isPending(): Boolean = pendingId != null

    /**
     * A tile intent arrived.
     *
     * @param requestId the id the tile stamped. A request with no id is
     *   refused outright: it cannot be told apart from its own replay, and
     *   "send the clipboard" is not an instruction worth guessing at. Our
     *   tile always stamps one.
     * @param hasWindowFocus `Activity.hasWindowFocus()`, asked at the call
     *   site rather than assumed — an intent delivered to a screen the person
     *   is already looking at is the ordinary `onNewIntent` case and must not
     *   wait for a focus change that has already happened and will not repeat.
     */
    fun onRequest(requestId: String?, hasWindowFocus: Boolean): Action {
        if (requestId.isNullOrEmpty()) return Action.NOTHING
        // A replay of something already done. The rotation case, and the
        // process-recreation case, and a redelivered PendingIntent.
        if (requestId == consumedId) return Action.NOTHING

        if (!hasWindowFocus) {
            pendingId = requestId
            return Action.NOTHING
        }
        return drain(requestId)
    }

    /**
     * `onWindowFocusChanged`.
     *
     * Losing focus is not a cancellation: the person pulled the shade down
     * over us, or a permission dialog took the window. The request stays
     * pending and is drained when focus genuinely arrives.
     */
    fun onWindowFocusChanged(hasFocus: Boolean): Action {
        if (!hasFocus) return Action.NOTHING
        val id = pendingId ?: return Action.NOTHING
        return drain(id)
    }

    /**
     * Marks [id] spent **before** returning [Action.SEND].
     *
     * The order is the whole re-entrancy guarantee: the caller starts a
     * coroutine that reads the clipboard, and anything that re-enters this
     * object while that runs — a second focus callback, a redelivered
     * intent — finds nothing pending and an id already consumed.
     */
    private fun drain(id: String): Action {
        pendingId = null
        consumedId = id
        return Action.SEND
    }
}
