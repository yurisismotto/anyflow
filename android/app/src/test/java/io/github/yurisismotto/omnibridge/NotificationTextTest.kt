package io.github.yurisismotto.omnibridge

import io.github.yurisismotto.omnibridge.notifications.LockPolicy
import io.github.yurisismotto.omnibridge.notifications.NotificationLimits
import io.github.yurisismotto.omnibridge.notifications.NotificationMapping
import io.github.yurisismotto.omnibridge.notifications.NotificationPolicy
import io.github.yurisismotto.omnibridge.notifications.NotificationReduction
import io.github.yurisismotto.omnibridge.notifications.NotificationText
import io.github.yurisismotto.omnibridge.notifications.PlatformNotification
import io.github.yurisismotto.omnibridge.proto.capabilities.NotificationCategory
import io.github.yurisismotto.omnibridge.proto.capabilities.NotificationImportance
import io.github.yurisismotto.omnibridge.proto.capabilities.NotificationPrivacy
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Text rules, enum mapping and the privacy reduction.
 *
 * The reduction tests are the ones that matter most: they assert that content
 * withheld by policy is not present in the value the encoder is handed, rather
 * than that it is present and flagged. There is no later step that could
 * forget to apply a flag, because there is nothing to forget.
 */
class NotificationTextTest {

    // -- sanitisation --------------------------------------------------------

    @Test
    fun `ordinary text passes through unchanged`() {
        assertEquals(
            "FIXTURE TITLE",
            NotificationText.reduce("FIXTURE TITLE", NotificationLimits.MAX_TITLE_BYTES),
        )
    }

    @Test
    fun `null and empty are the empty string`() {
        assertEquals("", NotificationText.reduce(null, 512))
        assertEquals("", NotificationText.reduce("", 512))
    }

    /**
     * NUL cannot be carried faithfully end to end — any consumer touching a C
     * string API truncates at it silently — so the receiver refuses a string
     * containing one. Stripping it here is the same rule seen from the source
     * end: nothing this function emits could be rejected for it.
     */
    @Test
    fun `NUL is stripped`() {
        val cleaned = NotificationText.reduce("FIX\u0000TURE", 512)
        assertEquals("FIXTURE", cleaned)
        assertFalse(cleaned.contains('\u0000'))
    }

    /**
     * Newline and tab survive: a multi-line notification body is ordinary, and
     * rewriting it would corrupt user content to satisfy a rule aimed at
     * terminal escapes.
     */
    @Test
    fun `newline and tab survive and other controls do not`() {
        assertEquals("a\nb\tc", NotificationText.reduce("a\nb\tc", 512))
        // ESC, BEL, backspace, DEL and a C1 control all go: these are what
        // forge terminal output and notification text downstream.
        assertEquals("ab", NotificationText.reduce("a\u001bb", 512))
        assertEquals("ab", NotificationText.reduce("a\u0007b", 512))
        assertEquals("ab", NotificationText.reduce("a\u0008b", 512))
        assertEquals("ab", NotificationText.reduce("a\u007fb", 512))
        assertEquals("ab", NotificationText.reduce("a\u009bb", 512))
        // Carriage return is a control too, and goes with them.
        assertEquals("ab", NotificationText.reduce("a\rb", 512))
    }

    // -- truncation ----------------------------------------------------------

    @Test
    fun `text within the limit is not truncated`() {
        val text = "x".repeat(NotificationLimits.MAX_BODY_BYTES)
        assertEquals(text, NotificationText.reduce(text, NotificationLimits.MAX_BODY_BYTES))
        assertFalse(NotificationText.exceeds(text, NotificationLimits.MAX_BODY_BYTES))
    }

    @Test
    fun `oversize text is truncated with a visible ellipsis`() {
        val text = "x".repeat(NotificationLimits.MAX_BODY_BYTES + 100)
        val reduced = NotificationText.reduce(text, NotificationLimits.MAX_BODY_BYTES)
        assertTrue(reduced.endsWith(NotificationText.ELLIPSIS))
        assertTrue(
            reduced.toByteArray(Charsets.UTF_8).size <= NotificationLimits.MAX_BODY_BYTES,
        )
    }

    /**
     * The limits are in UTF-8 bytes because that is what crosses the wire, and
     * a cut mid-sequence would produce bytes that are not text.
     */
    @Test
    fun `truncation lands on a character boundary`() {
        // Three-byte characters: 20 of them is 60 bytes, and a 50-byte limit
        // cannot land on a multiple of three without splitting one.
        val text = "中".repeat(20)
        val reduced = NotificationText.truncateUtf8(text, 50)
        val bytes = reduced.toByteArray(Charsets.UTF_8)
        assertTrue(bytes.size <= 50)
        // Round-tripping is lossless exactly when nothing was split.
        assertEquals(reduced, String(bytes, Charsets.UTF_8))
        assertTrue(reduced.endsWith(NotificationText.ELLIPSIS))
    }

