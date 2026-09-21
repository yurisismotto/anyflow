//! An opt-in probe of the machine this actually runs on.
//!
//! Ignored by default: it needs a live system bus and a real UPower, so it is
//! a diagnostic for a physical regression run, never part of the unit gate.
//! Every rule it exercises is covered deterministically in
//! `battery_presence.rs`; what this adds is a reading taken from real
//! hardware through the real D-Bus path.
//!
//! ```bash
//! cargo test -p omnibridge-capability-battery --features upower \
//!     --test real_upower -- --ignored --nocapture
//! ```
//!
//! On a laptop it must print a plausible percentage. On a machine with no
//! battery — a tower, or the U2 VMs — it must print `Absent` and no
//! percentage at all.

#![cfg(feature = "upower")]

use omnibridge_capability_battery::{LocalBattery, LocalBatterySource, UPowerReader};

#[tokio::test]
#[ignore = "needs a live system bus; run explicitly during a physical regression"]
async fn probe_this_machine() {
    match UPowerReader::detect().await {
        LocalBattery::Present(reader) => {
            let reading = reader
                .read()
                .await
                .expect("a machine detected as having a battery must produce a reading");
            println!(
                "PRESENT percentage={} charging_state={:?}",
                reading.percentage, reading.charging_state
            );
            assert!(
                reading.percentage <= 100,
                "percentage escaped the wire range"
            );
        }
        LocalBattery::Absent => {
            println!("ABSENT no system battery; nothing will be reported to peers");
        }
        LocalBattery::Unavailable => {
            println!("UNAVAILABLE UPower or D-Bus could not be read");
        }
    }
}
