//! Reads this machine's battery from UPower over D-Bus.
//!
//! Optional (`--features upower`): a desktop tower has no battery, and a
//! daemon that hard-depends on a D-Bus client for a feature most machines
//! cannot use is a bad trade. Without the feature the capability is
//! receive-only, which is exactly the Sprint's minimum requirement.
//!
//! We read the system bus properties directly rather than shelling out to
//! `upower(1)`, and we never poll faster than the display could show.
//!
//! # Presence is not the same question as charge
//!
//! UPower publishes `/org/freedesktop/UPower/devices/DisplayDevice` on
//! *every* machine, battery or not. On a machine with no battery the object
//! still answers every property read, with `Percentage = 0` and `State = 0`.
//! A successful D-Bus call is therefore **not** evidence that a battery
//! exists, and reading only `Percentage`/`State` reports a battery-less
//! tower, VM or mini-PC as a battery sitting at 0%.
//!
//! Presence is decided by [`presence`] from the properties UPower provides
//! for exactly that purpose — `Type`, `IsPresent` and `PowerSupply` — and
//! never inferred from the percentage. A real battery that is genuinely
//! empty reads 0% and must keep reading 0%.

use crate::{BatteryReading, LocalBatterySource};
use omnibridge_proto::v1::capabilities::ChargingState;

/// `UP_DEVICE_KIND_BATTERY` from UPower's `up-types.h`. The `DisplayDevice`
/// aggregate reports this only when it is standing in for one or more real
/// system batteries; with none present it reports `UP_DEVICE_KIND_UNKNOWN`.
const UPOWER_KIND_BATTERY: u32 = 2;

/// The `DisplayDevice` properties this reader consults, as one snapshot.
///
/// Split out from the D-Bus call so the presence rule can be tested against
/// every shape a real machine produces without a system bus. Field names
/// follow the D-Bus property names, except `kind` — `Type` is a keyword.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DisplayDevice {
    /// `Type`: `UP_DEVICE_KIND_*`. 2 is a battery.
    pub kind: u32,
    /// `IsPresent`: the battery is physically installed.
    pub is_present: bool,
    /// `PowerSupply`: this device powers the machine, as opposed to a
    /// peripheral that merely has a cell in it (a mouse, a headset).
    pub power_supply: bool,
    /// `Percentage`: 0..=100, as a float.
    pub percentage: f64,
    /// `State`: `UP_DEVICE_STATE_*`.
    pub state: u32,
}

/// Whether the aggregate describes a battery this machine actually has.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatteryPresence {
    /// A real, installed system battery. Its percentage means something.
    Present,
    /// UPower answered, and what it described is not a system battery.
    Absent,
}

/// What a probe of UPower concluded, and the reader if there is one to keep.
///
/// Three outcomes rather than two, because the daemon says something
/// different in each case and "we could not ask" is not "there is no
/// battery".
pub enum LocalBattery {
    /// This machine has a battery; report it to peers.
    Present(UPowerReader),
    /// UPower answered and this machine has no battery. Receive-only, and
    /// correctly so — there is nothing to send.
    Absent,
    /// UPower or D-Bus could not be reached, or its properties could not be
    /// read. Also receive-only, but for a different reason: we do not know.
    Unavailable,
}

/// The presence rule.
///
/// All three properties are authoritative statements by UPower about what
/// the device *is*; none of them is derived from how full it is. `State` is
/// deliberately not consulted: an installed battery may sit at
/// `UP_DEVICE_STATE_UNKNOWN` while a charge controller settles, and that is a
/// battery whose state we do not know, not an absent battery.
pub fn presence(device: &DisplayDevice) -> BatteryPresence {
    if device.kind == UPOWER_KIND_BATTERY && device.is_present && device.power_supply {
        BatteryPresence::Present
    } else {
        BatteryPresence::Absent
    }
}

