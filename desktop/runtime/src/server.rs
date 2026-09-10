//! Serves the local control endpoint.
//!
//! Nothing in this file names a socket. It is handed something that satisfies
//! [`ControlListener`] and speaks newline-delimited JSON over whatever byte
//! stream that yields — which is what makes a Windows named pipe a drop-in
//! rather than a rewrite. The Unix-domain implementation lives in
//! `anyflow-linux`.

use std::sync::Arc;
use std::time::Duration;

use anyflow_control::transport::ControlListener;
use anyflow_core::qr::QrPayload;
use tokio::io::{AsyncBufReadExt, AsyncWrite, AsyncWriteExt, BufReader};

use crate::control::{
    BatteryReport, ClipboardFlag, ClipboardPeerReport, ClipboardStatusReport, ConnectionReport,
    DeviceReport, DeviceState, Event, NotificationPeerReport, NotificationSetting,
    NotificationsStatusReport, PendingClipReport, Request, Response, StatusReport, TransferReport,
    BATTERY_STALE_AFTER_SECS,
};
use crate::state::DaemonState;

pub async fn run<L: ControlListener>(listener: L, state: Arc<DaemonState>) -> anyhow::Result<()> {
    loop {
        let stream = listener.accept().await?;
        let state = Arc::clone(&state);
        tokio::spawn(async move {
            if let Err(e) = serve_client(stream, state).await {
                tracing::debug!(error = %e, "control client ended");
            }
        });
    }
}

async fn serve_client<S: anyflow_control::transport::ControlStream>(
    stream: S,
    state: Arc<DaemonState>,
) -> anyhow::Result<()> {
    let (read, mut write) = tokio::io::split(stream);
    let mut lines = BufReader::new(read).lines();

    let Some(line) = lines.next_line().await? else {
        return Ok(());
    };

    let request: Request = match serde_json::from_str(&line) {
        Ok(r) => r,
        Err(e) => {
            send(
                &mut write,
                &Response::Error {
                    message: format!("malformed request: {e}"),
                },
            )
            .await?;
            return Ok(());
        }
    };

    match request {
        Request::Status => {
            let report = build_status(&state).await;
            send(&mut write, &Response::Status(report)).await?;
        }
        Request::Devices => {
            let report = build_devices(&state).await;
            send(&mut write, &Response::Devices(report)).await?;
        }
        Request::Ping { device } => {
            let response = do_ping(&state, &device).await;
            send(&mut write, &response).await?;
        }
        Request::Unpair { device } => {
            let response = do_unpair(&state, &device).await;
            send(&mut write, &response).await?;
        }
        Request::Confirm { .. } => {
            send(
                &mut write,
                &Response::Error {
                    message: "confirm is only valid inside a pair session".into(),
                },
            )
            .await?;
        }
        Request::Pair { ttl_secs } => {
            run_pair_session(state, lines, write, ttl_secs).await?;
        }
        Request::Grant {
            device,
            capability,
            granted,
        } => {
            let response = do_grant(&state, &device, &capability, granted).await;
            send(&mut write, &response).await?;
        }
        Request::Transfers => {
            let report = build_transfers(&state).await;
            send(&mut write, &Response::Transfers(report)).await?;
        }
        Request::CancelTransfer { transfer } => {
            let response = do_cancel_transfer(&state, &transfer).await;
            send(&mut write, &response).await?;
        }
        Request::Send { device, path } => {
            run_send_session(state, write, &device, &path).await?;
        }
        Request::ClipboardStatus => {
            let report = build_clipboard_status(&state).await;
            send(&mut write, &Response::Clipboard(report)).await?;
        }
        Request::ClipboardSend { device, sensitive } => {
            let response = do_clipboard_send(&state, &device, sensitive).await;
            send(&mut write, &response).await?;
        }
        Request::ClipboardApply { device } => {
            let response = do_clipboard_apply(&state, &device).await;
            send(&mut write, &response).await?;
        }
        Request::ClipboardPolicy {
            device,
            flag,
            enabled,
        } => {
            let response = do_clipboard_policy(&state, &device, flag, enabled).await;
            send(&mut write, &response).await?;
        }
        Request::NotificationsStatus => {
            let report = build_notifications_status(&state).await;
            send(&mut write, &Response::Notifications(report)).await?;
        }
        Request::NotificationsPolicy { device, setting } => {
            let response = do_notifications_policy(&state, &device, setting).await;
            send(&mut write, &response).await?;
        }
    }

    Ok(())
}

async fn send<W: AsyncWriteExt + Unpin, T: serde::Serialize>(
    w: &mut W,
    value: &T,
) -> anyhow::Result<()> {
    let mut bytes = serde_json::to_vec(value)?;
    bytes.push(b'\n');
    w.write_all(&bytes).await?;
    w.flush().await?;
    Ok(())
}

