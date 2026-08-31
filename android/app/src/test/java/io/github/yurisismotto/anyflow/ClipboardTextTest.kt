package io.github.yurisismotto.anyflow

import io.github.yurisismotto.anyflow.clipboard.ClipboardLimits
import io.github.yurisismotto.anyflow.clipboard.ClipboardRejected
import io.github.yurisismotto.anyflow.clipboard.ClipboardText
import io.github.yurisismotto.anyflow.clipboard.Redact
import io.github.yurisismotto.anyflow.clipboard.TextRejection
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The Kotlin half of the cross-language text contract.
 *
 * Every case here has a twin in `desktop/capabilities/clipboard/src/text.rs`.
 * The two implementations must accept and refuse exactly the same strings and
 * compute exactly the same hash, or a clip one platform sends is a clip the
 * other refuses.
 */
class ClipboardTextTest {

    private fun rejection(candidate: CharSequence?): TextRejection? =
        (ClipboardText.validate(candidate).exceptionOrNull() as? ClipboardRejected)?.rejection

    @Test
    fun `ascii round trips unchanged`() {
        val text = ClipboardText.validate("hello").getOrThrow()
        assertEquals("hello", text.text)
        assertEquals(5, text.byteLength)
    }

    @Test
    fun `unicode is never modified`() {
        // Portuguese, Spanish, emoji (a flag is two codepoints, a family is a
        // ZWJ sequence), CJK, mixed line endings, and significant whitespace.
        val cases = listOf(
            "olá, ação, coração",
            "¿cómo estás? ñandú",
            "🇧🇷 🎉 👨‍👩‍👧‍👦",
            "日本語のテキスト 中文 한국어",
            "line1\nline2\r\nline3\ttabbed",
            "  leading and trailing  ",
        )
        for (case in cases) {
            val text = ClipboardText.validate(case).getOrThrow()
            assertEquals("content was modified", case, text.text)
            assertEquals(
                "byte length changed",
                case.toByteArray(Charsets.UTF_8).size,
                text.byteLength,
            )
        }
    }

    @Test
    fun `empty and null are refused`() {
        assertEquals(TextRejection.Empty, rejection(""))
        assertEquals(TextRejection.Empty, rejection(null))
    }

    @Test
    fun `nul is refused explicitly`() {
        // Interior and trailing: a C-string consumer treats them differently
        // and neither is acceptable. The desktop refuses both too.
        assertEquals(TextRejection.ContainsNul, rejection("abc\u0000def"))
        assertEquals(TextRejection.ContainsNul, rejection("abc\u0000"))
    }

    @Test
    fun `the size limit is a byte limit not a character limit`() {
        val max = "a".repeat(ClipboardLimits.MAX_TEXT_BYTES)
        assertTrue("the limit is inclusive", ClipboardText.validate(max).isSuccess)

        val over = "a".repeat(ClipboardLimits.MAX_TEXT_BYTES + 1)
        assertEquals(
            TextRejection.TooLarge(ClipboardLimits.MAX_TEXT_BYTES + 1),
            rejection(over),
        )

        // A 4-byte emoji that lands exactly on the boundary.
        val exact = "🎉".repeat(ClipboardLimits.MAX_TEXT_BYTES / 4)
        assertEquals(ClipboardLimits.MAX_TEXT_BYTES, exact.toByteArray(Charsets.UTF_8).size)
        assertTrue(ClipboardText.validate(exact).isSuccess)
        assertTrue(ClipboardText.validate(exact + "🎉").isFailure)
    }

    @Test
    fun `oversize is never truncated`() {
        val over = "a".repeat(ClipboardLimits.MAX_TEXT_BYTES + 100)
        // The property: there is no path that returns a shortened success.
        assertTrue(ClipboardText.validate(over).isFailure)
    }

    @Test
    fun `the hash is sha256 over utf8 bytes`() {
        // The same pinned vector the Rust suite asserts, so the two cannot
        // drift apart silently.
        val text = ClipboardText.validate("hello").getOrThrow()
        assertEquals(
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824",
            Redact.hex(text.hash),
        )

        val accented = ClipboardText.validate("olá 🇧🇷").getOrThrow()
        assertTrue(accented.hash.contentEquals(ClipboardText.contentHash("olá 🇧🇷")))
        assertFalse(accented.hash.contentEquals(ClipboardText.contentHash("ola 🇧🇷")))
    }

    @Test
    fun `the hash distinguishes clips that differ only in whitespace`() {
        val a = ClipboardText.validate("secret").getOrThrow()
        val b = ClipboardText.validate("secret ").getOrThrow()
        assertNotEquals(Redact.hex(a.hash), Redact.hex(b.hash))
    }

    @Test
    fun `toString never prints the content`() {
        val text = ClipboardText.validate("correct-horse-battery-staple").getOrThrow()
        val rendered = text.toString()
        assertFalse(
            "toString leaked clipboard content: $rendered",
            rendered.contains("correct-horse"),
        )
        assertTrue("it should still be useful", rendered.contains("bytes="))
    }

    @Test
    fun `a rejection message describes the size without echoing the content`() {
        val filler = "\u0007"
        val over = filler.repeat(ClipboardLimits.MAX_TEXT_BYTES + 1)
        val message = rejection(over)!!.describe()
        assertFalse(message.contains(filler))
        assertTrue(message.contains("${ClipboardLimits.MAX_TEXT_BYTES}"))
    }

    @Test
    fun `redaction helpers are short and survive short input`() {
        assertEquals("000fff", Redact.hex(byteArrayOf(0, 0x0f, 0xff.toByte())))
        assertEquals(
            "deadbeef",
            Redact.hashPrefix(
                byteArrayOf(0xde.toByte(), 0xad.toByte(), 0xbe.toByte(), 0xef.toByte(), 0x99.toByte()),
            ),
        )
        assertEquals("0102", Redact.eventPrefix(byteArrayOf(1, 2)))
    }
}
