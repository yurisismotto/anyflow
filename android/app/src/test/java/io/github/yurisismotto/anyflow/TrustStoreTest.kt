package io.github.yurisismotto.anyflow

import io.github.yurisismotto.anyflow.store.TrustStore
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Covers the parts of the trust store that are pure logic.
 *
 * The persistence path (`TrustStore(context)`) needs a real `Context`, a
 * `filesDir` and the platform's `org.json`, none of which exist in a local JVM
 * unit test. That path is covered on-device rather than here; see the
 * technical-debt note in `android/README.md`.
 */
class TrustStoreTest {

    @Test
    fun `device ids are 128 random bits, not a hardware identifier`() {
        val a = TrustStore.randomDeviceId()
        val b = TrustStore.randomDeviceId()
        assertEquals(32, a.length)
        assertTrue(a.all { it in '0'..'9' || it in 'a'..'f' })
        assertNotEquals(a, b)
    }

    @Test
    fun `device names from the network cannot carry control characters`() {
        // A device name arrives from an untrusted peer and is rendered in a
        // notification. Control characters could forge or truncate that text.
        val hostile = "Laptop[31m\nFAKE\r"
        val clean = TrustStore.sanitizeDeviceName(hostile)
        assertFalse(clean.any { it.isISOControl() })
        assertEquals("Laptop[31mFAKE", clean)
    }

    @Test
    fun `device names are length capped`() {
        assertEquals(64, TrustStore.sanitizeDeviceName("x".repeat(500)).length)
    }

    @Test
    fun `an ordinary name survives untouched`() {
        assertEquals("Yuri's ThinkPad", TrustStore.sanitizeDeviceName("Yuri's ThinkPad"))
    }
}