/// Builds a battery report, marking it stale rather than presenting an old
/// reading as though it were current.
fn battery_report(
    state: &Arc<DaemonState>,
    peer: &anyflow_core::Fingerprint,
) -> Option<BatteryReport> {
    state.battery.get(peer).map(|b| {
        let age_secs = b.received_at.elapsed().as_secs();
        BatteryReport {
            percentage: b.reading.percentage,
            charging_state: format!("{:?}", b.reading.charging_state),
            age_secs,
            stale: age_secs >= BATTERY_STALE_AFTER_SECS,
        }
    })
}

/// The state of a device that currently has a session.
///
/// Derived from how long the *session* has been silent, never from the age of
/// a capability's last reading. Those are different questions and conflating
/// them gets both answers wrong: a link that is idle but answering its
/// liveness probes is perfectly healthy even though no battery update has
/// arrived in an hour, while a half-open socket is dead however recently its
/// last reading came in. `Stale` here means at least one probe has gone
/// unanswered, which is the real early warning.
fn live_state(handle: &anyflow_core::session::SessionHandle) -> DeviceState {
    if handle.is_stale() {
        DeviceState::Stale
    } else {
        DeviceState::Connected
    }
}

async fn build_status(state: &Arc<DaemonState>) -> StatusReport {
    let info = state.device_info();
    let fingerprint = anyflow_core::Fingerprint::from_hex(&info.identity_fingerprint).ok();

    let mut connections = Vec::new();
    for handle in state.session_handles().await {
        let battery = battery_report(state, &handle.peer());
        connections.push(ConnectionReport {
            device_id: handle.device_id().to_string(),
            device_name: handle.device_name().to_string(),
            fingerprint_short: handle.peer().to_display_short(),
            negotiated_capabilities: handle.negotiated_capabilities().to_vec(),
            state: live_state(&handle),
            silent_secs: handle.silent_for().as_secs(),
            battery,
            session_id: handle.id(),
        });
    }

    let devices = build_devices(state).await;
    let listen_port = state.listen_port().await;
    let listen_families = state.listen_families();
    let store = state.store.lock().await;
    StatusReport {
        device_name: store.settings().device_name.clone(),
        device_id: info.device_id.clone(),
        fingerprint: info.identity_fingerprint.clone(),
        fingerprint_short: fingerprint
            .map(|f| f.to_display_short())
            .unwrap_or_default(),
        key_backing: store.key_backing().to_string(),
        listen_port,
        listen_families,
        protocol_version_min: anyflow_core::session::PROTOCOL_VERSION_MIN,
        protocol_version_max: anyflow_core::session::PROTOCOL_VERSION_MAX,
        capabilities: state.registry.advertised(),
        paired_devices: store.peers().filter(|p| !p.revoked).count(),
        connections,
        devices,
        pairing_active: state.pairing_remaining().await.is_some(),
    }
}

async fn build_devices(state: &Arc<DaemonState>) -> Vec<DeviceReport> {
    // Collected before taking the store lock: `last_seen` has its own lock,
    // and nesting them in two different orders elsewhere would be a deadlock
    // waiting to happen.
    let mut rows = Vec::new();
    {
        let store = state.store.lock().await;
        for p in store.peers() {
            rows.push(p.clone());
        }
    }

    let mut out = Vec::with_capacity(rows.len());
    for p in rows {
        let session = state.session_for(&p.fingerprint).await;
        let is_connected = session.is_some();
        let battery = if is_connected {
            battery_report(state, &p.fingerprint)
        } else {
            // Telemetry is forgotten when a peer disconnects (see the
            // battery capability), so there is deliberately nothing to show
            // here. That is what makes "offline but 57%" impossible.
            None
        };

        let device_state = match (&session, p.revoked) {
            (_, true) => DeviceState::Revoked,
            (Some(handle), false) => live_state(handle),
            (None, false) => DeviceState::Disconnected,
        };
        let silent_secs = session.as_ref().map(|h| h.silent_for().as_secs());

        let last_seen_secs_ago = state
            .last_seen(&p.fingerprint)
            .await
            .and_then(|t| t.elapsed().ok())
            .map(|d| d.as_secs());

        out.push(DeviceReport {
            device_id: p.device_id.clone(),
            device_name: p.device_name.clone(),
            platform: match anyflow_proto::v1::Platform::try_from(p.platform) {
                Ok(anyflow_proto::v1::Platform::Android) => "android".into(),
                Ok(anyflow_proto::v1::Platform::Linux) => "linux".into(),
                _ => "unknown".into(),
            },
            fingerprint: p.fingerprint.to_hex(),
            fingerprint_short: p.fingerprint.to_display_short(),
            paired_at_unix: p.paired_at_unix,
            granted_capabilities: p
                .granted_capabilities
                .iter()
                .filter(|(_, g)| **g)
                .map(|(k, _)| k.clone())
                .collect(),
            revoked: p.revoked,
            paired: !p.revoked,
            connected: is_connected,
            state: device_state,
            silent_secs,
            last_seen_secs_ago,
            battery,
        });
    }
    out
}

