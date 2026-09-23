//! Serves the local control endpoint.
//!
//! Nothing in this file names a socket. It is handed something that satisfies
//! [`ControlListener`] and speaks newline-delimited JSON over whatever byte
//! stream that yields — which is what makes a Windows named pipe a drop-in
//! rather than a rewrite. The Unix-domain implementation lives in
//! `omnibridge-linux`.

use std::sync::Arc;
use std::time::Duration;

use omnibridge_control::transport::ControlListener;
use omnibridge_core::qr::QrPayload;
use omnibridge_core::store::HideOutcome;
use tokio::io::{AsyncBufReadExt, AsyncWrite, AsyncWriteExt, BufReader};

use crate::control::{
    BatteryReport, ClipboardFlag, ClipboardPeerReport, ClipboardStatusReport, ConnectionReport,
    DeviceReport, DeviceState, Event, FileOfferRequest, NotificationPeerReport,
    NotificationSetting, NotificationsStatusReport, PendingClipReport, Request, Response,
    StatusReport, TransferReport, BATTERY_STALE_AFTER_SECS,
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

async fn serve_client<S: omnibridge_control::transport::ControlStream>(
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
        Request::HideRevokedDevice { fingerprint } => {
            let response = do_hide_revoked(&state, &fingerprint).await;
            send(&mut write, &response).await?;
        }
        Request::HideAllRevokedDevices => {
            let response = do_hide_all_revoked(&state).await;
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
        Request::WatchFileOffers => {
            run_file_approval_session(state, lines, write).await?;
        }
        Request::FileDecision { .. } => {
            send(
                &mut write,
                &Response::Error {
                    message: "file_decision is only valid inside a watch_file_offers stream".into(),
                },
            )
            .await?;
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
    peer: &omnibridge_core::Fingerprint,
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
fn live_state(handle: &omnibridge_core::session::SessionHandle) -> DeviceState {
    if handle.is_stale() {
        DeviceState::Stale
    } else {
        DeviceState::Connected
    }
}

async fn build_status(state: &Arc<DaemonState>) -> StatusReport {
    let info = state.device_info();
    let fingerprint = omnibridge_core::Fingerprint::from_hex(&info.identity_fingerprint).ok();

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
        protocol_version_min: omnibridge_core::session::PROTOCOL_VERSION_MIN,
        protocol_version_max: omnibridge_core::session::PROTOCOL_VERSION_MAX,
        capabilities: state.registry.advertised(),
        paired_devices: store.listed_peers().filter(|p| !p.revoked).count(),
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
        // `listed_peers`: a hidden tombstone is not a row. It is still in the
        // store, still revoked, and still what `lookup_peer` answers from —
        // it simply is not something a person is shown.
        for p in store.listed_peers() {
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
            platform: match omnibridge_proto::v1::Platform::try_from(p.platform) {
                Ok(omnibridge_proto::v1::Platform::Android) => "android".into(),
                Ok(omnibridge_proto::v1::Platform::Linux) => "linux".into(),
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
            enforce_revocation(state, &fingerprint).await;
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

/// Makes a revocation true of everything that is running, not only of the file.
///
/// Extracted from [`do_unpair`] so that the *one* way a revoked device is torn
/// down has one implementation. "Remove from list" calls it too — not because
/// a hidden tombstone is expected to have a session (revoking took it down
/// already), but because the alternative is a second, subtly different kill
/// path, and "which of the two ran?" is not a question a revocation should
/// ever raise.
async fn enforce_revocation(state: &Arc<DaemonState>, fingerprint: &omnibridge_core::Fingerprint) {
    // Revocation must take effect now, not at the next reconnect: tear down
    // any live session with that device.
    //
    // A data stream is a *separate* TCP connection and would survive the
    // control session's death for as long as its copy loop ran, so in-flight
    // transfers are stopped explicitly rather than left to the reaper. Done
    // before the session is closed, so the peer still receives the
    // cancellation.
    if let Some(transfers) = state.transfers.clone() {
        transfers
            .cancel_peer(
                fingerprint,
                omnibridge_capability_files::transfer::FailureReason::Revoked,
            )
            .await;
    }
    if let Some(handle) = state.session_for(fingerprint).await {
        handle.shutdown().await;
    }
    state.drop_session(fingerprint).await;
    // Any outstanding reconnect request dies with the pairing: there is
    // nothing left to converge, and the peer must not be counted as
    // mid-reconnect if it is ever paired again.
    state.clear_renegotiation(fingerprint).await;
    // `clipboard.v1` has no second connection to tear down, so stopping it
    // means the watcher must re-read who wants auto-send. Without this a
    // revoked device would keep being pushed to until something else happened
    // to bump the epoch.
    state.notify_clipboard_policy_changed();
    // And `notifications.v1` has state on the *screen*, which no session
    // teardown removes. A revoked device's notifications come off it now.
    state.notify_notifications_revoked(fingerprint).await;
}

/// "Remove from list" for one revoked device.
///
/// Addressed by full fingerprint hex, never by a selector: the record has no
/// device id or name left to match on, and identity here must be the pinned
/// key rather than anything two devices could share.
async fn do_hide_revoked(state: &Arc<DaemonState>, fingerprint: &str) -> Response {
    let Ok(fingerprint) = omnibridge_core::Fingerprint::from_hex(fingerprint.trim()) else {
        return Response::Error {
            message: "that is not a device fingerprint".into(),
        };
    };

    let outcome = {
        let mut store = state.store.lock().await;
        store.hide_revoked_peer(&fingerprint)
    };

    match outcome {
        Ok(HideOutcome::Hidden) => {
            enforce_revocation(state, &fingerprint).await;
            // Non-secret metadata only: the short public fingerprint and the
            // name of the action. This is the same shape the revocation above
            // logs, and there is nothing else about a tombstone to say.
            tracing::info!(
                peer = %fingerprint.to_display_short(),
                "removed a revoked device from the list; the revocation stands"
            );
            Response::Ok {
                message: format!(
                    "removed {} from the list; it stays revoked",
                    fingerprint.to_display_short()
                ),
            }
        }
        Ok(HideOutcome::AlreadyHidden) => Response::Ok {
            message: format!(
                "{} was already off the list",
                fingerprint.to_display_short()
            ),
        },
        Ok(HideOutcome::NotRevoked) => Response::Error {
            message: "that device is still paired; revoke it before removing it from the list"
                .into(),
        },
        Ok(HideOutcome::NotFound) => Response::Error {
            message: "no such device".into(),
        },
        Err(e) => Response::Error {
            message: format!("failed to persist the change: {e}"),
        },
    }
}

/// "Remove all revoked devices".
///
/// Acts on the revoked *visible* records and on nothing else. A trusted device
/// is not a candidate, a tombstone is already gone from the list, and no
/// display name is compared anywhere in the path.
async fn do_hide_all_revoked(state: &Arc<DaemonState>) -> Response {
    let hidden = {
        let mut store = state.store.lock().await;
        store.hide_all_revoked_peers()
    };

    match hidden {
        Ok(fingerprints) if fingerprints.is_empty() => Response::Ok {
            message: "no revoked devices to remove".into(),
        },
        Ok(fingerprints) => {
            for fp in &fingerprints {
                enforce_revocation(state, fp).await;
                tracing::info!(
                    peer = %fp.to_display_short(),
                    "removed a revoked device from the list; the revocation stands"
                );
            }
            Response::Ok {
                message: format!(
                    "removed {} revoked device(s) from the list; they stay revoked",
                    fingerprints.len()
                ),
            }
        }
        Err(e) => Response::Error {
            message: format!("failed to persist the change: {e}"),
        },
    }
}

/// Renders one transfer for the CLI.
async fn transfer_report(
    state: &Arc<DaemonState>,
    snapshot: &omnibridge_capability_files::TransferSnapshot,
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
        seq: snapshot.seq,
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
        failure_code: snapshot.failure.map(|f| f.code().to_string()),
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
///
/// Public so the integration suite can drive the *real* handler rather than a
/// reimplementation of it. The consequences below — a cancelled transfer, a
/// re-pointed clipboard watcher, a notification taken off the screen — are the
/// part that would be easy to get wrong in a copy, and they are exactly what
/// the GUI's grant switch depends on.
pub async fn do_grant(
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
    // real in `omnibridge devices` and does nothing.
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
            if capability == omnibridge_capability_files::CAPABILITY_ID {
                transfers
                    .cancel_peer(
                        &fingerprint,
                        omnibridge_capability_files::transfer::FailureReason::Revoked,
                    )
                    .await;
            }
        }
    }

    // Either direction of a clipboard grant changes who the watcher should be
    // pushing to, so both are reported. Inbound authorization needs no
    // notification — it is re-read from the store per message — but the
    // outbound watcher is a running task and has to be told.
    if capability == omnibridge_capability_clipboard::CAPABILITY_ID {
        state.notify_clipboard_policy_changed();
    }

    // Withdrawing a notification grant has to take the notifications that are
    // already on the screen off it. Inbound authorization needs no telling —
    // it is re-read per message — but a mirror already displayed is state this
    // daemon put there, and a revocation that left it up would only apply to
    // notifications that had not arrived yet.
    if !granted && capability == omnibridge_capability_notifications::CAPABILITY_ID {
        state.notify_notifications_revoked(&fingerprint).await;
    }

    // A grant that *widens* is the other half of the same rule, and it is the
    // one nothing implemented until now. The session's capability set was
    // fixed at `HELLO`, so a capability granted afterwards has no negotiated
    // channel to announce itself on — ADR-0017 §3 says such a grant needs a
    // reconnect to take effect, and this is the daemon finally asking for one
    // instead of leaving it to the user to press Disconnect and Connect.
    //
    // Capability-agnostic on purpose: `notifications.v1` is where the defect
    // was observed, but `files.v1` and `clipboard.v1` froze in exactly the
    // same way, and naming one here would have fixed one.
    let renegotiation = state
        .renegotiate_after_grant(&fingerprint, capability, granted)
        .await;

    Response::Ok {
        message: format!(
            "{} {} for {}{}",
            if granted { "granted" } else { "withdrew" },
            capability,
            fingerprint.to_display_short(),
            if renegotiation.is_reconnect() {
                " (reconnecting the device so it takes effect now)"
            } else {
                ""
            }
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

/// Everything `omnibridge clipboard status` shows.
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
            backend_available: false,
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
    // Three separate questions, asked separately and reported separately.
    // A machine can be "yes, no, no" (GNOME + wl-clipboard 2.2.1, which is
    // every current Ubuntu LTS and Debian Stable) and any one bit standing in
    // for the others would misdescribe it.
    let backend_available = backend.availability().is_ok();
    let watch_available = backend.watch_availability().is_ok();
    let sensitive = backend.sensitive_support();
    let (event_cache_entries, suppression_cache_entries) = clipboard.cache_sizes().await;
    let last_results = clipboard.last_results().await;

    let rows: Vec<omnibridge_core::store::TrustedPeer> = {
        let store = state.store.lock().await;
        store.listed_peers().cloned().collect()
    };

    let mut peers = Vec::with_capacity(rows.len());
    for p in &rows {
        peers.push(ClipboardPeerReport {
            device_id: p.device_id.clone(),
            device_name: p.device_name.clone(),
            fingerprint_short: p.fingerprint.to_display_short(),
            granted: p.allows(omnibridge_capability_clipboard::CAPABILITY_ID),
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
        backend_available,
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
        let granted = peer.allows(omnibridge_capability_clipboard::CAPABILITY_ID);
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
             has no effect yet. Run: omnibridge grant {} clipboard.v1",
            omnibridge_capability_clipboard::CAPABILITY_ID,
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

/// Drives the incoming-file approval provider over one control connection.
///
/// This is the missing product surface from U2: an Android device offers a
/// file, `files.v1` asks [`FileApproval`], `FileApproval` asks whoever is on
/// the other end of this socket, and the answer comes back down it. The
/// daemon depends on no toolkit to do it — a GTK window, a `omnibridge` command
/// and a test are all the same client from here.
///
/// The provider's attachment lives exactly as long as this connection. When
/// it ends — the window closed, the process died, the socket broke — every
/// question it was still being asked is dropped, which `files.v1` reads as a
/// decline. There is no path through this function that produces an
/// acceptance nobody sent.
///
/// [`FileApproval`]: crate::approval::FileApproval
async fn run_file_approval_session(
    state: Arc<DaemonState>,
    mut lines: tokio::io::Lines<BufReader<impl tokio::io::AsyncRead + Unpin>>,
    mut write: impl AsyncWrite + Unpin,
) -> anyhow::Result<()> {
    let (Some(approval), Some(transfers)) = (state.file_approval.clone(), state.transfers.clone())
    else {
        send(
            &mut write,
            &Response::Error {
                message: "this agent has no incoming-file approval seam".into(),
            },
        )
        .await?;
        return Ok(());
    };

    // Subscribed *before* attaching, so that a transfer which ends between
    // the two cannot slip through unwithdrawn.
    let mut events = transfers.subscribe();
    let (epoch, mut offers) = approval.attach();

    send(
        &mut write,
        &Event::FileApprovalReady {
            unattended: approval.unattended(),
        },
    )
    .await?;

    // Which offers this provider has been shown and not yet answered. Kept so
    // a transfer that ends underneath a prompt produces exactly one
    // withdrawal, for a prompt that is actually on screen.
    let mut open: std::collections::BTreeSet<omnibridge_capability_files::transfer::TransferId> =
        std::collections::BTreeSet::new();

    let outcome = loop {
        tokio::select! {
            // A peer made an offer and `files.v1` wants a human.
            offer = offers.recv() => {
                let Some(offer) = offer else {
                    // The seam dropped this provider: it was replaced by a
                    // newer one. Ending is right — the new session owns the
                    // prompts now.
                    break ("replaced", "another approval provider attached".to_string());
                };

                // The transfer may already have ended while the question was
                // in flight — a disconnect, the reaper. Putting a dead
                // question on screen is its own defect, so it is checked
                // rather than assumed.
                let live = transfers
                    .snapshot_one(offer.transfer_id)
                    .await
                    .is_some_and(|s| !s.state.is_terminal());
                if !live {
                    approval.withdraw(offer.transfer_id);
                    continue;
                }

                let request = file_offer_request(&state, &offer).await;
                open.insert(offer.transfer_id);
                send(&mut write, &Event::FileOfferRequest(request)).await?;
            }

            // The human answered — or hung up.
            line = lines.next_line() => {
                match line {
                    Ok(None) | Err(_) => {
                        break ("closed", "the approval provider disconnected".to_string());
                    }
                    Ok(Some(text)) => {
                        if let Ok(Request::FileDecision { transfer, accept }) =
                            serde_json::from_str::<Request>(&text)
                        {
                            apply_file_decision(&approval, &mut open, &transfer, accept);
                        }
                        // Anything else on this connection is ignored rather
                        // than answered. A stray request must not be able to
                        // stand in for a decision.
                    }
                }
            }

            // A transfer settled. If a prompt for it is open, it is now a
            // question about something that no longer exists.
            event = events.recv() => {
                match event {
                    Ok(omnibridge_capability_files::TransferEvent(snapshot)) => {
                        if snapshot.state.is_terminal() && open.remove(&snapshot.id) {
                            approval.withdraw(snapshot.id);
                            send(&mut write, &Event::FileOfferWithdrawn {
                                transfer_id: snapshot.id.to_hex(),
                                reason: snapshot
                                    .failure
                                    .map(|f| f.as_str().to_string())
                                    .unwrap_or_else(|| snapshot.state.as_str().to_string()),
                            }).await?;
                        }
                    }
                    // Lagged: this provider fell behind a burst of progress
                    // updates. Nothing that matters here is lost — a terminal
                    // state is re-derivable, and a prompt left open is bounded
                    // by the accept timeout either way.
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(_) => break ("closed", "the transfer manager went away".to_string()),
                }
            }
        }
    };

    // Detaching declines whatever is still pending. The epoch is what stops a
    // session that was already displaced from unhooking its replacement.
    approval.detach(epoch);
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

/// Applies one decision, and only to a prompt this provider actually has open.
///
/// The `open` check is not belt and braces over the seam's own keying: it is
/// what stops a client from answering a question it was never asked, on a
/// transfer it learned about from `omnibridge transfers`.
fn apply_file_decision(
    approval: &crate::approval::FileApproval,
    open: &mut std::collections::BTreeSet<omnibridge_capability_files::transfer::TransferId>,
    transfer: &str,
    accept: bool,
) {
    // Exact, never a prefix. See `TransferId::from_hex`.
    let Some(id) = omnibridge_capability_files::transfer::TransferId::from_hex(transfer) else {
        return;
    };
    if !open.remove(&id) {
        return;
    }
    let decision = approval.decide(id, accept);
    tracing::debug!(
        transfer = %id,
        accept,
        answered = matches!(decision, crate::approval::Decision::Answered),
        "an incoming file offer was answered by the approval provider"
    );
}

/// Describes an offer to the human who has to decide about it.
///
/// The device name comes from the **trust store**, keyed by the authenticated
/// fingerprint — never from the offer. A peer that renamed itself to match
/// another of your devices changes nothing about what this says.
async fn file_offer_request(
    state: &Arc<DaemonState>,
    offer: &omnibridge_capability_files::IncomingOffer,
) -> FileOfferRequest {
    let device_name = {
        let store = state.store.lock().await;
        store
            .trusted_peer(&offer.peer)
            .map(|p| p.device_name.clone())
            // A peer with no record cannot reach `files.v1` at all, so this is
            // unreachable in practice. It says so rather than inventing a
            // plausible name.
            .unwrap_or_else(|| "an unknown device".to_string())
    };

    FileOfferRequest {
        transfer_id: offer.transfer_id.to_hex(),
        device_name,
        device_id: offer.peer_device_id.clone(),
        fingerprint: offer.peer.to_hex(),
        fingerprint_short: offer.peer.to_display_short(),
        filename: offer.filename.clone(),
        size_bytes: offer.size_bytes,
        mime_type: offer.mime_type.clone(),
    }
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
        .unwrap_or(omnibridge_core::pairing::DEFAULT_TOKEN_TTL);

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

    let fingerprint = omnibridge_core::Fingerprint::from_hex(&info.identity_fingerprint)?;
    let token = omnibridge_core::pairing::PairingToken::from_base32(&token_b32)?;
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
                    device_name: omnibridge_core::discovery::sanitize_device_name(
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

    let rows: Vec<omnibridge_core::store::TrustedPeer> = {
        let store = state.store.lock().await;
        store.listed_peers().cloned().collect()
    };

    let mut peers = Vec::with_capacity(rows.len());
    for p in &rows {
        let live = reports.iter().find(|r| r.peer == p.fingerprint);
        peers.push(NotificationPeerReport {
            device_id: p.device_id.clone(),
            device_name: p.device_name.clone(),
            fingerprint_short: p.fingerprint.to_display_short(),
            granted: p.allows(omnibridge_capability_notifications::CAPABILITY_ID),
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
            peer_is_dismiss_target: live.is_some_and(|r| r.peer_is_dismiss_target),
            peer_epoch: live.map_or(0, |r| r.peer_epoch),
            local_reports_dismissals: live.is_some_and(|r| r.local_reports_dismissals),
            dismissals_sent: live.map_or(0, |r| r.dismissals_sent),
            dismissals_refused: live.map_or(0, |r| r.dismissals_refused),
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
/// The grant is a separate command (`omnibridge grant <device>
/// notifications.v1`) and is deliberately not settable from here: a policy
/// edit must not be able to hand out the permission the policy is scoped by.
///
/// Public for the same reason [`do_grant`] is: the mirror-closing behaviour
/// when displaying is switched off is what the GUI's switch relies on, and a
/// test that reimplemented it would prove nothing about the real path.
pub async fn do_notifications_policy(
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
            match omnibridge_core::notification_policy::LockPolicy::parse(policy) {
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

// ---------------------------------------------------------------------------
// The control vocabulary
// ---------------------------------------------------------------------------

/// Pins the two halves of the transfer vocabulary to each other.
///
/// `omnibridge-capability-files` owns the state machine and the failure enum;
/// `omnibridge-control` names the tokens a front end is allowed to branch on.
/// Neither crate can see the other, and this one sees both — so this is the
/// only place the correspondence can be checked, and it is checked
/// exhaustively rather than by spot-checking the interesting variants.
#[cfg(test)]
mod vocabulary {
    use omnibridge_capability_files::transfer::{FailureReason, TransferState};
    use omnibridge_control::{transfer_direction, transfer_failure, transfer_state};

    #[test]
    fn every_failure_reason_is_a_token_the_control_protocol_names() {
        for reason in FailureReason::ALL {
            assert!(
                transfer_failure::ALL.contains(&reason.code()),
                "{:?} produces {:?}, which omnibridge-control does not name — \
                 a front end branching on it would see an unknown token and \
                 fall back to a generic label",
                reason,
                reason.code()
            );
        }
        // And nothing is named that cannot happen, which would be a label
        // nothing could ever reach.
        let produced: Vec<&str> = FailureReason::ALL.iter().map(|r| r.code()).collect();
        for token in transfer_failure::ALL {
            assert!(
                produced.contains(&token),
                "omnibridge-control names {token:?}, which no FailureReason produces"
            );
        }
    }

    #[test]
    fn every_transfer_state_is_a_token_the_control_protocol_names() {
        let named = [
            transfer_state::OFFERED,
            transfer_state::WAITING_ACCEPT,
            transfer_state::TRANSFERRING,
            transfer_state::VERIFYING,
            transfer_state::COMPLETED,
            transfer_state::FAILED,
            transfer_state::CANCELLED,
        ];
        for state in [
            TransferState::Offered,
            TransferState::WaitingAccept,
            TransferState::Transferring,
            TransferState::Verifying,
            TransferState::Completed,
            TransferState::Failed,
            TransferState::Cancelled,
        ] {
            assert!(
                named.contains(&state.as_str()),
                "{:?} serialises as {:?}, which omnibridge-control does not name",
                state,
                state.as_str()
            );
            // The two crates must agree on which states are terminal, or a
            // front end will show a finished transfer as still moving — or,
            // worse, a moving one as finished.
            assert_eq!(
                transfer_state::is_terminal(state.as_str()),
                state.is_terminal(),
                "{state:?} is terminal in one crate and not the other"
            );
        }
    }

    #[test]
    fn both_directions_are_named() {
        use omnibridge_capability_files::transfer::Direction;
        assert_eq!(Direction::Sending.as_str(), transfer_direction::SENDING);
        assert_eq!(Direction::Receiving.as_str(), transfer_direction::RECEIVING);
    }
}
