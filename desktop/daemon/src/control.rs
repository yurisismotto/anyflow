//! The daemon's local control interface.
//!
//! A newline-delimited JSON protocol over a Unix domain socket in
//! `$XDG_RUNTIME_DIR/anyflow/control.sock`.
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

    /// Grants or withdraws one capability for one device.
    ///
    /// Separate from pairing on purpose. Pairing establishes *who* a device
    /// is; a grant decides *what it may do*, and `files.v1` writes files, so
    /// it is never auto-granted (ADR-0008).
    Grant {
        device: String,
        capability: String,
        granted: bool,
    },

    /// Offers a local file to a device. The connection then streams
    /// [`Event`]s until the transfer reaches a terminal state.
    Send { device: String, path: String },

    /// Every transfer this daemon knows about in this run.
    Transfers,

    /// Cancels a transfer by id, or by an unambiguous id prefix.
    CancelTransfer { transfer: String },

    /// What `clipboard.v1` can do on this machine, and per-peer policy.
    ClipboardStatus,

    /// Sends the current local clipboard to one device.
    ///
    /// Manual and explicit: it is not gated on `auto_send`, because a human
    /// asking is a different act from a watcher firing.
    ClipboardSend {
        device: String,
        /// Ask the receiver to treat the clip as sensitive. On Android this
        /// sets `ClipDescription.EXTRA_IS_SENSITIVE`, which is a presentation
        /// hint and nothing more.
        sensitive: bool,
    },

    /// Writes a clip that arrived while `auto_receive` was off.
    ClipboardApply { device: String },

    /// Changes one per-peer clipboard policy flag.
    ///
    /// One request rather than four so that adding a flag is a new enum
    /// variant in [`ClipboardFlag`] and nothing else.
    ClipboardPolicy {
        device: String,
        flag: ClipboardFlag,
        enabled: bool,
    },
}

/// Which per-peer clipboard policy flag a [`Request::ClipboardPolicy`] sets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClipboardFlag {
    Send,
    Receive,
    AutoSend,
    AutoReceive,
}

impl std::fmt::Display for ClipboardFlag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Send => "send",
            Self::Receive => "receive",
            Self::AutoSend => "auto-send",
            Self::AutoReceive => "auto-receive",
        })
    }
}

/// A single-shot reply.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Response {
    Status(StatusReport),
    Devices(Vec<DeviceReport>),
    Transfers(Vec<TransferReport>),
    Clipboard(ClipboardStatusReport),
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

    /// A transfer changed. Streamed by `Send` so the CLI can render progress
    /// without polling.
    TransferProgress(TransferReport),
}

/// One transfer, as the CLI sees it.
///
/// `stored_at` is present only for a file this machine *received*, and only
/// once it is verified and promoted. It is local information and is never
/// sent to a peer — an absolute path on the receiver is exactly the kind of
/// thing the offer format deliberately has no room for.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferReport {
    /// Full hex. Local only: the truncated form is what reaches a log.
    pub transfer_id: String,
    pub device_name: String,
    pub fingerprint_short: String,
    pub direction: String,
    /// Sanitized. The raw peer-supplied name never reaches a display.
    pub filename: String,
    pub mime_type: String,
    pub size_bytes: u64,
    pub bytes_transferred: u64,
    /// `None` for a zero-byte file, where a percentage means nothing.
    pub percentage: Option<u8>,
    pub state: String,
    /// Set once the transfer is not going to complete.
    pub failure: Option<String>,
    pub stored_at: Option<String>,
}

/// What `clipboard.v1` can actually do here, and for whom.
///
/// Note what is *not* in this type: no clipboard text, not even a preview.
/// A pending clip is described by its size, a hash prefix and its age. That
/// is enough to tell two clips apart and to decide whether to apply one, and
/// it means the control socket never carries clipboard content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipboardStatusReport {
    /// Whether the capability is registered at all.
    pub enabled: bool,
    /// The platform backend, e.g. `wl-clipboard`.
    pub backend: String,
    /// One line on what the backend can do here, including where clipboard
    /// change notifications come from — or why there are none.
    pub backend_detail: String,
    /// True when this session can report clipboard changes, which is what
    /// `auto_send` needs. False is a normal, documented state (GNOME).
    pub watch_available: bool,
    /// How many events and suppression entries the caches hold. Present so
    /// the bounded-growth property is observable rather than merely claimed.
    pub event_cache_entries: usize,
    pub suppression_cache_entries: usize,
    pub peers: Vec<ClipboardPeerReport>,
    pub pending: Vec<PendingClipReport>,
}

