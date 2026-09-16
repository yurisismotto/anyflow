package io.github.yurisismotto.anyflow

import io.github.yurisismotto.anyflow.capability.BatteryCapability
import io.github.yurisismotto.anyflow.proto.capabilities.BatteryState
import io.github.yurisismotto.anyflow.proto.capabilities.ChargingState
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Test

/**
 * The phone's half of the U2 P2 battery-absence contract.
 *
 * The desktop defect was that a Linux machine with no battery answered
 * UPower's `DisplayDevice` anyway and sent `percentage = 0`, which this app
 * then rendered — correctly, for what it was given — as "Battery 0 percent".
 * The fix is on the desktop: a battery-less machine now sends no frame at
 * all, and no frame means `remoteReading()` stays null and the card draws no
 * battery pill.
 *
 * No production code changed here. What these tests pin is the boundary that
 * makes that fix safe to rely on:
 *
 * * a frame that *does* arrive saying 0% is a real flat battery and must be
 *   shown as 0%, never confused with the absence above;
 * * absence is expressed by the frame not arriving, not by a sentinel value
 *   inside one.
 */
class BatteryReadingTest {

    private fun frame(percentage: Int, state: ChargingState): BatteryCapability.Reading =
        BatteryCapability.decode(
            BatteryState.newBuilder()
                .setPercentage(percentage)
                .setChargingState(state)
                .setTimestampUnixMs(1_789_575_511_000L)
                .build()
                .toByteString(),
        )

    /**
     * The mandatory positive case. A desktop whose battery is genuinely empty
     * sends 0%, and the phone must show 0% — so no fix on either side may
     * treat zero as "no battery".
     */
    @Test
    fun `a real zero percent battery decodes as zero, not as absence`() {
        val reading = frame(0, ChargingState.CHARGING_STATE_NOT_CHARGING)
        assertEquals(0, reading.percentage)
        assertEquals(ChargingState.CHARGING_STATE_NOT_CHARGING, reading.chargingState)
    }

    /** An ordinary reading still decodes unchanged. */
    @Test
    fun `an ordinary battery decodes unchanged`() {
        val reading = frame(77, ChargingState.CHARGING_STATE_DISCHARGING)
        assertEquals(77, reading.percentage)
        assertEquals(ChargingState.CHARGING_STATE_DISCHARGING, reading.chargingState)
    }

    /** A full battery, and the upper bound of the wire range. */
    @Test
    fun `a full battery decodes at the range boundary`() {
        val reading = frame(100, ChargingState.CHARGING_STATE_FULL)
        assertEquals(100, reading.percentage)
    }

    /**
     * A desktop that is plugged in but holding below a charge threshold
     * reports `NOT_CHARGING`. This is the state the physical Fedora host sits
     * in, so it is the one the positive regression actually exercises.
     */
    @Test
    fun `not charging is a distinct state from unspecified`() {
        assertEquals(
            ChargingState.CHARGING_STATE_NOT_CHARGING,
            frame(77, ChargingState.CHARGING_STATE_NOT_CHARGING).chargingState,
        )
        assertEquals(
            ChargingState.CHARGING_STATE_UNSPECIFIED,
            frame(77, ChargingState.CHARGING_STATE_UNSPECIFIED).chargingState,
        )
    }

    /**
     * The range check stays at the capability boundary: a peer cannot push a
     * value that a progress bar would then have to defend itself against.
     */
    @Test
    fun `an out of range percentage is rejected`() {
        assertThrows(IllegalArgumentException::class.java) {
            frame(101, ChargingState.CHARGING_STATE_DISCHARGING)
        }
    }
}