async fn do_ping(state: &Arc<DaemonState>, device: &str) -> Response {
    let fingerprint = match state.resolve_device(device).await {
        Ok(f) => f,
        Err(message) => return Response::Error { message },
    };
    let Some(handle) = state.session_for(&fingerprint).await else {
        return Response::Error {
            message: "device is not currently connected".into(),
        };
    };
    match handle.ping(Duration::from_secs(10)).await {
        Some(rtt) => Response::Pong {
            rtt_ms: rtt.as_millis() as u64,
        },
        None => Response::Error {
            message: "no PONG received before the timeout".into(),
        },
    }
}

async fn do_unpair(state: &Arc<DaemonState>, device: &str) -> Response {
    let fingerprint = match state.resolve_device(device).await {
        Ok(f) => f,
        Err(message) => return Response::Error { message },
    };

    let revoked = {
        let mut store = state.store.lock().await;
        store.revoke_peer(&fingerprint)
    };

    match revoked {
        Ok(true) => {
            // Revocation must take effect now, not at the next reconnect:
            // tear down any live session with that device.
            //
            // A data stream is a *separate* TCP connection and would survive
            // the control session's death for as long as its copy loop ran,
            // so in-flight transfers are stopped explicitly rather than left
            // to the reaper. Done before the session is closed, so the peer
            // still receives the cancellation.
            if let Some(transfers) = state.transfers.clone() {
                transfers
                    .cancel_peer(
                        &fingerprint,
                        anyflow_capability_files::transfer::FailureReason::Revoked,
                    )
                    .await;
            }
            if let Some(handle) = state.session_for(&fingerprint).await {
                handle.shutdown().await;
            }
            state.drop_session(&fingerprint).await;
            // `clipboard.v1` has no second connection to tear down, so
            // stopping it means the watcher must re-read who wants
            // auto-send. Without this a revoked device would keep being
            // pushed to until something else happened to bump the epoch.
            state.notify_clipboard_policy_changed();
            // And `notifications.v1` has state on the *screen*, which no
            // session teardown removes. A revoked device's notifications come
            // off it now.
            state.notify_notifications_revoked(&fingerprint).await;
            Response::Ok {
                message: format!("revoked {}", fingerprint.to_display_short()),
            }
        }
        Ok(false) => Response::Error {
            message: "no such device".into(),
        },
        Err(e) => Response::Error {
            message: format!("failed to persist revocation: {e}"),
        },
    }
}

/// Renders one transfer for the CLI.
async fn transfer_report(
    state: &Arc<DaemonState>,
    snapshot: &anyflow_capability_files::TransferSnapshot,
) -> TransferReport {
    let device_name = {
        let store = state.store.lock().await;
        store
            .peer_record(&snapshot.peer)
            .map(|p| p.device_name.clone())
            .unwrap_or_else(|| "unknown device".to_string())
    };

    TransferReport {
        transfer_id: snapshot.id.to_hex(),
        device_name,
        fingerprint_short: snapshot.peer.to_display_short(),
        direction: snapshot.direction.as_str().to_string(),
        filename: snapshot.filename.clone(),
        mime_type: snapshot.mime_type.clone(),
        size_bytes: snapshot.size_bytes,
        bytes_transferred: snapshot.bytes_transferred,
        percentage: snapshot.percentage(),
        state: snapshot.state.as_str().to_string(),
        failure: snapshot.failure.map(|f| f.as_str().to_string()),
        stored_at: snapshot.stored_at.as_ref().map(|p| p.display().to_string()),
    }
}

async fn build_transfers(state: &Arc<DaemonState>) -> Vec<TransferReport> {
    let Some(transfers) = state.transfers.clone() else {
        return Vec::new();
    };
    let snapshots = transfers.snapshot().await;
    let mut out = Vec::with_capacity(snapshots.len());
    for snapshot in &snapshots {
        out.push(transfer_report(state, snapshot).await);
    }
    out
}

