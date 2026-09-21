package io.github.yurisismotto.omnibridge

import io.github.yurisismotto.omnibridge.notifications.LockPolicy
import io.github.yurisismotto.omnibridge.notifications.NotificationPolicy
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * What the notification policy stores, what it defaults to, and what it can
 * never hold.
 *
 * The defaults are a security control: a trust store written before this
 * capability existed, or by an older release, must load with mirroring
 * denied-by-default rather than with everything on. And the field set is the
 * persistence audit in miniature — package names are settings the person
 * chose, and there is no field here a title or a body could reach.
 */
class NotificationPolicyStorageTest {

    @Test
    fun `a fresh policy shares nothing`() {
        val policy = NotificationPolicy()
        assertTrue(policy.allowedApps.isEmpty())
        assertTrue(policy.knownApps.isEmpty())
        assertFalse(policy.includeWorkProfile)
        assertFalse(policy.includeOngoing)
        assertFalse(policy.allowDismissSync)
        assertEquals(LockPolicy.APP_ONLY, policy.whenSourceLocked)
        assertFalse(policy.allowsApp("com.example.chat"))
    }

    @Test
    fun `a record written before this capability existed loads with safe defaults`() {
        val policy = NotificationPolicy.fromJson(null)
        assertTrue(policy.allowedApps.isEmpty())
        assertFalse(policy.includeWorkProfile)
        assertFalse(policy.includeOngoing)
        assertFalse(policy.allowDismissSync)
        assertEquals(LockPolicy.APP_ONLY, policy.whenSourceLocked)
    }

    @Test
    fun `a record written before the picker existed has no known apps`() {
        // Which is what stops the very first opening of the picker on an
        // upgraded install from announcing that every app is new.
        val stored = NotificationPolicy(allowedApps = setOf("com.example.chat")).toJson()
        stored.remove("knownApps")
        assertTrue(NotificationPolicy.fromJson(stored).knownApps.isEmpty())
    }

    @Test
    fun `the app list and the baseline survive a round trip`() {
        val policy = NotificationPolicy(
            allowedApps = setOf("com.example.chat", "com.example.bank"),
            knownApps = setOf("com.example.chat", "com.example.bank", "com.example.maps"),
            whenSourceLocked = LockPolicy.FULL,
            includeOngoing = true,
        )
        val restored = NotificationPolicy.fromJson(JSONObject(policy.toJson().toString()))
        assertEquals(policy, restored)
    }

    @Test
    fun `an unrecognised lock policy falls back to app-only and never to full`() {
        val stored = NotificationPolicy().toJson().put("whenSourceLocked", "SHARE_EVERYTHING")
        assertEquals(LockPolicy.APP_ONLY, NotificationPolicy.fromJson(stored).whenSourceLocked)
    }

    @Test
    fun `the baseline is not an allow-list`() {
        // Recording that a person was *shown* an application is not a decision
        // to share it. Anything else would turn opening the picker into
        // selecting everything in it.
        val policy = NotificationPolicy(
            allowedApps = emptySet(),
            knownApps = setOf("com.example.chat"),
        )
        assertFalse(policy.allowsApp("com.example.chat"))
    }

    @Test
    fun `the denied policy shares nothing whatever is stored beside it`() {
        val denied = NotificationPolicy.DENIED
        assertFalse(denied.allowsApp("com.example.chat"))
        assertEquals(LockPolicy.SUPPRESS, denied.whenSourceLocked)
        assertTrue(denied.knownApps.isEmpty())
    }

    @Test
    fun `the description carries counts and never a package name`() {
        val policy = NotificationPolicy(
            allowedApps = setOf("com.example.chat"),
            knownApps = setOf("com.example.chat", "com.example.bank"),
        )
        val described = policy.describe()
        assertFalse(described.contains("com.example.chat"))
        assertFalse(described.contains("com.example.bank"))
        assertTrue(described.contains("apps=1"))
    }

    @Test
    fun `the stored form has exactly the documented fields`() {
        // The persistence audit, as a test. A field added here would be a
        // field that could hold something else, so adding one has to be a
        // deliberate change to this list.
        val json = NotificationPolicy().toJson()
        assertEquals(
            setOf(
                "allowMirror",
                "allowedApps",
                "knownApps",
                "includeWorkProfile",
                "includeOngoing",
                "whenSourceLocked",
                "allowDismissSync",
            ),
            json.keys().asSequence().toSet(),
        )
    }

    @Test
    fun `dismiss sync stays false and is not changed by anything else`() {
        // The stored default has to survive every other setting being changed,
        // or a person could end up with it on without ever having asked for
        // it. This is the assertion that catches a future edit which reaches
        // for `copy()` and passes the wrong argument.
        var policy = NotificationPolicy()
        policy = policy.copy(allowedApps = setOf("com.example.chat"))
        policy = policy.copy(whenSourceLocked = LockPolicy.FULL)
        policy = policy.copy(includeOngoing = true, includeWorkProfile = true)
        assertFalse(policy.allowDismissSync)
        assertFalse(NotificationPolicy.fromJson(policy.toJson()).allowDismissSync)
    }

    /**
     * And the converse: turning dismiss sync on changes **only** dismiss sync.
     *
     * The one control on the screen that lets a computer act on this phone
     * must not be a control that also changes what is shared with it.
     */
    @Test
    fun `enabling dismiss sync changes nothing else`() {
        val before = NotificationPolicy(
            allowedApps = setOf("com.example.chat"),
            knownApps = setOf("com.example.chat", "com.example.other"),
            includeOngoing = true,
            whenSourceLocked = LockPolicy.SUPPRESS,
        )
        val after = before.copy(allowDismissSync = true)

        assertTrue(after.allowDismissSync)
        assertEquals(before.allowMirror, after.allowMirror)
        assertEquals(before.allowedApps, after.allowedApps)
        assertEquals(before.knownApps, after.knownApps)
        assertEquals(before.includeOngoing, after.includeOngoing)
        assertEquals(before.includeWorkProfile, after.includeWorkProfile)
        assertEquals(before.whenSourceLocked, after.whenSourceLocked)

        // And it survives a round trip through the stored form.
        val stored = NotificationPolicy.fromJson(after.toJson())
        assertEquals(after, stored)
    }

    /** A stored `true` is honoured. Persistence is what the switch is for. */
    @Test
    fun `dismiss sync persists once it is turned on`() {
        val stored = NotificationPolicy(allowDismissSync = true).toJson()
        assertTrue(NotificationPolicy.fromJson(stored).allowDismissSync)
    }

    /**
     * A trust store written before this field existed reads as **off**.
     *
     * Never interpret a missing policy as enabled: an upgrade must not switch
     * on the one setting that lets another device act on this one.
     */
    @Test
    fun `a stored policy with no dismiss sync field reads as off`() {
        val old = JSONObject()
            .put("allowMirror", true)
            .put("allowedApps", JSONArray(listOf("com.example.chat")))
        val policy = NotificationPolicy.fromJson(old)
        assertTrue(policy.allowMirror)
        assertFalse(policy.allowDismissSync)

        // And an explicitly null one, and an unparseable one.
        assertFalse(NotificationPolicy.fromJson(JSONObject()).allowDismissSync)
        assertFalse(NotificationPolicy.fromJson(null).allowDismissSync)
    }

    /** A revoked peer's policy permits nothing, whatever was stored. */
    @Test
    fun `the denied policy never permits a dismissal`() {
        assertFalse(NotificationPolicy.DENIED.allowDismissSync)
        assertFalse(NotificationPolicy.DENIED.allowMirror)
    }
}
