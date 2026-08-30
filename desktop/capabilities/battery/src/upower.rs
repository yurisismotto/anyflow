//! Reads this machine's battery from UPower over D-Bus.
//!
//! Optional (`--features upower`): a desktop tower has no battery, and a
//! daemon that hard-depends on a D-Bus client for a feature most machines
//! cannot use is a bad trade. Without the feature the capability is
//! receive-only, which is exactly the Sprint's minimum requirement.
//!
//! We read the session/system bus property directly rather than shelling out
//! to `upower(1)`, and we never poll faster than the display could show.

use crate::{BatteryReading, LocalBatterySource};

/// D-Bus reader for `org.freedesktop.UPower`.
pub struct UPowerReader {
    #[cfg(feature = "upower")]
    connection: zbus::Connection,
}

#[cfg(feature = "upower")]
mod imp {
    use super::*;
    use fedroid_proto::v1::capabilities::ChargingState;

    /// UPower's `DisplayDevice` is the aggregate the desktop shell shows.
    const UPOWER_PATH: &str = "/org/freedesktop/UPower/devices/DisplayDevice";
    const UPOWER_DEST: &str = "org.freedesktop.UPower";
    const DEVICE_IFACE: &str = "org.freedesktop.UPower.Device";

    impl UPowerReader {
        /// Connects to the system bus. `None` if UPower is unavailable, which
        /// is a normal outcome, not an error.
        pub async fn connect() -> Option<Self> {
            let connection = zbus::Connection::system().await.ok()?;
            let reader = Self { connection };
            // Probe once so a machine without a battery reports `None` here
            // rather than on every later read.
            reader.read_raw().await?;
            Some(reader)
        }

        async fn read_raw(&self) -> Option<(f64, u32)> {
            let proxy = zbus::Proxy::new(&self.connection, UPOWER_DEST, UPOWER_PATH, DEVICE_IFACE)
                .await
                .ok()?;

            let percentage: f64 = proxy.get_property("Percentage").await.ok()?;
            let state: u32 = proxy.get_property("State").await.ok()?;
            Some((percentage, state))
        }
    }

    #[async_trait::async_trait]
    impl LocalBatterySource for UPowerReader {
        async fn read(&self) -> Option<BatteryReading> {
            let (percentage, state) = self.read_raw().await?;
            Some(BatteryReading {
                // Clamp rather than trust: UPower reports a float and some
                // firmware reports >100 on a freshly calibrated pack.
                percentage: percentage.round().clamp(0.0, 100.0) as u32,
                charging_state: map_state(state),
                peer_timestamp_unix_ms: now_ms(),
            })
        }
    }

    /// UPower `Device.State` enum -> our wire enum.
    fn map_state(state: u32) -> ChargingState {
        match state {
            1 => ChargingState::Charging,
            2 => ChargingState::Discharging,
            3 => ChargingState::NotCharging,
            4 => ChargingState::Full,
            _ => ChargingState::Unspecified,
        }
    }

    fn now_ms() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0)
    }
}

#[cfg(not(feature = "upower"))]
impl UPowerReader {
    /// Always `None` when the feature is disabled.
    pub async fn connect() -> Option<Self> {
        None
    }
}

#[cfg(not(feature = "upower"))]
#[async_trait::async_trait]
impl LocalBatterySource for UPowerReader {
    async fn read(&self) -> Option<BatteryReading> {
        None
    }
}