/// Grants or withdraws a capability for one device.
async fn do_grant(
    state: &Arc<DaemonState>,
    device: &str,
    capability: &str,
    granted: bool,
) -> Response {
    let fingerprint = match state.resolve_device(device).await {
        Ok(f) => f,
        Err(message) => return Response::Error { message },
    };

    // Only a capability this build actually implements can be granted.
    // Storing a grant for an unknown id would produce a permission that looks
    // real in `anyflow devices` and does nothing.
    if !state.registry.supports(capability) {
        return Response::Error {
            message: format!("this daemon does not implement '{capability}'"),
        };
    }

    let result = {
        let mut store = state.store.lock().await;
        if store.trusted_peer(&fingerprint).is_none() {
            return Response::Error {
                message: "that device is not paired (or its pairing was revoked)".into(),
            };
        }
        store.set_capability_grant(&fingerprint, capability, granted)
    };

    if let Err(e) = result {
        return Response::Error {
            message: format!("could not persist the grant: {e}"),
        };
    }

    // Withdrawing a grant must bite immediately, exactly as a full revocation
    // does. Otherwise a transfer already in flight would run to completion
    // under a permission the user has just taken away.
    if !granted {
        if let Some(transfers) = state.transfers.clone() {
            if capability == anyflow_capability_files::CAPABILITY_ID {
                transfers
                    .cancel_peer(
                        &fingerprint,
                        anyflow_capability_files::transfer::FailureReason::Revoked,
                    )
                    .await;
            }
        }
    }

    // Either direction of a clipboard grant changes who the watcher should be
    // pushing to, so both are reported. Inbound authorization needs no
    // notification — it is re-read from the store per message — but the
    // outbound watcher is a running task and has to be told.
    if capability == anyflow_capability_clipboard::CAPABILITY_ID {
        state.notify_clipboard_policy_changed();
    }

    // Withdrawing a notification grant has to take the notifications that are
    // already on the screen off it. Inbound authorization needs no telling —
    // it is re-read per message — but a mirror already displayed is state this
    // daemon put there, and a revocation that left it up would only apply to
    // notifications that had not arrived yet.
    if !granted && capability == anyflow_capability_notifications::CAPABILITY_ID {
        state.notify_notifications_revoked(&fingerprint).await;
    }

    Response::Ok {
        message: format!(
            "{} {} for {}",
            if granted { "granted" } else { "withdrew" },
            capability,
            fingerprint.to_display_short()
        ),
    }
}

/// Cancels a transfer named by id or by an unambiguous prefix.
async fn do_cancel_transfer(state: &Arc<DaemonState>, selector: &str) -> Response {
    let Some(transfers) = state.transfers.clone() else {
        return Response::Error {
            message: "file transfer is not enabled".into(),
        };
    };

    let needle = selector.trim().to_ascii_lowercase();
    if needle.len() < 4 {
        return Response::Error {
            message: "a transfer id prefix must be at least 4 characters".into(),
        };
    }

    let matches: Vec<_> = transfers
        .snapshot()
        .await
        .into_iter()
        .filter(|t| t.state.is_active() && t.id.to_hex().starts_with(&needle))
        .collect();

    // An ambiguous prefix is an error, never a guess — the same rule
    // `resolve_device` follows, and for the same reason.
    match matches.as_slice() {
        [one] => {
            if transfers.cancel(one.id).await {
                Response::Ok {
                    message: format!("cancelled transfer {}", one.id),
                }
            } else {
                Response::Error {
                    message: "that transfer already finished".into(),
                }
            }
        }
        [] => Response::Error {
            message: format!("no active transfer matches '{selector}'"),
        },
        many => Response::Error {
            message: format!(
                "'{}' is ambiguous: it matches {} transfers",
                selector,
                many.len()
            ),
        },
    }
}

// ---------------------------------------------------------------------------
// clipboard.v1
// ---------------------------------------------------------------------------

/// Everything `anyflow clipboard status` shows.
///
/// Deliberately assembled from three independent sources — the backend's
/// probed capability, the trust store's grants, and the manager's live state
/// — and it keeps them visibly separate. Collapsing "granted" into "will
/// work" is how a user ends up believing auto-send is running on a desktop
/// whose compositor cannot report clipboard changes at all.
async fn build_clipboard_status(state: &Arc<DaemonState>) -> ClipboardStatusReport {
    let Some(clipboard) = state.clipboard.clone() else {
        return ClipboardStatusReport {
            enabled: false,
            backend: "none".into(),
            backend_detail: "clipboard.v1 is not enabled in this daemon".into(),
            watch_available: false,
            sensitive_available: false,
            sensitive_detail: "clipboard.v1 is not enabled in this daemon".into(),
            event_cache_entries: 0,
            suppression_cache_entries: 0,
            peers: Vec::new(),
            pending: Vec::new(),
        };
    };

    let backend = clipboard.backend();
    let watch_available = backend.watch_availability().is_ok();
    let sensitive = backend.sensitive_support();
    let (event_cache_entries, suppression_cache_entries) = clipboard.cache_sizes().await;
    let last_results = clipboard.last_results().await;

    let rows: Vec<anyflow_core::store::TrustedPeer> = {
        let store = state.store.lock().await;
        store.peers().cloned().collect()
    };

    let mut peers = Vec::with_capacity(rows.len());
    for p in &rows {
        peers.push(ClipboardPeerReport {
            device_id: p.device_id.clone(),
            device_name: p.device_name.clone(),
            fingerprint_short: p.fingerprint.to_display_short(),
            granted: p.allows(anyflow_capability_clipboard::CAPABILITY_ID),
            revoked: p.revoked,
            connected: state.session_for(&p.fingerprint).await.is_some(),
            allow_send: p.clipboard_policy.allow_send,
            allow_receive: p.clipboard_policy.allow_receive,
            auto_send: p.clipboard_policy.auto_send,
            auto_receive: p.clipboard_policy.auto_receive,
            last_outcome: last_results
                .get(&p.fingerprint)
                .map(|o| o.as_str().to_string()),
        });
    }

    let pending = clipboard
        .pending_clips()
        .await
        .into_iter()
        .map(|clip| {
            let name = rows
                .iter()
                .find(|p| p.fingerprint == clip.peer)
                .map(|p| p.device_name.clone())
                .unwrap_or_else(|| "unknown device".to_string());
            PendingClipReport {
                device_name: name,
                fingerprint_short: clip.peer.to_display_short(),
                bytes: clip.bytes,
                hash_prefix: clip.hash_prefix,
                sensitive: clip.sensitive,
                origin_device_id: clip.origin_device_id,
                age_secs: clip.age_secs,
            }
        })
        .collect();

    ClipboardStatusReport {
        enabled: true,
        backend: backend.id().to_string(),
        backend_detail: backend.describe(),
        watch_available,
        sensitive_available: sensitive.is_ok(),
        sensitive_detail: sensitive.err().unwrap_or_default(),
        event_cache_entries,
        suppression_cache_entries,
        peers,
        pending,
    }
}