    @Test
    fun `a surrogate pair is never split`() {
        // U+1F600, four UTF-8 bytes, two Kotlin chars.
        val text = "😀".repeat(10)
        val reduced = NotificationText.truncateUtf8(text, 15)
        assertEquals(reduced, String(reduced.toByteArray(Charsets.UTF_8), Charsets.UTF_8))
        assertFalse(reduced.any { it.isSurrogate() && reduced.indexOf(it) == reduced.length - 1 })
    }

    @Test
    fun `a limit smaller than the ellipsis yields nothing`() {
        assertEquals("", NotificationText.truncateUtf8("abcdef", 2))
    }

    // -- enum mapping --------------------------------------------------------

    @Test
    fun `importance maps as the schema documents`() {
        assertEquals(
            NotificationImportance.NOTIFICATION_IMPORTANCE_LOW,
            NotificationMapping.importance(NotificationMapping.ANDROID_IMPORTANCE_MIN),
        )
        assertEquals(
            NotificationImportance.NOTIFICATION_IMPORTANCE_LOW,
            NotificationMapping.importance(NotificationMapping.ANDROID_IMPORTANCE_LOW),
        )
        assertEquals(
            NotificationImportance.NOTIFICATION_IMPORTANCE_NORMAL,
            NotificationMapping.importance(NotificationMapping.ANDROID_IMPORTANCE_DEFAULT),
        )
        assertEquals(
            NotificationImportance.NOTIFICATION_IMPORTANCE_HIGH,
            NotificationMapping.importance(NotificationMapping.ANDROID_IMPORTANCE_HIGH),
        )
        assertEquals(
            NotificationImportance.NOTIFICATION_IMPORTANCE_HIGH,
            NotificationMapping.importance(NotificationMapping.ANDROID_IMPORTANCE_MAX),
        )
    }

    @Test
    fun `an unknown or unavailable importance is normal`() {
        assertEquals(
            NotificationImportance.NOTIFICATION_IMPORTANCE_NORMAL,
            NotificationMapping.importance(PlatformNotification.IMPORTANCE_UNKNOWN),
        )
        assertEquals(
            NotificationImportance.NOTIFICATION_IMPORTANCE_NORMAL,
            NotificationMapping.importance(99),
        )
    }

    /**
     * An unknown or unset visibility must never decay to the most permissive
     * value. A privacy hint that fails open is not a hint worth carrying.
     */
    @Test
    fun `an unknown visibility is private, never public`() {
        for (value in listOf(-99, 2, 7, Int.MAX_VALUE, Int.MIN_VALUE)) {
            assertEquals(
                NotificationPrivacy.NOTIFICATION_PRIVACY_PRIVATE,
                NotificationMapping.privacy(value),
            )
            assertNotEquals(
                NotificationPrivacy.NOTIFICATION_PRIVACY_PUBLIC,
                NotificationMapping.privacy(value),
            )
        }
    }

    @Test
    fun `the three known visibilities map exactly`() {
        assertEquals(
            NotificationPrivacy.NOTIFICATION_PRIVACY_PUBLIC,
            NotificationMapping.privacy(NotificationMapping.VISIBILITY_PUBLIC),
        )
        assertEquals(
            NotificationPrivacy.NOTIFICATION_PRIVACY_PRIVATE,
            NotificationMapping.privacy(NotificationMapping.VISIBILITY_PRIVATE),
        )
        assertEquals(
            NotificationPrivacy.NOTIFICATION_PRIVACY_SECRET,
            NotificationMapping.privacy(NotificationMapping.VISIBILITY_SECRET),
        )
    }

    @Test
    fun `categories project onto the portable subset`() {
        assertEquals(
            NotificationCategory.NOTIFICATION_CATEGORY_MESSAGE,
            NotificationMapping.category("msg"),
        )
        assertEquals(
            NotificationCategory.NOTIFICATION_CATEGORY_CALL,
            NotificationMapping.category("missed_call"),
        )
        assertEquals(
            NotificationCategory.NOTIFICATION_CATEGORY_SYSTEM,
            NotificationMapping.category("sys"),
        )
        assertEquals(
            NotificationCategory.NOTIFICATION_CATEGORY_UNSPECIFIED,
            NotificationMapping.category(null),
        )
        // A category Android adds later is OTHER, never dropped and never
        // guessed at.
        assertEquals(
            NotificationCategory.NOTIFICATION_CATEGORY_OTHER,
            NotificationMapping.category("a_future_category"),
        )
    }

    // -- the privacy reduction -----------------------------------------------

