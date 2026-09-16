//! `battery.v1` — battery presence, and the capability seam that consumes it.
//!
//! The defect these tests exist for: UPower answers every property read on a
//! machine with no battery at all, so code that reads only `Percentage` and
//! `State` reports a battery-less tower or VM as a battery sitting at 0%, and
//! the phone faithfully renders "Battery 0 percent".
//!
//! The rule under test is therefore two-sided, and neither side may be
//! waived:
//!
//! * a machine with no battery reports **nothing** (not zero);
//! * a machine whose battery is genuinely flat still reports **0%**.
//!
//! No system bus is required: the D-Bus call is separated from the decision,
//! and these tests drive the decision directly with the property shapes real
//! machines produce.

use std::sync::Arc;

use anyflow_capability_battery::{
    presence, reading_of, BatteryCapability, BatteryPresence, BatteryReading, BatteryState,
    DisplayDevice, LocalBatterySource, CAPABILITY_ID,
};
use anyflow_core::capability::{Capability, CapabilityContext, OutboundMessage};
use anyflow_core::Fingerprint;
use anyflow_proto::v1::capabilities::ChargingState;

/// UPower `UP_DEVICE_KIND_*`.
const KIND_UNKNOWN: u32 = 0;
const KIND_LINE_POWER: u32 = 1;
const KIND_BATTERY: u32 = 2;
const KIND_MOUSE: u32 = 5;

/// UPower `UP_DEVICE_STATE_*`.
const STATE_UNKNOWN: u32 = 0;
const STATE_CHARGING: u32 = 1;
const STATE_DISCHARGING: u32 = 2;
const STATE_EMPTY: u32 = 3;
const STATE_FULLY_CHARGED: u32 = 4;
const STATE_PENDING_CHARGE: u32 = 5;

const NOW: i64 = 1_789_575_511_000;

/// A real laptop `DisplayDevice`, as captured from the Fedora host in §14 of
/// the P2 report: `Type=2 IsPresent=true PowerSupply=true`.
fn real_battery(percentage: f64, state: u32) -> DisplayDevice {
    DisplayDevice {
        kind: KIND_BATTERY,
        is_present: true,
        power_supply: true,
        percentage,
        state,
    }
}

/// The battery-less aggregate, as captured from the U2 VMs: UPower still
/// publishes `DisplayDevice` and still answers, saying it is nothing.
fn no_battery() -> DisplayDevice {
    DisplayDevice {
        kind: KIND_UNKNOWN,
        is_present: false,
        power_supply: false,
        percentage: 0.0,
        state: STATE_UNKNOWN,
    }
}

// ---------------------------------------------------------------------------
// Present batteries. Nothing here may regress: every one of these is a real
// battery and must keep being reported.
// ---------------------------------------------------------------------------

/// B1 — the ordinary case: a battery at 79%, discharging.
#[test]
fn present_battery_discharging_is_reported() {
    let device = real_battery(79.0, STATE_DISCHARGING);
    assert_eq!(presence(&device), BatteryPresence::Present);

    let reading = reading_of(&device, NOW).expect("a present battery reports");
    assert_eq!(reading.percentage, 79);
    assert_eq!(reading.charging_state, ChargingState::Discharging);
    assert_eq!(reading.peer_timestamp_unix_ms, NOW);
}

/// B2 — the same battery, charging.
#[test]
fn present_battery_charging_is_reported() {
    let reading = reading_of(&real_battery(79.0, STATE_CHARGING), NOW).expect("reports");
    assert_eq!(reading.percentage, 79);
    assert_eq!(reading.charging_state, ChargingState::Charging);
}

/// B3 — **the mandatory one** (§8). A battery that is physically present and
/// genuinely flat reads 0%, and must be reported as 0% rather than silenced.
///
/// This is what forbids the tempting one-line "fix" `if percentage == 0 {
/// None }`: it would pass every battery-less test above and silently delete
/// the reading a person most needs to see.
#[test]
fn present_battery_at_zero_percent_is_still_a_battery() {
    let device = real_battery(0.0, STATE_EMPTY);
    assert_eq!(
        presence(&device),
        BatteryPresence::Present,
        "0% is a charge level, not an absence"
    );

    let reading = reading_of(&device, NOW).expect("a flat battery still reports");
    assert_eq!(reading.percentage, 0);
}