async fn do_clipboard_send(state: &Arc<DaemonState>, device: &str, sensitive: bool) -> Response {
    let Some(clipboard) = state.clipboard.clone() else {
        return Response::Error {
            message: "clipboard.v1 is not enabled".into(),
        };
    };
    // An ambiguous or unknown selector fails here, loudly. Sending a
    // clipboard — which may hold a password — to the wrong device because a
    // prefix matched two of them is exactly the failure this refuses to have.
    let fingerprint = match state.resolve_device(device).await {
        Ok(f) => f,
        Err(message) => return Response::Error { message },
    };

    match clipboard
        .send_current_clipboard(&fingerprint, sensitive)
        .await
    {
        // The byte count is reported; the content is not, here or anywhere.
        Ok(bytes) => Response::Ok {
            message: format!(
                "sent {bytes} bytes of clipboard text to {}{}",
                fingerprint.to_display_short(),
                if sensitive { " (marked sensitive)" } else { "" }
            ),
        },
        Err(e) => Response::Error {
            message: e.to_string(),
        },
    }
}

async fn do_clipboard_apply(state: &Arc<DaemonState>, device: &str) -> Response {
    let Some(clipboard) = state.clipboard.clone() else {
        return Response::Error {
            message: "clipboard.v1 is not enabled".into(),
        };
    };
    let fingerprint = match state.resolve_device(device).await {
        Ok(f) => f,
        Err(message) => return Response::Error { message },
    };

    match clipboard.apply_pending(&fingerprint).await {
        Ok(bytes) => Response::Ok {
            message: format!("applied {bytes} bytes to the clipboard"),
        },
        Err(e) => Response::Error {
            message: e.to_string(),
        },
    }
}

/// Sets one policy flag for one device.
///
/// Two things happen after the write and both matter. The store is persisted,
/// so the answer survives a restart; and the clipboard manager is told, so a
/// watcher starts or stops *now* rather than at the next reconnect. Turning
/// `auto-send` off is a security action, and a security action that takes
/// effect eventually is not one.
async fn do_clipboard_policy(
    state: &Arc<DaemonState>,
    device: &str,
    flag: ClipboardFlag,
    enabled: bool,
) -> Response {
    let fingerprint = match state.resolve_device(device).await {
        Ok(f) => f,
        Err(message) => return Response::Error { message },
    };

    let (result, granted) = {
        let mut store = state.store.lock().await;
        let Some(peer) = store.trusted_peer(&fingerprint) else {
            return Response::Error {
                message: "that device is not paired (or its pairing was revoked)".into(),
            };
        };
        let granted = peer.allows(anyflow_capability_clipboard::CAPABILITY_ID);
        let mut policy = peer.clipboard_policy;
        match flag {
            ClipboardFlag::Send => policy.allow_send = enabled,
            ClipboardFlag::Receive => policy.allow_receive = enabled,
            ClipboardFlag::AutoSend => policy.auto_send = enabled,
            ClipboardFlag::AutoReceive => policy.auto_receive = enabled,
        }
        (store.set_clipboard_policy(&fingerprint, policy), granted)
    };

    if let Err(e) = result {
        return Response::Error {
            message: format!("could not persist the policy: {e}"),
        };
    }

    state.notify_clipboard_policy_changed();

    // A policy set on a peer with no grant is stored and inert. Saying so is
    // the difference between a setting that does not work and a setting the
    // user believes is working.
    let caveat = if granted {
        String::new()
    } else {
        format!(
            "\nNote: {} is not granted for this device, so clipboard policy \
             has no effect yet. Run: anyflow grant {} clipboard.v1",
            anyflow_capability_clipboard::CAPABILITY_ID,
            device
        )
    };

    Response::Ok {
        message: format!(
            "clipboard {flag} is now {} for {}{caveat}",
            if enabled { "on" } else { "off" },
            fingerprint.to_display_short()
        ),
    }
}

