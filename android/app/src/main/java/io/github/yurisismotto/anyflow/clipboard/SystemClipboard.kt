package io.github.yurisismotto.anyflow.clipboard

import android.content.ClipData
import android.content.ClipDescription
import android.content.ClipboardManager
import android.content.Context
import android.os.Build
import android.os.PersistableBundle
import android.util.Log

/**
 * The Android clipboard, and the platform rules that come with it.
 *
 * Everything that knows about [ClipboardManager] is here. Above this class
 * the capability deals in [ClipboardText] and would work unchanged against
 * another platform.
 *
 * ## Reading is restricted, and that is not worked around
 *
 * Since Android 10 (API 29), `getPrimaryClip` returns null unless the calling
 * app has input focus or is the default IME. That is a deliberate platform
 * decision — the clipboard routinely holds passwords and one-time codes — and
 * AnyFlow respects it. Specifically, this app does **not**:
 *
 *  * declare an `AccessibilityService`;
 *  * ask to become the default IME;
 *  * hold `READ_LOGS`, `SYSTEM_ALERT_WINDOW` or any hidden permission;
 *  * use reflection against private APIs;
 *  * open an invisible activity to steal focus;
 *  * poll in the background hoping to catch a moment of focus.
 *
 * The consequence is stated rather than hidden: reads happen only when the
 * person has AnyFlow in the foreground and asks for one. See
 * [ClipboardCapabilities.AUTO_SEND_SUPPORTED].
 *
 * ## Writing is not restricted the same way
 *
 * `setPrimaryClip` has no focus requirement, so a clip received from a
 * computer can be applied while AnyFlow is in the background. That asymmetry
 * is what makes automatic Fedora → Android sync possible while automatic
 * Android → Fedora sync is not. It is nonetheless verified on hardware rather
 * than assumed, and [ClipboardTarget.WriteResult] carries what actually happened so
 * the caller can fall back to a notification instead of reporting a success
 * that did not occur.
 */
class SystemClipboard(context: Context) : ClipboardTarget {

    private val appContext = context.applicationContext
    private val manager: ClipboardManager? =
        appContext.getSystemService(ClipboardManager::class.java)

    /**
     * Reads the current clipboard.
     *
     * The caller must be in the foreground. Failing that, this returns
     * [ClipboardTarget.ReadFailure.NotAllowed] rather than an empty string, so the UI can say
     * something true instead of "the clipboard is empty".
     */
    override fun read(): Result<ClipboardTarget.ReadClip> {
        val manager = manager
            ?: return Result.failure(ClipboardReadFailed(ClipboardTarget.ReadFailure.NotAllowed))

        // `getPrimaryClip` returns null for *two* different situations — the
        // clipboard is empty, and the read was refused — and telling a person
        // "open AnyFlow and try again" when their clipboard is simply empty
        // sends them round a loop that cannot end. Observed on an SM-X620
        // running Android 16, which is why this asks twice.
        //
        // `hasPrimaryClip` is itself subject to the same focus restriction, so
        // it cannot be used *instead*; it disambiguates only once a read has
        // already come back null. False here means the platform is willing to
        // answer and the answer is "nothing".
        val hasClip = try {
            manager.hasPrimaryClip()
        } catch (e: SecurityException) {
            Log.d(TAG, "clipboard probe refused: ${e.javaClass.simpleName}")
            false
        }

        val clip = try {
            manager.primaryClip
        } catch (e: SecurityException) {
            // Some builds throw rather than returning null.
            Log.d(TAG, "clipboard read refused: ${e.javaClass.simpleName}")
            null
        } ?: return Result.failure(
            ClipboardReadFailed(
                if (hasClip) {
                    ClipboardTarget.ReadFailure.NotAllowed
                } else {
                    ClipboardTarget.ReadFailure.Empty
                },
            ),
        )

        if (clip.itemCount == 0) {
            return Result.failure(ClipboardReadFailed(ClipboardTarget.ReadFailure.Empty))
        }

        val description = clip.description
        // `coerceToText` is what turns a styled span, or an intent's label,
        // into plain text. It is used rather than `item.text` so that a clip
        // from a rich-text editor is still shareable — but only when the
        // description actually claims to be text, so that an image clip does
        // not become the word "Image".
        val looksLikeText = description == null ||
            description.hasMimeType(ClipDescription.MIMETYPE_TEXT_PLAIN) ||
            description.hasMimeType(ClipDescription.MIMETYPE_TEXT_HTML)
        if (!looksLikeText) {
            return Result.failure(ClipboardReadFailed(ClipboardTarget.ReadFailure.NotText))
        }

        val coerced = clip.getItemAt(0).coerceToText(appContext)
        if (coerced.isNullOrEmpty()) {
            return Result.failure(ClipboardReadFailed(ClipboardTarget.ReadFailure.Empty))
        }

        return ClipboardText.validate(coerced).fold(
            onSuccess = { text -> Result.success(ClipboardTarget.ReadClip(text, isSensitive(description))) },
            onFailure = { failure ->
                val rejection = (failure as? ClipboardRejected)?.rejection ?: TextRejection.Empty
                Result.failure(ClipboardReadFailed(ClipboardTarget.ReadFailure.Rejected(rejection)))
            },
        )
    }

