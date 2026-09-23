package io.github.yurisismotto.omnibridge.files

/**
 * What type to tell Android a file is, when handing it over to be opened.
 *
 * ## Why a peer's MIME type cannot be used as given
 *
 * `FileOffer.mime_type` is attacker-controlled. `FileTransferManager` bounds
 * its length and nothing else, because nothing downstream of it needed more:
 * MediaStore validates what it stores, and the bytes are verified against a
 * digest agreed before the first one moved.
 *
 * Opening changes that. The type is what Android resolves an application
 * with, so it is the one peer-supplied string that selects *code to run*. A
 * type carrying a newline can forge an intent's extras in a log; one carrying
 * a wildcard (`* / *`) widens the chooser to every installed app; one that is
 * simply not a media type at all produces an `ActivityNotFoundException`
 * where a plain "no app can open this" was the honest answer.
 *
 * So the type is re-derived at the moment of opening from the most
 * trustworthy source available — the `ContentResolver`, which answers for the
 * item as *stored* — and whatever comes out is passed through [sanitize]
 * before it reaches an Intent.
 *
 * ## What this deliberately does not do
 *
 * It does not sniff content, and it does not decide whether a file is safe.
 * "It ends in .apk" is not a security control and neither is "it ends in
 * .txt": Android's own installer consent is what guards an install, and
 * duplicating that judgement here would only make OmniBridge's version of it the
 * one that is wrong. Opening means handing the URI to the platform's ordinary
 * app resolution and letting the platform ask.
 */
object MimeTypes {

    /**
     * The honest answer when the type is unknown.
     *
     * Not a guess dressed as knowledge: it tells Android "these are bytes",
     * and a chooser that offers nothing for it is a truthful outcome rather
     * than a bug.
     */
    const val FALLBACK = "application/octet-stream"

    /** RFC 6838 restricted-name characters, plus the `/` that separates them. */
    private val TOKEN = Regex("[A-Za-z0-9][A-Za-z0-9!#\\$&^_.+-]{0,126}")

    /**
     * Reduces a claimed media type to one that is safe to put in an Intent,
     * or [FALLBACK] when it is not a media type at all.
     *
     * Parameters (`; charset=utf-8`) are dropped rather than cleaned. Android
     * matches an intent filter on type and subtype only, so a parameter can
     * change nothing about which app opens the file — it can only carry
     * something through into a place that logs it.
     *
     * A wildcard in either half is refused. `* / *` from a peer would mean
     * "offer this to everything", which is a decision about the chooser that
     * the sender of a file does not get to make.
     */
    fun sanitize(raw: String?): String {
        val candidate = raw?.substringBefore(';')?.trim()?.lowercase().orEmpty()
        if (candidate.isEmpty()) return FALLBACK

        val parts = candidate.split('/')
        if (parts.size != 2) return FALLBACK
        val (type, subtype) = parts
        if (type == "*" || subtype == "*") return FALLBACK
        if (!TOKEN.matches(type) || !TOKEN.matches(subtype)) return FALLBACK

        return "$type/$subtype"
    }

    /**
     * The first of [candidates] that survives [sanitize], else [FALLBACK].
     *
     * Callers pass them most-trustworthy first: what the resolver says about
     * the stored item, then what the offer claimed, then nothing.
     */
    fun firstUsable(vararg candidates: String?): String {
        for (candidate in candidates) {
            val clean = sanitize(candidate)
            if (clean != FALLBACK) return clean
        }
        return FALLBACK
    }
}