/// Offers a file and streams the transfer's progress until it settles.
async fn run_send_session(
    state: Arc<DaemonState>,
    mut write: impl AsyncWrite + Unpin,
    device: &str,
    path: &str,
) -> anyhow::Result<()> {
    let Some(transfers) = state.transfers.clone() else {
        send(
            &mut write,
            &Response::Error {
                message: "file transfer is not enabled".into(),
            },
        )
        .await?;
        return Ok(());
    };

    let fingerprint = match state.resolve_device(device).await {
        Ok(f) => f,
        Err(message) => {
            // An ambiguous or unknown name fails here, loudly. Sending a file
            // to the wrong device because a prefix matched two of them is
            // exactly the failure this refuses to have.
            send(&mut write, &Response::Error { message }).await?;
            return Ok(());
        }
    };

    let path = std::path::PathBuf::from(path);
    let metadata = match tokio::fs::metadata(&path).await {
        Ok(m) if m.is_file() => m,
        Ok(_) => {
            send(
                &mut write,
                &Response::Error {
                    message: "only regular files can be sent (directories come later)".into(),
                },
            )
            .await?;
            return Ok(());
        }
        Err(e) => {
            send(
                &mut write,
                &Response::Error {
                    message: format!("cannot read that file: {e}"),
                },
            )
            .await?;
            return Ok(());
        }
    };
    let _ = metadata;

    // Subscribed *before* the offer, so no early event is missed.
    let mut events = transfers.subscribe();

    let id = match transfers.offer_file(fingerprint, path).await {
        Ok(id) => id,
        Err(e) => {
            send(
                &mut write,
                &Response::Error {
                    message: format!("could not offer the file: {e}"),
                },
            )
            .await?;
            return Ok(());
        }
    };

    loop {
        let event = match events.recv().await {
            Ok(e) => e,
            // Lagged: the CLI fell behind a burst of progress updates. The
            // next event still carries the current byte count, so nothing is
            // lost but resolution.
            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
            Err(_) => break,
        };
        if event.0.id != id {
            continue;
        }

        let report = transfer_report(&state, &event.0).await;
        let terminal = event.0.state.is_terminal();
        send(&mut write, &Event::TransferProgress(report)).await?;

        if terminal {
            send(
                &mut write,
                &Event::Finished {
                    status: event.0.state.as_str().to_string(),
                    detail: event
                        .0
                        .failure
                        .map(|f| f.as_str().to_string())
                        .unwrap_or_else(|| event.0.filename.clone()),
                },
            )
            .await?;
            break;
        }
    }

    Ok(())
}

/// Drives an interactive pairing session over one control connection.
///
/// The window closes when this connection ends, for any reason. That coupling
/// is intentional: pairing is only open while a human is watching for it.
async fn run_pair_session(
    state: Arc<DaemonState>,
    mut lines: tokio::io::Lines<BufReader<impl tokio::io::AsyncRead + Unpin>>,
    mut write: impl AsyncWrite + Unpin,
    ttl_secs: Option<u64>,
) -> anyhow::Result<()> {
    let ttl = ttl_secs
        .map(Duration::from_secs)
        .unwrap_or(anyflow_core::pairing::DEFAULT_TOKEN_TTL);

    let (confirm_tx, mut confirm_rx) = tokio::sync::mpsc::channel(1);
    let token_b32 = match state.begin_pairing(ttl, confirm_tx).await {
        Ok(t) => t,
        Err(e) => {
            send(
                &mut write,
                &Response::Error {
                    message: format!("could not start pairing: {e}"),
                },
            )
            .await?;
            return Ok(());
        }
    };

    let info = state.device_info();
    let port = state.listen_port().await;
    let addresses = local_addresses(port);

    let fingerprint = anyflow_core::Fingerprint::from_hex(&info.identity_fingerprint)?;
    let token = anyflow_core::pairing::PairingToken::from_base32(&token_b32)?;
    let payload = QrPayload::encode(&fingerprint, &token, &info.device_id, &addresses);
    drop(token);

    let qr_ascii = render_qr(&payload)
        .unwrap_or_else(|| "(QR rendering unavailable; use the payload above)".to_string());

    send(
        &mut write,
        &Event::PairingReady {
            payload,
            qr_ascii,
            expires_in_secs: ttl.as_secs(),
        },
    )
    .await?;

    let deadline = tokio::time::Instant::now() + ttl;

    let outcome = loop {
        tokio::select! {
            _ = tokio::time::sleep_until(deadline) => {
                break ("expired", "the pairing window closed".to_string());
            }

            // The operator hung up (Ctrl-C on the CLI).
            line = lines.next_line() => {
                match line {
                    Ok(None) | Err(_) => break ("cancelled", "operator disconnected".into()),
                    Ok(Some(_)) => {
                        // A stray line outside a confirmation prompt.
                        continue;
                    }
                }
            }

            Some(request) = confirm_rx.recv() => {
                let short = request.fingerprint.to_display_short();
                send(&mut write, &Event::ConfirmRequest {
                    device_name: anyflow_core::discovery::sanitize_device_name(
                        &request.device.device_name,
                    ),
                    device_id: request.device.device_id.clone(),
                    fingerprint: request.fingerprint.to_hex(),
                    fingerprint_short: short.clone(),
                }).await?;

                let answer = match lines.next_line().await {
                    Ok(Some(line)) => matches!(
                        serde_json::from_str::<Request>(&line),
                        Ok(Request::Confirm { accept: true })
                    ),
                    _ => false,
                };

                let _ = request.reply.send(answer);

                if answer {
                    break ("paired", short);
                }
                break ("declined", short);
            }
        }
    };

    state.end_pairing().await;
    send(
        &mut write,
        &Event::Finished {
            status: outcome.0.to_string(),
            detail: outcome.1,
        },
    )
    .await?;
    Ok(())
}