    /**
     * Replaces the clipboard with text that came from another device.
     *
     * Two platform hints are set, and both are *presentation* hints — neither
     * is enforcement and nothing in AnyFlow depends on either:
     *
     *  * `EXTRA_IS_REMOTE_DEVICE` (API 34+) tells the system this clip came
     *    from another device, which is what suppresses the "copied" toast
     *    Android would otherwise show for a clip the person did not copy, and
     *    lets the system label it as remote;
     *  * `EXTRA_IS_SENSITIVE` (API 33+), when [sensitive] is set, tells the
     *    system to hide the preview and tells clipboard managers not to keep
     *    it in history.
     *
     * @param label a short, non-content description shown in some system UI.
     *   It must never be the clipboard text itself.
     */
    override fun write(
        text: ClipboardText,
        sensitive: Boolean,
        label: String,
    ): ClipboardTarget.WriteResult {
        val manager = manager
            ?: return ClipboardTarget.WriteResult.Failed("no clipboard service")

        val clip = ClipData.newPlainText(label, text.text)
        applyExtras(clip.description, sensitive)

        return try {
            manager.setPrimaryClip(clip)
            ClipboardTarget.WriteResult.Applied
        } catch (e: RuntimeException) {
            // Documented as unchecked, and OEM builds do throw here — for
            // instance while the device is locked. A failure to write is a
            // result the caller must handle, not a crash.
            Log.w(TAG, "setPrimaryClip failed: ${e.javaClass.simpleName}")
            ClipboardTarget.WriteResult.Failed(e.javaClass.simpleName)
        }
    }

    /**
     * Sets the description extras for a clip that arrived from a peer.
     *
     * Split out so an instrumented test can assert the exact bundle contents
     * on a real device, which is the only place these constants mean
     * anything.
     */
    fun applyExtras(description: ClipDescription?, sensitive: Boolean) {
        if (description == null) return
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU) return

        val extras = description.extras ?: PersistableBundle()
        if (sensitive) {
            extras.putBoolean(ClipDescription.EXTRA_IS_SENSITIVE, true)
        }
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
            extras.putBoolean(ClipDescription.EXTRA_IS_REMOTE_DEVICE, true)
        }
        description.extras = extras
    }

    /**
     * Whether the platform marked this clip sensitive. Always false below
     * API 33, where the extra does not exist.
     */
    fun isSensitive(description: ClipDescription?): Boolean {
        if (description == null) return false
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU) return false
        return description.extras?.getBoolean(ClipDescription.EXTRA_IS_SENSITIVE, false) == true
    }

    companion object {
        private const val TAG = "SystemClipboard"

        /**
         * The label attached to an applied clip.
         *
         * Fixed and content-free. Some system surfaces show a clip's label,
         * so putting any part of the text here would leak it onto a screen
         * the user did not ask for.
         */
        const val DEFAULT_LABEL = "AnyFlow"
    }
}

/** Carries a [ClipboardTarget.ReadFailure] through a [Result]. */
class ClipboardReadFailed(val failure: ClipboardTarget.ReadFailure) :
    Exception(failure.describe())