/// B4 — a present battery whose charging state UPower does not know.
///
/// Presence and charge state are different questions. A charge controller
/// that has not settled must not make the battery vanish.
#[test]
fn present_battery_with_unknown_state_is_still_present() {
    let device = real_battery(55.0, STATE_UNKNOWN);
    assert_eq!(presence(&device), BatteryPresence::Present);

    let reading = reading_of(&device, NOW).expect("still reports");
    assert_eq!(reading.percentage, 55);
    assert_eq!(reading.charging_state, ChargingState::Unspecified);
}

/// B5 — a full battery.
#[test]
fn present_battery_full() {
    let reading = reading_of(&real_battery(100.0, STATE_FULLY_CHARGED), NOW).expect("reports");
    assert_eq!(reading.percentage, 100);
    assert_eq!(reading.charging_state, ChargingState::Full);
}

/// B6 — plugged in and held below a charge threshold.
///
/// `UP_DEVICE_STATE_PENDING_CHARGE` is the steady state on a laptop with
/// battery conservation enabled — including the physical host this branch was
/// verified on, which sat at `State=5` throughout.
#[test]
fn present_battery_pending_charge_is_not_charging() {
    let reading = reading_of(&real_battery(77.0, STATE_PENDING_CHARGE), NOW).expect("reports");
    assert_eq!(reading.percentage, 77);
    assert_eq!(reading.charging_state, ChargingState::NotCharging);
}

// ---------------------------------------------------------------------------
// Absent batteries. Each is a shape a real battery-less machine produces.
// ---------------------------------------------------------------------------

/// A1 — the certified reproduction: the U2 VMs' aggregate.
#[test]
fn battery_less_machine_reports_nothing() {
    let device = no_battery();
    assert_eq!(presence(&device), BatteryPresence::Absent);
    assert!(
        reading_of(&device, NOW).is_none(),
        "a battery-less machine must say nothing, not 0%"
    );
}

/// A2 — `IsPresent = false` alone is enough: a laptop with its battery pulled
/// out still reports `Type=Battery`.
#[test]
fn is_present_false_is_absent() {
    let device = DisplayDevice {
        is_present: false,
        ..real_battery(0.0, STATE_UNKNOWN)
    };
    assert_eq!(presence(&device), BatteryPresence::Absent);
    assert!(reading_of(&device, NOW).is_none());
}

/// A3 — `PowerSupply = false` alone is enough. This is the property that
/// separates a battery that powers the machine from a cell in a peripheral.
#[test]
fn power_supply_false_is_absent() {
    let device = DisplayDevice {
        power_supply: false,
        ..real_battery(64.0, STATE_DISCHARGING)
    };
    assert_eq!(presence(&device), BatteryPresence::Absent);
    assert!(
        reading_of(&device, NOW).is_none(),
        "a peripheral's cell is not this machine's battery"
    );
}

/// A4 — `Type` is not a battery. Covers the battery-less aggregate
/// (`UNKNOWN`), a line-power-only machine, and a peripheral that claims to be
/// present and charged.
#[test]
fn non_battery_kinds_are_absent() {
    for kind in [KIND_UNKNOWN, KIND_LINE_POWER, KIND_MOUSE] {
        let device = DisplayDevice {
            kind,
            ..real_battery(88.0, STATE_DISCHARGING)
        };
        assert_eq!(
            presence(&device),
            BatteryPresence::Absent,
            "kind {kind} is not a system battery"
        );
        assert!(reading_of(&device, NOW).is_none(), "kind {kind} reported");
    }
}

/// A5 — a machine with no battery that nonetheless reports a plausible
/// percentage.
///
/// The percentage is never the input to the presence question, so a
/// non-zero one changes nothing. A fix built on `percentage == 0` would
/// wrongly report this machine as a battery at 50%.
#[test]
fn absence_does_not_depend_on_the_percentage() {
    let device = DisplayDevice {
        percentage: 50.0,
        ..no_battery()
    };
    assert_eq!(presence(&device), BatteryPresence::Absent);
    assert!(reading_of(&device, NOW).is_none());
}