/// One device's clipboard grant and policy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipboardPeerReport {
    pub device_id: String,
    pub device_name: String,
    pub fingerprint_short: String,
    /// Whether `clipboard.v1` is granted. Without it every flag below is
    /// inert, which is why it is reported next to them.
    pub granted: bool,
    /// Whether the pairing itself was revoked.
    ///
    /// Reported separately from `granted` because "not granted" and "this
    /// device is no longer trusted at all" are different situations with
    /// different fixes, and showing the second as the first understates it.
    pub revoked: bool,
    pub connected: bool,
    pub allow_send: bool,
    pub allow_receive: bool,
    pub auto_send: bool,
    pub auto_receive: bool,
    /// The last outcome this peer reported for something we sent it.
    pub last_outcome: Option<String>,
}

/// A clip held in memory because `auto_receive` is off. No content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingClipReport {
    pub device_name: String,
    pub fingerprint_short: String,
    /// Size in UTF-8 bytes.
    pub bytes: usize,
    /// First 8 hex characters of the SHA-256 of the content. Enough to tell
    /// two clips apart; useless for recovering either.
    pub hash_prefix: String,
    pub sensitive: bool,
    pub origin_device_id: String,
    pub age_secs: u64,
}

/// How a known device stands *right now*.
///
/// The distinction this type exists to enforce: being paired is a durable
/// property of the trust store, having a session is a momentary property of
/// the network, and the last telemetry we saw is neither. Collapsing them
/// into one "connected" flag is what let a dead session keep displaying a
/// battery percentage as though it were live.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceState {
    /// Trust was revoked. The device cannot connect until it pairs again.
    Revoked,
    /// A live session exists and the device is talking to us.
    Connected,
    /// A session exists, but nothing has arrived from the device for long
    /// enough that anything it last told us should be read as history. This
    /// is what a half-open socket looks like before liveness gives up on it.
    Stale,
    /// Paired, no session.
    Disconnected,
}

impl std::fmt::Display for DeviceState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Revoked => "revoked",
            Self::Connected => "connected",
            Self::Stale => "stale",
            Self::Disconnected => "disconnected",
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusReport {
    pub device_name: String,
    pub device_id: String,
    pub fingerprint: String,
    pub fingerprint_short: String,
    pub listen_port: u16,
    /// Which address families the listener actually accepts on, e.g.
    /// `IPv4+IPv6`. Reported because the mDNS record is derived from it.
    pub listen_families: String,
    pub protocol_version_min: u32,
    pub protocol_version_max: u32,
    pub capabilities: Vec<String>,
    pub paired_devices: usize,
    pub connections: Vec<ConnectionReport>,
    /// Every known device with its current state, so `status` can show a
    /// paired-but-offline device instead of silently omitting it.
    pub devices: Vec<DeviceReport>,
    pub pairing_active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionReport {
    pub device_id: String,
    pub device_name: String,
    pub fingerprint_short: String,
    pub negotiated_capabilities: Vec<String>,
    pub battery: Option<BatteryReport>,
    /// Distinguishes this session from an earlier one with the same device.
    pub session_id: u64,
    pub state: DeviceState,
    /// How long since anything at all arrived on this session. This, not the
    /// age of a capability reading, is what says whether the link is healthy.
    pub silent_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatteryReport {
    pub percentage: u32,
    pub charging_state: String,
    pub age_secs: u64,
    /// True once the reading is old enough that it describes the past. A
    /// percentage is never shown without this alongside it.
    pub stale: bool,
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
    /// Durable: this device is in the trust store and not revoked.
    pub paired: bool,
    /// Momentary: a session exists right now. Never inferred from `paired`.
    pub connected: bool,
    pub state: DeviceState,
    /// Seconds of silence on the live session, if there is one.
    pub silent_secs: Option<u64>,
    /// Seconds since this device's last session ended, if one did in this
    /// daemon run. `None` means "not since the daemon started", not "never".
    pub last_seen_secs_ago: Option<u64>,
    /// Present only while a session is live: telemetry is dropped when a peer
    /// disconnects, so an offline device never carries a battery number.
    pub battery: Option<BatteryReport>,
}

/// A reading older than this is *labelled* as no longer current.
///
/// It marks the reading, not the link: `battery.v1` sends a value when a
/// session opens and when the level changes, so a healthy phone that has not
/// moved a percent legitimately has an old reading. Whether the session
/// itself is healthy is [`DeviceState`]'s question, answered from silence on
/// the wire.
pub const BATTERY_STALE_AFTER_SECS: u64 = 120;

/// Path of the control socket.
///
/// `XDG_RUNTIME_DIR` is per-user and mode 0700, so the socket is not
/// reachable by other local users. If it is unset (an unusual login), we fall
/// back to a per-uid path under `/tmp` and create it 0700 ourselves.
pub fn control_socket_path() -> std::path::PathBuf {
    let base = std::env::var_os("XDG_RUNTIME_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from(format!("/tmp/anyflow-{}", nix_uid())));
    base.join("anyflow").join("control.sock")
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
