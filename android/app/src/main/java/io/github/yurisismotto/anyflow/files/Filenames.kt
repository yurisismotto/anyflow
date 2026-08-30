package io.github.yurisismotto.anyflow.files

/**
 * Reduces a peer-supplied filename to something safe to create.
 *
 * Mirrors `anyflow_capability_files::filename` on the desktop, rule for rule,
 * and the two test suites assert the same cases. A divergence here would mean
 * one of the two devices accepting a name the other refuses, which is exactly
 * the kind of quiet asymmetry a path-traversal bug lives in.
 *
 * The rule: a filename from the network is a *hint about what to call the
 * file*, never an instruction about where to put it. What comes out of
 * [sanitize] is a bare name; the caller decides the location, always.
 *
 * On Android the location is a MediaStore `RELATIVE_PATH` the app chooses, so
 * the peer could not name a directory even if it wanted to. This runs anyway:
 * a name with a `/` in it would be rejected by MediaStore in a way the user
 * could not act on, and a name full of control characters could forge the
 * notification it appears in.
 */
object Filenames {

    /** `NAME_MAX` on every filesystem this runs on. Counted in bytes. */
    const val MAX_FILENAME_BYTES = 255

    /**
     * Windows device names, unusable as filenames there even with an
     * extension (`CON.txt` is still `CON`). Nothing here runs on Windows, but
     * a received file is routinely synced or shared onward.
     */
    private val RESERVED_STEMS = setOf(
        "con", "prn", "aux", "nul",
        "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8", "com9",
        "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
    )

    /**
     * @return a bare, safe filename, or null when nothing usable survives.
     *
     * Null means "reject this offer", never "pick a name for them": inventing
     * a name for a file whose own name was hostile hides the attack from the
     * person being attacked.
     */
    fun sanitize(raw: String): String? {
        // The last path component, treating both separators as separators so
        // a Windows-shaped path cannot smuggle a component past a Unix split.
        val base = raw.substringAfterLast('/').substringAfterLast('\\')

        // Control characters (NUL included) and any surviving separator go. A
        // NUL truncates a C string, so a name that passes a Kotlin check could
        // still mean something else to whatever opens the file later.
        val cleaned = base.filter { !it.isISOControl() && it != '/' && it != '\\' }

        // Leading whitespace is cosmetic. Trailing dots and spaces are not:
        // Windows and SMB silently trim them, so `evil.txt.` and `evil.txt`
        // are the same file there.
        val trimmed = cleaned.trimStart().trimEnd(' ', '.', '\t')

        if (trimmed.isEmpty() || trimmed == "." || trimmed == "..") return null
        if (trimmed.substringBefore('.').lowercase() in RESERVED_STEMS) return null

        val capped = truncatePreservingExtension(trimmed)
        return capped.ifEmpty { null }
    }

    /**
     * Caps a name at [MAX_FILENAME_BYTES], cutting on a character boundary and
     * keeping the extension where it fits.
     *
     * Keeping the extension matters more than keeping the tail of the stem:
     * `holiday-….jpg` still opens with the right app, `holiday-…` does not.
     */
    private fun truncatePreservingExtension(name: String): String {
        if (name.toByteArray(Charsets.UTF_8).size <= MAX_FILENAME_BYTES) return name

        val dot = name.lastIndexOf('.')
        // A dot at position 0 is a leading dot, not an extension separator.
        var extension = if (dot > 0) name.substring(dot) else ""
        // An "extension" that is itself enormous is not an extension.
        if (extension.toByteArray(Charsets.UTF_8).size > 16) extension = ""

        val budget = MAX_FILENAME_BYTES - extension.toByteArray(Charsets.UTF_8).size
        val stem = name.substring(0, name.length - extension.length)

        // Walk back until the UTF-8 encoding fits, so the cut never splits a
        // multi-byte character into a replacement character.
        var end = stem.length
        while (end > 0 && stem.substring(0, end).toByteArray(Charsets.UTF_8).size > budget) {
            end--
        }
        return stem.substring(0, end) + extension
    }
}