// ---------------------------------------------------------------------------
// Malformed values and bounds.
// ---------------------------------------------------------------------------

/// M1 — a present battery whose percentage is not a number.
///
/// A `NaN` cast to `u32` saturates to 0, so the naive path would turn a
/// malfunctioning sensor into a confident "0%". Say nothing instead.
#[test]
fn non_finite_percentage_reports_nothing() {
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(
            reading_of(&real_battery(bad, STATE_DISCHARGING), NOW).is_none(),
            "{bad} was reported as a percentage"
        );
    }
}

/// M2 — out-of-range percentages from firmware are clamped, not rejected.
/// Pre-existing product behaviour, pinned here so the rewrite preserved it.
#[test]
fn percentages_are_clamped_to_the_wire_range() {
    let over = reading_of(&real_battery(101.7, STATE_FULLY_CHARGED), NOW).expect("reports");
    assert_eq!(over.percentage, 100);

    let under = reading_of(&real_battery(-3.0, STATE_EMPTY), NOW).expect("reports");
    assert_eq!(under.percentage, 0);
}

/// M3 — rounding, not truncation.
#[test]
fn percentages_round_to_nearest() {
    assert_eq!(
        reading_of(&real_battery(78.6, STATE_DISCHARGING), NOW)
            .expect("reports")
            .percentage,
        79
    );
}

/// M4 — an unknown `State` value from a future UPower does not hide a
/// present battery; only the charge state is unspecified.
#[test]
fn unknown_state_value_keeps_the_battery() {
    let reading = reading_of(&real_battery(42.0, 99), NOW).expect("reports");
    assert_eq!(reading.percentage, 42);
    assert_eq!(reading.charging_state, ChargingState::Unspecified);
}

// ---------------------------------------------------------------------------
// The capability seam. Absence is expressed by having no local source; it
// must silence sending and change nothing about receiving.
// ---------------------------------------------------------------------------

/// A source standing in for the UPower reader, fed one snapshot.
struct FakeSource(Option<DisplayDevice>);

#[async_trait::async_trait]
impl LocalBatterySource for FakeSource {
    async fn read(&self) -> Option<BatteryReading> {
        // Mirrors the real reader: `None` when D-Bus could not be read at
        // all, and otherwise whatever the presence rule concludes.
        reading_of(&self.0?, NOW)
    }
}

fn peer() -> Fingerprint {
    Fingerprint::from_hex(&"07".repeat(32)).expect("a valid test fingerprint")
}

fn context() -> (
    CapabilityContext,
    tokio::sync::mpsc::Receiver<OutboundMessage>,
) {
    let (tx, rx) = tokio::sync::mpsc::channel(8);
    (
        CapabilityContext {
            peer: peer(),
            peer_device_id: "test-peer".to_string(),
            outbound: tx,
        },
        rx,
    )
}

/// C1 — a machine with a battery announces it on connect.
#[tokio::test]
async fn a_machine_with_a_battery_sends_on_connect() {
    let capability = BatteryCapability::new(Arc::new(BatteryState::default())).with_local_source(
        Arc::new(FakeSource(Some(real_battery(79.0, STATE_DISCHARGING)))),
    );

    let (ctx, mut rx) = context();
    capability.on_peer_connected(&ctx).await.expect("connect");

    let sent = rx.try_recv().expect("a battery was announced");
    assert_eq!(sent.capability_id, CAPABILITY_ID);
    let decoded = BatteryCapability::decode(&sent.payload).expect("decodes");
    assert_eq!(decoded.percentage, 79);
}

/// C2 — a flat battery is still announced. The end-to-end half of §8: a real
/// 0% must reach the wire.
#[tokio::test]
async fn a_flat_battery_is_still_announced() {
    let capability = BatteryCapability::new(Arc::new(BatteryState::default()))
        .with_local_source(Arc::new(FakeSource(Some(real_battery(0.0, STATE_EMPTY)))));

    let (ctx, mut rx) = context();
    capability.on_peer_connected(&ctx).await.expect("connect");

    let sent = rx.try_recv().expect("a flat battery is still a battery");
    assert_eq!(
        BatteryCapability::decode(&sent.payload)
            .expect("decodes")
            .percentage,
        0
    );
}

