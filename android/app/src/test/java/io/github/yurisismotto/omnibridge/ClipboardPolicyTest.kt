package io.github.yurisismotto.omnibridge

import io.github.yurisismotto.omnibridge.clipboard.ClipboardCapabilities
import io.github.yurisismotto.omnibridge.clipboard.ClipboardPolicy
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The policy model, and the guarantee that an upgrade cannot widen it.
 *
 * Mirrors `omnibridge_core::clipboard_policy`'s suite: the two must agree on the
 * defaults, or a phone and a desktop that were both "just granted" would
 * disagree about whether anything is automatic.
 */
class ClipboardPolicyTest {

    @Test
    fun `automatic directions are off by default`() {
        val policy = ClipboardPolicy()
        assertTrue("a granted computer can be sent to by hand", policy.allowSend)
        assertTrue("a granted computer can be received from", policy.allowReceive)
        assertFalse("auto-send must never default on", policy.autoSend)
        assertFalse("auto-receive must never default on", policy.autoReceive)
        assertFalse(policy.mayAutoSend())
        assertFalse(policy.mayAutoReceive())
    }

    @Test
    fun `denied closes every direction`() {
        val policy = ClipboardPolicy.DENIED
        assertFalse(policy.allowSend)
        assertFalse(policy.allowReceive)
        assertFalse(policy.mayAutoSend())
        assertFalse(policy.mayAutoReceive())
    }

    @Test
    fun `withdrawing a direction defeats a stale automatic flag`() {
        // The state a person reaches by turning automation on and then
        // turning the whole direction off. The direction must win.
        val policy = ClipboardPolicy(
            allowSend = false,
            allowReceive = false,
            autoSend = true,
            autoReceive = true,
        )
        assertFalse(policy.mayAutoSend())
        assertFalse(policy.mayAutoReceive())
    }

    @Test
    fun `a policy round trips through json`() {
        val policy = ClipboardPolicy(
            allowSend = false,
            allowReceive = true,
            autoSend = false,
            autoReceive = true,
        )
        assertEquals(policy, ClipboardPolicy.fromJson(policy.toJson()))
    }

    @Test
    fun `a missing policy object deserialises to the documented defaults`() {
        // A trust store written before this capability existed.
        assertEquals(ClipboardPolicy(), ClipboardPolicy.fromJson(null))
        assertEquals(ClipboardPolicy(), ClipboardPolicy.fromJson(JSONObject()))
    }

    @Test
    fun `a partial policy object keeps automation off and directions on`() {
        // The dangerous upgrade: a field this build knows and the stored file
        // does not must not default to a *wider* value.
        val partial = JSONObject().put("allowSend", false)
        val policy = ClipboardPolicy.fromJson(partial)
        assertFalse(policy.allowSend)
        assertTrue(policy.allowReceive)
        assertFalse("automation must not appear from nowhere", policy.autoSend)
        assertFalse(policy.autoReceive)
    }

    @Test
    fun `describe is stable and carries no content`() {
        assertEquals(
            "send=on receive=on auto-send=off auto-receive=off",
            ClipboardPolicy().describe(),
        )
    }

    @Test
    fun `this platform reports that automatic sending is unsupported`() {
        // The gate ANDROID-CLIPBOARD-READ, as a constant the UI and the tests
        // both read. If a future Android release changed this, the value —
        // and this assertion — would change with evidence, not by assumption.
        assertFalse(ClipboardCapabilities.AUTO_SEND_SUPPORTED)
        assertTrue(ClipboardCapabilities.AUTO_SEND_REASON.contains("background"))
    }
}