    private fun notification() = PlatformNotification(
        platformKey = "0|example.fixture.app|1|null|10123",
        packageName = "example.fixture.app",
        secondaryProfile = false,
        postedAtUnixMs = 1_700_000_000_000L,
        ongoing = false,
        clearable = true,
        visibility = NotificationMapping.VISIBILITY_PRIVATE,
        androidImportance = NotificationMapping.ANDROID_IMPORTANCE_DEFAULT,
        category = "msg",
        groupKey = null,
        groupSummary = false,
        title = "FIXTURE TITLE",
        body = "FIXTURE BODY",
        hasProgress = false,
        progressCurrent = 0,
        progressMax = 0,
        progressIndeterminate = false,
    )

    @Test
    fun `an unlocked phone sends everything`() {
        val reduced = NotificationReduction.reduce(
            notification(),
            NotificationPolicy(whenSourceLocked = LockPolicy.APP_ONLY),
            sourceLocked = false,
        )!!
        assertEquals("FIXTURE TITLE", reduced.title)
        assertEquals("FIXTURE BODY", reduced.body)
        assertFalse(reduced.redacted)
    }

    /**
     * The default, and the important one: a locked phone sends the app name
     * and nothing else. The title and body are **absent from the value**, not
     * present with a flag beside them — so there is nothing downstream that
     * could send them by mistake.
     */
    @Test
    fun `a locked phone under the default policy withholds title and body`() {
        val reduced = NotificationReduction.reduce(
            notification(),
            NotificationPolicy(),
            sourceLocked = true,
        )!!
        assertEquals("", reduced.title)
        assertEquals("", reduced.body)
        assertTrue(reduced.redacted)
        assertEquals(LockPolicy.APP_ONLY, LockPolicy.DEFAULT)
    }

    @Test
    fun `a locked phone under suppress sends nothing at all`() {
        assertNull(
            NotificationReduction.reduce(
                notification(),
                NotificationPolicy(whenSourceLocked = LockPolicy.SUPPRESS),
                sourceLocked = true,
            ),
        )
    }

    @Test
    fun `a locked phone under full sends everything`() {
        val reduced = NotificationReduction.reduce(
            notification(),
            NotificationPolicy(whenSourceLocked = LockPolicy.FULL),
            sourceLocked = true,
        )!!
        assertEquals("FIXTURE BODY", reduced.body)
        assertFalse(reduced.redacted)
    }

    /**
     * `redacted` is a privacy signal, not a length one. A very long body is
     * truncated and **not** marked redacted, because the sink would otherwise
     * tell the user their message had been withheld when it had merely been
     * shortened.
     */
    @Test
    fun `a truncation is not a redaction`() {
        val long = PlatformNotification(
            platformKey = "0|example.fixture.app|1|null|10123",
            packageName = "example.fixture.app",
            secondaryProfile = false,
            postedAtUnixMs = 0,
            ongoing = false,
            clearable = true,
            visibility = NotificationMapping.VISIBILITY_PRIVATE,
            androidImportance = NotificationMapping.ANDROID_IMPORTANCE_DEFAULT,
            category = null,
            groupKey = null,
            groupSummary = false,
            title = "x".repeat(NotificationLimits.MAX_TITLE_BYTES + 10),
            body = "y".repeat(NotificationLimits.MAX_BODY_BYTES + 10),
            hasProgress = false,
            progressCurrent = 0,
            progressMax = 0,
            progressIndeterminate = false,
        )
        val reduced = NotificationReduction.reduce(
            long,
            NotificationPolicy(),
            sourceLocked = false,
        )!!
        assertFalse(reduced.redacted)
        assertTrue(reduced.title.endsWith(NotificationText.ELLIPSIS))
        assertTrue(reduced.body.toByteArray(Charsets.UTF_8).size <= NotificationLimits.MAX_BODY_BYTES)
    }

    /**
     * Unlocking is not retroactive, and it cannot be: the reduction produces a
     * value that never held the content, so there is nothing anywhere to
     * deliver later. Asserted on the type — it has three fields and none of
     * them could hold a withheld body.
     */
    @Test
    fun `a reduced notification has nowhere to keep withheld content`() {
        val reduced = NotificationReduction.reduce(
            notification(),
            NotificationPolicy(),
            sourceLocked = true,
        )!!
        val rendered = reduced.toString()
        assertFalse(rendered.contains("FIXTURE"))
        assertTrue(rendered.contains("titleBytes=0"))
        assertTrue(rendered.contains("bodyBytes=0"))
    }

    // -- the rendering rule --------------------------------------------------

    /**
     * A `PlatformNotification` is not a data class, so nothing it reaches — a
     * log line, an exception message, a test failure — can print its title or
     * body.
     */
    @Test
    fun `a platform notification never renders its content`() {
        val rendered = notification().toString()
        assertFalse(rendered.contains("FIXTURE TITLE"))
        assertFalse(rendered.contains("FIXTURE BODY"))
        // And not its platform key either: that is `userId|pkg|id|tag|uid`.
        assertFalse(rendered.contains("10123"))
        // The package is permitted: it is already transmitted as `app_id`.
        assertTrue(rendered.contains("example.fixture.app"))
    }
}