/// Renders the QR payload as ASCII for a terminal.
fn render_qr(payload: &str) -> Option<String> {
    use qrcode::render::unicode;
    use qrcode::{EcLevel, QrCode};

    // Medium error correction: enough for a phone camera pointed at a
    // terminal, without inflating the code so much that it stops fitting in
    // an 80-column window.
    let code = QrCode::with_error_correction_level(payload.as_bytes(), EcLevel::M).ok()?;
    Some(code.render::<unicode::Dense1x2>().quiet_zone(true).build())
}

/// Best-effort list of addresses to advertise in the QR code.
///
/// These are hints only. If they are all wrong the phone falls back to mDNS,
/// and if that is wrong too the connection simply fails the pinned-key check.
///
/// Both families are offered, so a phone that reaches this machine over IPv6
/// is not forced through mDNS to find that out. Link-local IPv6 is excluded
/// on purpose: a `fe80::` address is meaningless without the zone index of
/// the interface it belongs to, and a QR code carries no zone.
fn local_addresses(port: u16) -> Vec<std::net::SocketAddr> {
    use std::net::{IpAddr, SocketAddr};

    // `getifaddrs` would need a libc dependency; reading the routing table
    // via a connected UDP socket is the standard trick and needs no crate.
    // No packet is sent: `connect` on UDP only selects a route.
    fn route_to(bind: &str, probe: &str) -> Option<IpAddr> {
        let sock = std::net::UdpSocket::bind(bind).ok()?;
        sock.connect(probe).ok()?;
        Some(sock.local_addr().ok()?.ip())
    }

    let mut out = Vec::new();

    // 192.0.2.0/24 and 2001:db8::/32 are the reserved documentation ranges:
    // nothing is ever routed to them, which is exactly what we want from an
    // address used only to ask the kernel which interface it would pick.
    if let Some(ip @ IpAddr::V4(v4)) = route_to("0.0.0.0:0", "192.0.2.1:9") {
        if !v4.is_loopback() && !v4.is_unspecified() {
            out.push(SocketAddr::new(ip, port));
        }
    }
    if let Some(ip @ IpAddr::V6(v6)) = route_to("[::]:0", "[2001:db8::1]:9") {
        let usable = !v6.is_loopback() && !v6.is_unspecified() && !is_link_local_v6(&v6);
        if usable {
            out.push(SocketAddr::new(ip, port));
        }
    }

    out
}

/// `Ipv6Addr::is_unicast_link_local` is still unstable, so test the prefix.
fn is_link_local_v6(ip: &std::net::Ipv6Addr) -> bool {
    (ip.segments()[0] & 0xffc0) == 0xfe80
}

// ---------------------------------------------------------------------------
// notifications.v1
// ---------------------------------------------------------------------------

