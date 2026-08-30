package io.github.yurisismotto.anyflow

import io.github.yurisismotto.anyflow.files.Filenames
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The filename sanitizer.
 *
 * Deliberately the same cases as `filename::tests` in the Rust crate. A
 * divergence would mean one device accepting a name the other refuses, which
 * is exactly the quiet asymmetry a path-traversal bug lives in.
 */
class FilenamesTest {

    @Test
    fun `an ordinary name is kept verbatim`() {
        assertEquals("photo.jpg", Filenames.sanitize("photo.jpg"))
        assertEquals("Report 2026.pdf", Filenames.sanitize("Report 2026.pdf"))
    }

    @Test
    fun `a unix traversal keeps only the basename`() {
        assertEquals("passwd", Filenames.sanitize("../../etc/passwd"))
        assertEquals(
            "authorized_keys",
            Filenames.sanitize("../../../../root/.ssh/authorized_keys"),
        )
    }

    @Test
    fun `a windows traversal keeps only the basename`() {
        assertEquals("cmd.exe", Filenames.sanitize("..\\..\\windows\\system32\\cmd.exe"))
        assertEquals("baz.txt", Filenames.sanitize("../foo\\..\\bar/baz.txt"))
    }

    @Test
    fun `an absolute path keeps only the basename`() {
        assertEquals("shadow", Filenames.sanitize("/etc/shadow"))
        assertEquals(".bashrc", Filenames.sanitize("/home/yuri/.bashrc"))
        assertEquals("win.ini", Filenames.sanitize("C:\\Windows\\win.ini"))
    }

    @Test
    fun `a name that is only a path is rejected`() {
        assertNull(Filenames.sanitize("/"))
        assertNull(Filenames.sanitize("../../"))
        assertNull(Filenames.sanitize("/etc/"))
        assertNull(Filenames.sanitize(""))
    }

    @Test
    fun `dot directories are rejected`() {
        assertNull(Filenames.sanitize("."))
        assertNull(Filenames.sanitize(".."))
        assertNull(Filenames.sanitize("..."))
        assertNull(Filenames.sanitize("...."))
    }

    @Test
    fun `a NUL byte cannot survive`() {
        // The danger is a name the JVM sees as one thing and a syscall, or a
        // downstream C program, sees as another.
        assertEquals("safe.txt.sh", Filenames.sanitize("safe.txt\u0000.sh"))
        assertNull(Filenames.sanitize("\u0000"))
    }

    @Test
    fun `control characters are stripped`() {
        // A newline in a filename forges a second line of notification text.
        assertEquals("abcd.txt", Filenames.sanitize("a\nb\rc\td.txt"))
        // An ANSI escape would repaint a terminal on the other side.
        assertEquals("x[2Ky.txt", Filenames.sanitize("x\u001b[2Ky.txt"))
    }

    @Test
    fun `trailing dots and spaces go`() {
        // Windows and SMB trim these silently, so `evil.txt.` and `evil.txt`
        // are the same file there.
        assertEquals("evil.txt", Filenames.sanitize("evil.txt."))
        assertEquals("evil.txt", Filenames.sanitize("evil.txt   "))
        assertEquals("evil.txt", Filenames.sanitize("evil.txt . . "))
    }

    @Test
    fun `windows device names are rejected`() {
        assertNull(Filenames.sanitize("CON"))
        assertNull(Filenames.sanitize("con.txt"))
        assertNull(Filenames.sanitize("NUL.jpg"))
        assertNull(Filenames.sanitize("lpt9.tar.gz"))
        // Only the exact stems are reserved.
        assertEquals("console.log", Filenames.sanitize("console.log"))
    }

    @Test
    fun `a leading dot is allowed`() {
        assertEquals(".gitignore", Filenames.sanitize(".gitignore"))
    }

    @Test
    fun `a long name is capped on a character boundary keeping the extension`() {
        val long = "é".repeat(400) + ".jpg"
        val out = requireNotNull(Filenames.sanitize(long))
        assertTrue(
            "got ${out.toByteArray(Charsets.UTF_8).size} bytes",
            out.toByteArray(Charsets.UTF_8).size <= Filenames.MAX_FILENAME_BYTES,
        )
        assertTrue(out.endsWith(".jpg"))
        // The cut must not have split a multi-byte character.
        assertTrue(!out.contains('\uFFFD'))
        assertEquals(out, String(out.toByteArray(Charsets.UTF_8), Charsets.UTF_8))
    }

    @Test
    fun `a long name with no extension is still capped`() {
        val out = requireNotNull(Filenames.sanitize("a".repeat(1000)))
        assertEquals(Filenames.MAX_FILENAME_BYTES, out.length)
    }

    @Test
    fun `an absurd extension is not allowed to eat the whole budget`() {
        val long = "a".repeat(300) + "." + "b".repeat(300)
        val out = requireNotNull(Filenames.sanitize(long))
        assertEquals(Filenames.MAX_FILENAME_BYTES, out.toByteArray(Charsets.UTF_8).size)
    }

    @Test
    fun `the result never contains a separator`() {
        val hostile = listOf(
            "../../etc/passwd",
            "..\\..\\x",
            "/a/b/c",
            "a/b\\c/d.txt",
            "\u0000/../x",
        )
        for (raw in hostile) {
            val name = Filenames.sanitize(raw) ?: continue
            assertTrue("$raw -> $name", !name.contains('/'))
            assertTrue("$raw -> $name", !name.contains('\\'))
            assertTrue("$raw -> $name", name != "." && name != "..")
        }
    }
}