/// Turns a snapshot into a reading, or `None` when there is no battery to
/// report.
///
/// `None` here means "say nothing", never "say zero": the caller sends no
/// frame at all, so the peer shows no battery rather than a fake empty one.
pub fn reading_of(device: &DisplayDevice, now_unix_ms: i64) -> Option<BatteryReading> {
    if presence(device) == BatteryPresence::Absent {
        return None;
    }
    // A present battery whose percentage is not a number is a malfunctioning
    // or half-initialised reading, not a flat battery. Saying nothing is
    // better than saying 0%, which is the whole point of this module.
    if !device.percentage.is_finite() {
        return None;
    }
    Some(BatteryReading {
        // Clamp rather than trust: UPower reports a float and some firmware
        // reports >100 on a freshly calibrated pack.
        percentage: device.percentage.round().clamp(0.0, 100.0) as u32,
        charging_state: map_state(device.state),
        peer_timestamp_unix_ms: now_unix_ms,
    })
}

/// UPower `UP_DEVICE_STATE_*` -> our wire enum.
///
/// The two `PENDING_*` states are what a laptop reports when it is plugged in
/// but held below a charge threshold, which is the steady state on machines
/// with battery conservation enabled — including the host this was tested on.
pub fn map_state(state: u32) -> ChargingState {
    match state {
        1 => ChargingState::Charging,
        2 => ChargingState::Discharging,
        3 => ChargingState::NotCharging,
        4 => ChargingState::Full,
        5 => ChargingState::NotCharging,
        6 => ChargingState::Discharging,
        _ => ChargingState::Unspecified,
    }
}

/// Gated with the reader: without the `upower` feature nothing in this crate
/// takes a local reading, so a wall clock would be dead weight.
#[cfg(feature = "upower")]
pub(crate) fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// D-Bus reader for `org.freedesktop.UPower`.
pub struct UPowerReader {
    #[cfg(feature = "upower")]
    connection: zbus::Connection,
}

#[cfg(feature = "upower")]
mod imp {
    use super::*;

    /// UPower's `DisplayDevice` is the aggregate the desktop shell shows.
    const UPOWER_PATH: &str = "/org/freedesktop/UPower/devices/DisplayDevice";
    const UPOWER_DEST: &str = "org.freedesktop.UPower";
    const DEVICE_IFACE: &str = "org.freedesktop.UPower.Device";

    impl UPowerReader {
        /// Asks UPower whether this machine has a battery.
        ///
        /// Presence is decided once, here, because the answer is a property
        /// of the hardware: a tower does not grow a battery. A machine whose
        /// battery is physically removable is discussed in the crate docs —
        /// [`LocalBatterySource::read`] re-checks presence on every read, so
        /// a battery that disappears mid-session stops being reported rather
        /// than freezing at its last value.
        pub async fn detect() -> LocalBattery {
            let Ok(connection) = zbus::Connection::system().await else {
                return LocalBattery::Unavailable;
            };
            let reader = Self { connection };
            match reader.read_display_device().await {
                None => LocalBattery::Unavailable,
                Some(device) => match presence(&device) {
                    BatteryPresence::Present => LocalBattery::Present(reader),
                    BatteryPresence::Absent => LocalBattery::Absent,
                },
            }
        }

        /// One snapshot of the properties the presence rule needs.
        ///
        /// `None` means UPower could not be read, which is distinct from
        /// "read fine, and there is no battery".
        async fn read_display_device(&self) -> Option<DisplayDevice> {
            let proxy = zbus::Proxy::new(&self.connection, UPOWER_DEST, UPOWER_PATH, DEVICE_IFACE)
                .await
                .ok()?;
            Some(DisplayDevice {
                kind: proxy.get_property("Type").await.ok()?,
                is_present: proxy.get_property("IsPresent").await.ok()?,
                power_supply: proxy.get_property("PowerSupply").await.ok()?,
                percentage: proxy.get_property("Percentage").await.ok()?,
                state: proxy.get_property("State").await.ok()?,
            })
        }
    }

    #[async_trait::async_trait]
    impl LocalBatterySource for UPowerReader {
        async fn read(&self) -> Option<BatteryReading> {
            let device = self.read_display_device().await?;
            reading_of(&device, now_ms())
        }
    }
}

#[cfg(not(feature = "upower"))]
impl UPowerReader {
    /// Never a battery when the feature is disabled: there is no D-Bus
    /// client compiled in to ask with.
    pub async fn detect() -> LocalBattery {
        LocalBattery::Unavailable
    }
}

#[cfg(not(feature = "upower"))]
#[async_trait::async_trait]
impl LocalBatterySource for UPowerReader {
    async fn read(&self) -> Option<BatteryReading> {
        None
    }
}
