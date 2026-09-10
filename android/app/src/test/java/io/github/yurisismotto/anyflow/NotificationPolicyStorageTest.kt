package io.github.yurisismotto.anyflow

import io.github.yurisismotto.anyflow.notifications.LockPolicy
import io.github.yurisismotto.anyflow.notifications.NotificationPolicy
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
        // N4 owns the runtime. Until then the stored default has to survive
        // every other setting being changed, or a person could end up with it
        // on without ever having asked for it.
        var policy = NotificationPolicy()
        policy = policy.copy(allowedApps = setOf("com.example.chat"))
        policy = policy.copy(whenSourceLocked = LockPolicy.FULL)
        policy = policy.copy(includeOngoing = true, includeWorkProfile = true)
        assertFalse(policy.allowDismissSync)
        assertFalse(NotificationPolicy.fromJson(policy.toJson()).allowDismissSync)
    }
}
