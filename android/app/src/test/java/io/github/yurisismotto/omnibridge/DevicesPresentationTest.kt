package io.github.yurisismotto.omnibridge

import io.github.yurisismotto.omnibridge.ui.UiMapping
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * What the Devices screen says *underneath* the device list.
 *
 * The screen used to end with a "Connection" card carrying a status badge,
 * a Connect and a Disconnect — all three of which the selected device's own
 * card already showed. Two controls that appear to drive one session invite
 * the reading that stopping one leaves the other running.
 *
 * The device card is now the only place a session is reported, started or
 * stopped. [UiMapping.connectionNotice] is what decides whether anything
 * still needs saying, and this is what holds it to that rule.
 */
class DevicesPresentationTest {

    private val connected = OmniBridgeApp.ConnectionState.Connected("fedora", "149F 6B66")
    private val idle = OmniBridgeApp.ConnectionState.Idle
    private val connecting = OmniBridgeApp.ConnectionState.Connecting

    // ---- the duplication that had to go ----------------------------------

    /**
     * The states a device card fully expresses add nothing below it.
     *
     * This is the regression test for the duplication itself: if any of
     * these starts returning a notice again, the Devices screen is back to
     * saying "Connected" twice.
     */
    @Test
    fun `the states a device card already shows add nothing below the list`() {
        listOf(
            "connected" to connected,
            "idle" to idle,
            "connecting" to connecting,
        ).forEach { (name, state) ->
            assertNull(
                "$name must not repeat itself under the device list",
                UiMapping.connectionNotice(state, mustChoose = false),
            )
        }
    }

    /** In particular, a live session produces no second Connected block. */
    @Test
    fun `a live session produces no second connected block`() {
        val notice = UiMapping.connectionNotice(connected, mustChoose = false)
        assertNull(notice)
    }

    // ---- what genuinely could not fit on a card --------------------------

    /**
     * A retry keeps its countdown and its reason.
     *
     * A status badge has room for a word. "Reconnecting in 8s · the computer
     * stopped answering" is the sentence that tells someone the app is still
     * trying, and removing it to tidy the screen would hide something they
     * need.
     */
    @Test
    fun `a retry keeps its countdown and its reason`() {
        val retrying = OmniBridgeApp.ConnectionState.Retrying("Wi-Fi dropped", 8)
        val notice = UiMapping.connectionNotice(retrying, mustChoose = false)

        assertNotNull(notice)
        assertTrue("the countdown must survive", notice!!.title.contains("8"))
        assertEquals("Wi-Fi dropped", notice.body)
    }

    /**
     * A retry is not drawn as a fault.
     *
     * It is the app doing its job. Painting an ordinary Wi-Fi blip amber
     * would make a recoverable state look like a broken one.
     */
    @Test
    fun `a scheduled retry is not presented as a problem`() {
        val retrying = OmniBridgeApp.ConnectionState.Retrying("Wi-Fi dropped", 8)
        assertFalse(UiMapping.connectionNotice(retrying, mustChoose = false)!!.isProblem)
    }

    /** A terminal error keeps its message, and does read as a problem. */
    @Test
    fun `a terminal error keeps its message and reads as a problem`() {
        val error = OmniBridgeApp.ConnectionState.Error("could not find the computer")
        val notice = UiMapping.connectionNotice(error, mustChoose = false)

        assertNotNull(notice)
        assertEquals("could not find the computer", notice!!.body)
        assertTrue("a failure must wear the caution tone", notice.isProblem)
    }

    /**
     * "Several devices are paired" still has somewhere to be said.
     *
     * It is the one message that is not a property of any single card: the
     * answer is "choose one", which no individual row can state.
     */
    @Test
    fun `the choose-a-device hint survives in every quiet state`() {
        listOf(idle, connecting, connected).forEach { state ->
            val notice = UiMapping.connectionNotice(state, mustChoose = true)
            assertNotNull("mustChoose must still be said over $state", notice)
            assertTrue(notice!!.body!!, notice.body!!.contains("Connect"))
        }
    }

    /**
     * A failure outranks the choose-a-device hint.
     *
     * Both can be true at once. The one that just happened is the one worth
     * the line.
     */
    @Test
    fun `a failure outranks the choose-a-device hint`() {
        val error = OmniBridgeApp.ConnectionState.Error("handshake refused")
        val notice = UiMapping.connectionNotice(error, mustChoose = true)!!

        assertEquals("handshake refused", notice.body)
        assertTrue(notice.isProblem)
    }

    /** Every notice that is shown carries a title worth reading. */
    @Test
    fun `every notice shown has a non-empty title`() {
        val states = listOf(
            idle,
            connecting,
            connected,
            OmniBridgeApp.ConnectionState.Retrying("r", 3),
            OmniBridgeApp.ConnectionState.Error("e"),
        )
        for (state in states) {
            for (mustChoose in listOf(true, false)) {
                UiMapping.connectionNotice(state, mustChoose)?.let {
                    assertTrue("$state/$mustChoose has a blank title", it.title.isNotBlank())
                }
            }
        }
    }
}
