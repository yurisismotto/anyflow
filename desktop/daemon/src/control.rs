//! The daemon's local control interface.
//!
//! A newline-delimited JSON protocol over a Unix domain socket in
//! `$XDG_RUNTIME_DIR/fedroid-bridge/control.sock`.
//!
//! # Why a Unix socket and not D-Bus
//!
//! D-Bus is the idiomatic desktop choice and we will likely add it for the
//! GUI. It is the wrong thing to *start* with: it would make the daemon
//! untestable without a session bus, and the CLI is the only client today.
//! A socket in `XDG_RUNTIME_DIR` is already restricted to the owning user by
//! the directory's 0700 mode, needs no bus, and is trivial to drive from a
//! test. See ADR-0003.
//!
//! # Why this is not "remote execution"
//!
//! This socket is local-only and is never reachable from the network. No
//! protocol message from a peer can reach it. Peers cannot invoke any command
//! here — this Sprint has no remote command execution of any kind.

use serde::{Deserialize, Serialize};

/// A request from the CLI to the daemon.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum Request {
    /// Daemon health, identity and live connections.
    Status,
    /// All known devices, including revoked ones.
    Devices,
    /// Opens a pairing window. The connection then streams [`Event`]s.
    Pair { ttl_secs: Option<u64> },
    /// Answers a `ConfirmRequest`. Only valid inside a `Pair` stream.
    Confirm { accept: bool },
    /// Revokes a pairing by device id or fingerprint prefix.
    Unpair { device: String },
    /// Round-trips a PING over the live session with a device.
    Ping { device: String },
}

/// A single-shot reply.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Response {
    Status(StatusReport),
    Devices(Vec<DeviceReport>),
    Pong { rtt_ms: u64 },
    Ok { message: String },
    Error { message: String },
}

/// A streamed event, currently only used by `Pair`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    /// The QR payload plus a pre-rendered ASCII code, so the CLI does not
    /// need a QR library of its own.
    PairingReady {
        payload: String,
        qr_ascii: String,
        expires_in_secs: u64,
    },
    /// A peer proved it holds the pairing token. The human must now decide.
    ConfirmRequest {
        device_name: String,
        device_id: String,
        fingerprint: String,
        fingerprint_short: String,
    },
    /// Terminal event for the stream.
    Finished { status: String, detail: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusReport {
    pub device_name: String,
    pub device_id: String,
    pub fingerprint: String,
    pub fingerprint_short: String,
    pub listen_port: u16,
    pub protocol_version_min: u32,
    pub protocol_version_max: u32,
    pub capabilities: Vec<String>,
    pub paired_devices: usize,
    pub connections: Vec<ConnectionReport>,
    pub pairing_active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionReport {
    pub device_id: String,
    pub device_name: String,
    pub fingerprint_short: String,
    pub negotiated_capabilities: Vec<String>,
    pub battery: Option<BatteryReport>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatteryReport {
    pub percentage: u32,
    pub charging_state: String,
    pub age_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceReport {
    pub device_id: String,
    pub device_name: String,
    pub platform: String,
    pub fingerprint: String,
    pub fingerprint_short: String,
    pub paired_at_unix: i64,
    pub granted_capabilities: Vec<String>,
    pub revoked: bool,
    pub connected: bool,
}

/// Path of the control socket.
///
/// `XDG_RUNTIME_DIR` is per-user and mode 0700, so the socket is not
/// reachable by other local users. If it is unset (an unusual login), we fall
/// back to a per-uid path under `/tmp` and create it 0700 ourselves.
pub fn control_socket_path() -> std::path::PathBuf {
    let base = std::env::var_os("XDG_RUNTIME_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from(format!("/tmp/fedroid-bridge-{}", nix_uid())));
    base.join("fedroid-bridge").join("control.sock")
}

fn nix_uid() -> u32 {
    // Avoids a `libc`/`nix` dependency for one number. `/proc/self/status`
    // is always present on Linux, which is the only platform this daemon
    // targets.
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines()
                .find_map(|l| l.strip_prefix("Uid:"))
                .and_then(|l| l.split_whitespace().next().map(str::to_string))
        })
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
}