/// Builds the notification diagnostic.
///
/// Counts and states only. Nothing here reads, holds or renders a
/// notification's title, body or application name — there is no field on
/// [`NotificationsStatusReport`] that could carry one, which is the property
/// that makes this safe to print, log and paste into a bug report.
async fn build_notifications_status(state: &Arc<DaemonState>) -> NotificationsStatusReport {
    let Some(notifications) = state.notifications.clone() else {
        return NotificationsStatusReport {
            enabled: false,
            backend: "none".into(),
            backend_detail: "notifications.v1 is not enabled in this daemon".into(),
            available: false,
            body_markup: false,
            persistence: false,
            lock_source: "none".into(),
            lock_detail: "notifications.v1 is not enabled in this daemon".into(),
            // Not "unlocked". A report about a capability that does not exist
            // must not read as an assurance about the screen.
            locked: true,
            mirrors: 0,
            peers: Vec::new(),
        };
    };

    let capabilities = notifications.capabilities().clone();
    let reports = notifications.peer_reports().await;

    let rows: Vec<anyflow_core::store::TrustedPeer> = {
        let store = state.store.lock().await;
        store.peers().cloned().collect()
    };

    let mut peers = Vec::with_capacity(rows.len());
    for p in &rows {
        let live = reports.iter().find(|r| r.peer == p.fingerprint);
        peers.push(NotificationPeerReport {
            device_id: p.device_id.clone(),
            device_name: p.device_name.clone(),
            fingerprint_short: p.fingerprint.to_display_short(),
            granted: p.allows(anyflow_capability_notifications::CAPABILITY_ID),
            revoked: p.revoked,
            connected: state.session_for(&p.fingerprint).await.is_some(),
            allow_mirror: p.notification_policy.allow_mirror,
            when_locked: p.notification_policy.when_sink_locked.as_str().to_string(),
            allow_dismiss_sync: p.notification_policy.allow_dismiss_sync,
            mirrors: live.map_or(0, |r| r.mirrors),
            displayed: live.map_or(0, |r| r.displayed),
            evicted: live.map_or(0, |r| r.evicted),
            local_roles: live.map_or(0, |r| r.local_roles),
            local_epoch: live.map_or(0, |r| r.local_epoch),
            peer_is_source: live.is_some_and(|r| r.peer_is_source),
            peer_epoch: live.map_or(0, |r| r.peer_epoch),
            snapshot_open: live.is_some_and(|r| r.snapshot_open),
            queued: live.map_or(0, |r| r.queue.pending),
            coalesced: live.map_or(0, |r| r.queue.coalesced),
            dropped: live.map_or(0, |r| r.queue.dropped_terminal),
        });
    }

    NotificationsStatusReport {
        enabled: true,
        backend: notifications.sink().id().to_string(),
        backend_detail: notifications.sink().describe(),
        available: notifications.is_available(),
        body_markup: capabilities.body_markup,
        persistence: capabilities.persistence,
        lock_source: notifications.lock_source().id().to_string(),
        lock_detail: notifications.lock_source().describe(),
        locked: notifications.is_locked(),
        mirrors: reports.iter().map(|r| r.mirrors).sum(),
        peers,
    }
}

/// Changes one per-peer notification setting.
///
/// The grant is a separate command (`anyflow grant <device>
/// notifications.v1`) and is deliberately not settable from here: a policy
/// edit must not be able to hand out the permission the policy is scoped by.
async fn do_notifications_policy(
    state: &Arc<DaemonState>,
    device: &str,
    setting: NotificationSetting,
) -> Response {
    let fingerprint = match state.resolve_device(device).await {
        Ok(f) => f,
        Err(message) => return Response::Error { message },
    };

    let current = {
        let store = state.store.lock().await;
        match store.trusted_peer(&fingerprint) {
            Some(p) => p.notification_policy,
            None => {
                return Response::Error {
                    message: "that device is not paired (or its pairing was revoked)".into(),
                }
            }
        }
    };

    let mut updated = current;
    let described = match &setting {
        NotificationSetting::Mirror { enabled } => {
            updated.allow_mirror = *enabled;
            format!("mirror={}", if *enabled { "on" } else { "off" })
        }
        NotificationSetting::WhenLocked { policy } => {
            match anyflow_core::notification_policy::LockPolicy::parse(policy) {
                Some(parsed) => {
                    updated.when_sink_locked = parsed;
                    format!("when-locked={}", parsed.as_str())
                }
                // Not guessed at, and not defaulted: a typo must never select
                // the most permissive option.
                None => {
                    return Response::Error {
                        message: format!(
                            "'{policy}' is not a lock policy. Use one of: \
                             full, app-only, suppress"
                        ),
                    }
                }
            }
        }
        NotificationSetting::DismissSync { enabled } => {
            updated.allow_dismiss_sync = *enabled;
            format!("dismiss-sync={}", if *enabled { "on" } else { "off" })
        }
    };

    let result = {
        let mut store = state.store.lock().await;
        store.set_notification_policy(&fingerprint, updated)
    };
    if let Err(e) = result {
        return Response::Error {
            message: format!("could not persist the policy: {e}"),
        };
    }

    // Turning mirroring off is a withdrawal, and a withdrawal has to take the
    // notifications that are already up down. Turning it back on displays
    // nothing retroactively: the source re-states what is active on its next
    // snapshot, which is the only place the truth lives.
    if matches!(setting, NotificationSetting::Mirror { enabled: false }) {
        state.notify_notifications_revoked(&fingerprint).await;
    }

    Response::Ok {
        message: format!("{} {}", fingerprint.to_display_short(), described),
    }
}
