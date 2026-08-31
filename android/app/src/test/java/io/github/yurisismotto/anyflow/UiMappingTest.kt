package io.github.yurisismotto.anyflow

import io.github.yurisismotto.anyflow.capability.BatteryCapability
import io.github.yurisismotto.anyflow.capability.ClipboardCapability
import io.github.yurisismotto.anyflow.capability.FilesCapability
import io.github.yurisismotto.anyflow.clipboard.ClipboardPolicy
import io.github.yurisismotto.anyflow.files.TransferState
import io.github.yurisismotto.anyflow.identity.Fingerprint
import io.github.yurisismotto.anyflow.store.TrustStore
import io.github.yurisismotto.anyflow.ui.UiMapping
import io.github.yurisismotto.anyflow.ui.theme.AnyFlowStatus
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The rules the UI applies, tested without a device.
 *
 * These are the assertions that would otherwise need an instrumented run —
 * and an instrumented run uninstalls the app, which destroys the Keystore key
 * and costs a manual re-pairing every time. Keeping the decisions in plain
 * functions is what makes them cheap to check.
 */
class UiMappingTest {

    private fun peer(
        vararg capabilities: String,
        policy: ClipboardPolicy = ClipboardPolicy(),
    ) = TrustStore.TrustedPeer(
        deviceId = "d0",
        deviceName = "Fedora",
        fingerprint = Fingerprint.fromHex("00".repeat(32))!!,
        pairedAtUnix = 0,
        grantedCapabilities = capabilities.toSet(),
        addresses = emptyList(),
        clipboardPolicy = policy,
    )

    // ---- status mapping --------------------------------------------------

    @Test
    fun `a live session reads as connected`() {
        assertEquals(
            AnyFlowStatus.Connected,
            UiMapping.statusFor(AnyFlowApp.ConnectionState.Idle, connected = true),
        )
    }

    @Test
    fun `a paired but idle peer reads as available, not disconnected`() {
        // "Disconnected" reads as a fault. A paired device with no session is
        // simply resting, and the card should not imply something is wrong.
        assertEquals(
            AnyFlowStatus.Available,
            UiMapping.statusFor(AnyFlowApp.ConnectionState.Idle, connected = false),
        )
    }

    @Test
    fun `retrying reads as connecting rather than failed`() {
        assertEquals(
            AnyFlowStatus.Connecting,
            UiMapping.statusFor(
                AnyFlowApp.ConnectionState.Retrying("network", 3),
                connected = false,
            ),
        )
    }

    @Test
    fun `an error state is surfaced as an error`() {
        assertEquals(
            AnyFlowStatus.Error,
            UiMapping.statusFor(AnyFlowApp.ConnectionState.Error("boom"), connected = false),
        )
    }

    // ---- capability and policy gating ------------------------------------

    @Test
    fun `sending the clipboard needs the grant, the direction and a session`() {
        val full = peer(ClipboardCapability.ID)
        assertTrue(UiMapping.canSendClipboard(full, connected = true))

        // Any one of the three missing is enough to disable the action.
        assertFalse(
            "a session alone is not authorisation",
            UiMapping.canSendClipboard(peer(), connected = true),
        )
        assertFalse(
            "the grant alone is not a connection",
            UiMapping.canSendClipboard(full, connected = false),
        )
        assertFalse(
            "the grant does not override the send direction being off",
            UiMapping.canSendClipboard(
                peer(ClipboardCapability.ID, policy = ClipboardPolicy(allowSend = false)),
                connected = true,
            ),
        )
    }

    @Test
    fun `a battery grant does not authorise the clipboard`() {
        // Capabilities are independent. Granting one must never widen another.
        assertFalse(UiMapping.canSendClipboard(peer(BatteryCapability.ID), connected = true))
        assertFalse(UiMapping.canSendFiles(peer(BatteryCapability.ID), connected = true))
    }

