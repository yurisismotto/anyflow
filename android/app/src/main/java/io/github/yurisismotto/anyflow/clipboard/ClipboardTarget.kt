package io.github.yurisismotto.anyflow.clipboard

/**
 * The clipboard, as the capability needs it.
 *
 * [SystemClipboard] is the one real implementation. The interface exists so
 * that the *domain* logic — policy, de-duplication, loop suppression — can be
 * tested on the JVM, where there is no `ClipboardManager` at all.
 *
 * That split is deliberate and bounded: a fake implements this interface, and
 * nothing more. It is never used to make a claim about how the platform
 * behaves. Every assertion about `getPrimaryClip`, `setPrimaryClip`,
 * `EXTRA_IS_SENSITIVE` or `EXTRA_IS_REMOTE_DEVICE` lives in an *instrumented*
 * test on a real device, because a mock that agrees with our assumptions
 * would prove only that we are consistent, not that we are right.
 */
interface ClipboardTarget {

    /** Why a read did not produce text. */
    sealed interface ReadFailure {
        /** The platform refused: no input focus, and not the default IME. */
        data object NotAllowed : ReadFailure

        /**
         * The clipboard is empty.
         *
         * Distinguished from [NotAllowed] by asking `hasPrimaryClip()` after
         * `getPrimaryClip()` came back null — the platform returns null for
         * both, and telling someone to open an app that is already open,
         * because their clipboard is empty, is a loop with no exit.
         *
         * The disambiguation is only reliable **with window focus**, because
         * `hasPrimaryClip()` is subject to the same restriction. That is
         * enough: AnyFlow only ever reads from a focused Activity. Without
         * focus the two are indistinguishable from inside the app, and the
         * outcome is the same either way — nothing is read and nothing is
         * sent.
         */
        data object Empty : ReadFailure

        /** It holds something that is not text — an image, a file URI. */
        data object NotText : ReadFailure

        /** The text is there but unusable — oversized, or NUL-bearing. */
        data class Rejected(val rejection: TextRejection) : ReadFailure

        fun describe(): String = when (this) {
            NotAllowed ->
                "Android would not let AnyFlow read the clipboard. Open AnyFlow " +
                    "and try again — the clipboard can only be read while the app " +
                    "is on screen."
            Empty -> "The clipboard is empty."
            NotText -> "The clipboard does not contain text. Only text can be shared."
            is Rejected -> rejection.describe()
        }
    }

    /** A clip read from the platform, with the platform's own hints. */
    data class ReadClip(
        val text: ClipboardText,
        /**
         * What `ClipDescription.EXTRA_IS_SENSITIVE` said, on API 33+.
         *
         * A hint from whichever app produced the clip — a password manager
         * sets it — and nothing more. It is not an access control and carries
         * no guarantee: a sensitive clip is still perfectly readable here.
         * What it is *for* is deciding how to present the clip, and whether
         * to ask once more before it leaves the device.
         */
        val sensitive: Boolean,
    )

    /** What a write actually did. */
    sealed interface WriteResult {
        data object Applied : WriteResult

        /**
         * The platform refused or threw. Real on some builds and while the
         * device is locked, so it is a value rather than an exception: the
         * caller falls back to a notification rather than reporting a success
         * that did not happen.
         */
        data class Failed(val reason: String) : WriteResult
    }

    /** The current clipboard text, with the platform's sensitivity hint. */
    fun read(): Result<ReadClip>

    /** Replaces the clipboard contents. */
    fun write(
        text: ClipboardText,
        sensitive: Boolean,
        label: String = "AnyFlow",
    ): WriteResult
}
