package io.github.yurisismotto.omnibridge.clipboard

import org.json.JSONObject

/**
 * What this device will do with clipboard traffic for one computer.
 *
 * ## Two different questions
 *
 * `clipboard.v1` has a *grant* and a *policy*, and they are deliberately not
 * the same thing:
 *
 *  * the **grant** — in the trust store, next to `files.v1`'s — answers "may
 *    this computer speak clipboard with me at all?". It is never automatic;
 *  * the **policy** — this type — answers "and in which directions, and how
 *    automatically?".
 *
 * Collapsing them would mean either a grant that quietly enables automatic
 * two-way sync, or four capability ids. Neither is right.
 *
 * ## A peer can never set its own policy
 *
 * Every field here is decided and stored locally. There is no protocol
 * message that changes any of them, on purpose: a policy a peer can widen is
 * not a policy.
 *
 * ## Defaults
 *
 * The two `allow_*` flags default on and the two `auto_*` flags default off,
 * matching the desktop exactly (`omnibridge_core::clipboard_policy`). Reaching
 * these defaults already required an explicit grant, so the `allow_*` pair is
 * what the person just asked for; the `auto_*` pair is the one that must
 * never turn itself on.
 */
data class ClipboardPolicy(
    /** May this phone send its clipboard to that computer at all? */
    val allowSend: Boolean = true,
    /** May that computer's clipboard updates be accepted at all? */
    val allowReceive: Boolean = true,
    /**
     * Push local clipboard changes to that computer automatically.
     *
     * **Not implementable on modern Android, and deliberately not faked.**
     * Android 10 and later refuse `getPrimaryClip` to an app that does not
     * have input focus, so a background watcher cannot read the clipboard —
     * and every way around that (an accessibility service, becoming the
     * default IME, an invisible focus-stealing activity) is either forbidden
     * by policy or user-hostile. The flag exists so the model is symmetric
     * with the desktop's and so a future platform that *does* allow it needs
     * no schema change; [ClipboardCapabilities.AUTO_SEND_SUPPORTED] is what
     * the UI reads, and it is false.
     */
    val autoSend: Boolean = false,
    /**
     * Write that computer's updates straight to the system clipboard.
     *
     * Off by default. With it off an accepted clip is held in memory and
     * offered through a notification, so a paired-but-misbehaving computer
     * cannot replace what you are about to paste.
     */
    val autoReceive: Boolean = false,
) {
    /**
     * `auto_send` alone is not enough: a policy with `allowSend` off and
     * `autoSend` on is contradictory and resolves to "no", so turning the
     * direction off cannot be defeated by a stale automatic flag.
     */
    fun mayAutoSend(): Boolean = allowSend && autoSend

    fun mayAutoReceive(): Boolean = allowReceive && autoReceive

    /** Contains no content and no identity; safe to log. */
    fun describe(): String =
        "send=${onOff(allowSend)} receive=${onOff(allowReceive)} " +
            "auto-send=${onOff(autoSend)} auto-receive=${onOff(autoReceive)}"

    fun toJson(): JSONObject = JSONObject().apply {
        put(KEY_ALLOW_SEND, allowSend)
        put(KEY_ALLOW_RECEIVE, allowReceive)
        put(KEY_AUTO_SEND, autoSend)
        put(KEY_AUTO_RECEIVE, autoReceive)
    }

    companion object {
        /** Nothing is permitted, whatever the stored flags say. */
        val DENIED = ClipboardPolicy(
            allowSend = false,
            allowReceive = false,
            autoSend = false,
            autoReceive = false,
        )

        private const val KEY_ALLOW_SEND = "allowSend"
        private const val KEY_ALLOW_RECEIVE = "allowReceive"
        private const val KEY_AUTO_SEND = "autoSend"
        private const val KEY_AUTO_RECEIVE = "autoReceive"

        /**
         * Reads a stored policy.
         *
         * A trust store written before this capability existed has no policy
         * object at all, and one written by an older build may be missing
         * individual flags. Each field falls back to its documented default
         * rather than to `false`, so an upgrade cannot silently disable a
         * direction the person had enabled — nor silently enable automation.
         */
        fun fromJson(json: JSONObject?): ClipboardPolicy {
            if (json == null) return ClipboardPolicy()
            return ClipboardPolicy(
                allowSend = json.optBoolean(KEY_ALLOW_SEND, true),
                allowReceive = json.optBoolean(KEY_ALLOW_RECEIVE, true),
                autoSend = json.optBoolean(KEY_AUTO_SEND, false),
                autoReceive = json.optBoolean(KEY_AUTO_RECEIVE, false),
            )
        }

        private fun onOff(value: Boolean) = if (value) "on" else "off"
    }
}

/**
 * What this platform can actually do, as opposed to what the model can
 * express.
 *
 * Kept as constants rather than as prose in a document so that the UI, the
 * tests and the certification report all read the same value, and so that a
 * future Android release that changed the answer would change it in one
 * place.
 */
object ClipboardCapabilities {
    /**
     * Whether this app can watch the clipboard and push changes on its own.
     *
     * **False, and not a limitation of this implementation.** Since Android
     * 10 (API 29) `ClipboardManager.getPrimaryClip` returns null unless the
     * calling app has input focus or is the default IME. OmniBridge is a normal
     * app: it is not an IME, it holds no accessibility service, and it does
     * not steal focus. So there is no supported way to observe the clipboard
     * in the background, and this constant says so rather than the app
     * pretending and failing quietly.
     *
     * Android → Fedora is therefore a manual action. See
     * `docs/architecture/CLIPBOARD.md`.
     */
    const val AUTO_SEND_SUPPORTED = false

    /**
     * A one-line explanation for the UI, so the person is told *why* rather
     * than shown a toggle that does nothing.
     */
    const val AUTO_SEND_REASON =
        "Android does not let an ordinary app read the clipboard in the " +
            "background, so clips are sent when you tap Send clipboard."
}