    @Test
    fun `sending files needs the files grant and a session`() {
        assertTrue(UiMapping.canSendFiles(peer(FilesCapability.ID), connected = true))
        assertFalse(UiMapping.canSendFiles(peer(FilesCapability.ID), connected = false))
        assertFalse(UiMapping.canSendFiles(peer(ClipboardCapability.ID), connected = true))
    }

    @Test
    fun `auto-apply cannot be enabled while receiving is off`() {
        assertTrue(UiMapping.autoReceiveEnabled(ClipboardPolicy(allowReceive = true)))
        assertFalse(UiMapping.autoReceiveEnabled(ClipboardPolicy(allowReceive = false)))
    }

    @Test
    fun `no auto-send switch is offered on this platform`() {
        // Offering a toggle that cannot work is worse than not offering one:
        // the person turns it on and quietly gets nothing.
        assertFalse(UiMapping.autoSendOfferable())
    }

    @Test
    fun `the receive behaviour is described accurately for each policy`() {
        assertTrue(
            UiMapping.receiveBehaviour(ClipboardPolicy(allowReceive = false))
                .contains("refused"),
        )
        assertTrue(
            UiMapping.receiveBehaviour(
                ClipboardPolicy(allowReceive = true, autoReceive = true),
            ).contains("replaces"),
        )
        assertTrue(
            UiMapping.receiveBehaviour(
                ClipboardPolicy(allowReceive = true, autoReceive = false),
            ).contains("waits"),
        )
    }

    // ---- transfer states -------------------------------------------------

    @Test
    fun `a cancelled transfer is not shown as a failure`() {
        assertNotEquals(
            AnyFlowStatus.Error,
            UiMapping.transferStatus(TransferState.CANCELLED),
        )
        assertEquals(AnyFlowStatus.Error, UiMapping.transferStatus(TransferState.FAILED))
    }

    @Test
    fun `every transfer state maps to a status`() {
        TransferState.entries.forEach { state ->
            // An unmapped state would throw; this is what stops a new one from
            // being added without deciding how it reads.
            UiMapping.transferStatus(state)
        }
    }

    // ---- the status language itself --------------------------------------

    @Test
    fun `no status is carried by colour alone`() {
        AnyFlowStatus.entries.forEach { status ->
            assertTrue(
                "${status.name} has no label",
                status.label.isNotBlank(),
            )
            assertTrue(
                "${status.name} has no icon",
                status.icon != 0,
            )
        }
    }

    // ---- lazy list keys --------------------------------------------------

    /**
     * The regression this file exists for.
     *
     * A pending clip is identified by the fingerprint of the peer it came
     * from; a device card by the fingerprint of the peer it *is*. Before the
     * keys were namespaced those collided, Compose threw
     * `IllegalArgumentException: Key ... was already used`, and the home
     * screen crashed on every single received clip. Caught on hardware, not
     * in a test — so here is the test.
     */
    @Test
    fun `a clip and its own peer do not share a list key`() {
        val fingerprint = "df65d3e4ba28edf9"
        assertNotEquals(
            UiMapping.clipKey(fingerprint),
            UiMapping.peerKey(fingerprint),
        )
    }

    @Test
    fun `an offer and a transfer with the same id do not share a list key`() {
        // An incoming offer becomes a transfer, and both can be on screen at
        // once carrying the same transfer id.
        val id = "0123abcd"
        assertNotEquals(UiMapping.offerKey(id), UiMapping.transferKey(id))
    }

    @Test
    fun `every key kind stays distinct for one identifier`() {
        val id = "same-identifier"
        val keys = listOf(
            UiMapping.offerKey(id),
            UiMapping.clipKey(id),
            UiMapping.peerKey(id),
            UiMapping.transferKey(id),
        )
        assertEquals(
            "each kind of list item must produce its own key",
            keys.size,
            keys.toSet().size,
        )
    }

    @Test
    fun `destructive actions are declared in one place`() {
        assertTrue(UiMapping.DESTRUCTIVE_ACTIONS.contains("forget_device"))
    }
}