/// C3 — **the defect**. A battery-less machine sends no frame at all, so the
/// phone has nothing to render and shows no percentage. Before this branch
/// the same machine sent `percentage = 0` and the phone read "Battery 0
/// percent".
#[tokio::test]
async fn a_battery_less_machine_sends_no_frame() {
    let capability = BatteryCapability::new(Arc::new(BatteryState::default()))
        .with_local_source(Arc::new(FakeSource(Some(no_battery()))));

    let (ctx, mut rx) = context();
    capability.on_peer_connected(&ctx).await.expect("connect");

    assert!(
        rx.try_recv().is_err(),
        "a machine with no battery announced one anyway"
    );
}

/// C4 — no local source at all: the shape the daemon builds when
/// `UPowerReader::detect` says `Absent` or `Unavailable`, and the shape a
/// build without the `upower` feature always has.
#[tokio::test]
async fn no_local_source_sends_no_frame() {
    let capability = BatteryCapability::new(Arc::new(BatteryState::default()));

    let (ctx, mut rx) = context();
    capability.on_peer_connected(&ctx).await.expect("connect");

    assert!(rx.try_recv().is_err(), "a capability with no source sent");
}

/// C5 — D-Bus unreadable. Distinct from absence in what the daemon logs, and
/// identical in what reaches the wire: nothing. "We could not ask" must never
/// become "0%" either.
#[tokio::test]
async fn an_unreadable_source_sends_no_frame() {
    let capability = BatteryCapability::new(Arc::new(BatteryState::default()))
        .with_local_source(Arc::new(FakeSource(None)));

    let (ctx, mut rx) = context();
    capability.on_peer_connected(&ctx).await.expect("connect");

    assert!(rx.try_recv().is_err(), "an unreadable source sent a value");
}

/// C6 — **the independence rule** (§10/§15). Having no battery of its own
/// must not stop this machine from receiving the phone's battery. The two
/// directions share a capability id and nothing else.
#[tokio::test]
async fn a_battery_less_machine_still_receives_the_peers_battery() {
    let state = Arc::new(BatteryState::default());
    let capability = BatteryCapability::new(Arc::clone(&state))
        .with_local_source(Arc::new(FakeSource(Some(no_battery()))));

    let (ctx, mut rx) = context();
    capability.on_peer_connected(&ctx).await.expect("connect");
    assert!(rx.try_recv().is_err(), "sent a battery it does not have");

    let phone = BatteryCapability::encode(&BatteryReading {
        percentage: 79,
        charging_state: ChargingState::Charging,
        peer_timestamp_unix_ms: NOW,
    });
    capability
        .on_message(&ctx, &phone.payload)
        .await
        .expect("inbound accepted");

    let held = state.get(&peer()).expect("the phone's battery was stored");
    assert_eq!(held.reading.percentage, 79);
    assert_eq!(held.reading.charging_state, ChargingState::Charging);
}

/// C7 — the same, for a machine with no source at all.
#[tokio::test]
async fn a_machine_with_no_source_still_receives() {
    let state = Arc::new(BatteryState::default());
    let capability = BatteryCapability::new(Arc::clone(&state));

    let (ctx, _rx) = context();
    let phone = BatteryCapability::encode(&BatteryReading {
        percentage: 12,
        charging_state: ChargingState::Discharging,
        peer_timestamp_unix_ms: NOW,
    });
    capability
        .on_message(&ctx, &phone.payload)
        .await
        .expect("inbound accepted");

    assert_eq!(
        state.get(&peer()).expect("stored").reading.percentage,
        12,
        "local battery absence disabled receiving"
    );
}

/// C8 — the capability keeps its id whether or not it has a battery, so the
/// daemon advertises `battery.v1` either way and the peer can still send to
/// it. Absence is expressed by the missing source, never by dropping the
/// capability out of the registry.
#[test]
fn the_capability_is_advertised_either_way() {
    let with = BatteryCapability::new(Arc::new(BatteryState::default()))
        .with_local_source(Arc::new(FakeSource(Some(no_battery()))));
    let without = BatteryCapability::new(Arc::new(BatteryState::default()));

    assert_eq!(with.id(), CAPABILITY_ID);
    assert_eq!(without.id(), CAPABILITY_ID);
}
