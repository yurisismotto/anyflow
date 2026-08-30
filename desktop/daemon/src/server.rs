//! Serves the local control socket.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use anyflow_core::qr::QrPayload;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};

use crate::control::{
    BatteryReport, ConnectionReport, DeviceReport, Event, Request, Response, StatusReport,
};
use crate::state::DaemonState;

/// Binds the control socket, replacing a stale one left by a crash.
pub fn bind(path: &Path) -> anyhow::Result<UnixListener> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
        harden(parent, 0o700)?;
    }
    // A leftover socket file from an unclean shutdown would make bind fail.
    // Removing it is safe: only this user can reach the directory, and a
    // second live daemon would have failed its own port bind first.
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    let listener = UnixListener::bind(path)?;
    harden(path, 0o600)?;
    Ok(listener)
}

fn harden(path: &Path, mode: u32) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
}

pub async fn run(listener: UnixListener, state: Arc<DaemonState>) -> anyhow::Result<()> {
    loop {
        let (stream, _) = listener.accept().await?;
        let state = Arc::clone(&state);
        tokio::spawn(async move {
            if let Err(e) = serve_client(stream, state).await {
                tracing::debug!(error = %e, "control client ended");
            }
        });
    }
}

async fn serve_client(stream: UnixStream, state: Arc<DaemonState>) -> anyhow::Result<()> {
    let (read, mut write) = stream.into_split();
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

async fn build_status(state: &Arc<DaemonState>) -> StatusReport {
    let info = state.device_info();
    let fingerprint = anyflow_core::Fingerprint::from_hex(&info.identity_fingerprint).ok();

    let mut connections = Vec::new();
    for handle in state.session_handles().await {
        let battery = state.battery.get(&handle.peer()).map(|b| BatteryReport {
            percentage: b.reading.percentage,
            charging_state: format!("{:?}", b.reading.charging_state),
            age_secs: b.received_at.elapsed().as_secs(),
        });
        connections.push(ConnectionReport {
            device_id: handle.device_id().to_string(),
            device_name: handle.device_name().to_string(),
            fingerprint_short: handle.peer().to_display_short(),
            negotiated_capabilities: handle.negotiated_capabilities().to_vec(),
            battery,
        });
    }

    let listen_port = state.listen_port().await;
    let store = state.store.lock().await;
    StatusReport {
        device_name: store.settings().device_name.clone(),
        device_id: info.device_id.clone(),
        fingerprint: info.identity_fingerprint.clone(),
        fingerprint_short: fingerprint
            .map(|f| f.to_display_short())
            .unwrap_or_default(),
        listen_port,
        protocol_version_min: anyflow_core::session::PROTOCOL_VERSION_MIN,
        protocol_version_max: anyflow_core::session::PROTOCOL_VERSION_MAX,
        capabilities: state.registry.advertised(),
        paired_devices: store.peers().filter(|p| !p.revoked).count(),
        connections,
        pairing_active: state.pairing_remaining().await.is_some(),
    }
}

async fn build_devices(state: &Arc<DaemonState>) -> Vec<DeviceReport> {
    let connected: Vec<_> = state
        .session_handles()
        .await
        .into_iter()
        .map(|h| h.peer())
        .collect();

    let store = state.store.lock().await;
    store
        .peers()
        .map(|p| DeviceReport {
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
            connected: connected.contains(&p.fingerprint),
        })
        .collect()
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
            if let Some(handle) = state.session_for(&fingerprint).await {
                handle.shutdown().await;
            }
            state.unregister_session(&fingerprint).await;
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

/// Drives an interactive pairing session over one control connection.
///
/// The window closes when this connection ends, for any reason. That coupling
/// is intentional: pairing is only open while a human is watching for it.
async fn run_pair_session(
    state: Arc<DaemonState>,
    mut lines: tokio::io::Lines<BufReader<tokio::net::unix::OwnedReadHalf>>,
    mut write: tokio::net::unix::OwnedWriteHalf,
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
fn local_addresses(port: u16) -> Vec<std::net::SocketAddr> {
    use std::net::{IpAddr, SocketAddr};

    let mut out = Vec::new();
    // `getifaddrs` would need a libc dependency; reading the routing table
    // via a connected UDP socket is the standard trick and needs no crate.
    // No packet is sent: `connect` on UDP only selects a route.
    if let Ok(sock) = std::net::UdpSocket::bind("0.0.0.0:0") {
        if sock.connect("192.0.2.1:9").is_ok() {
            if let Ok(SocketAddr::V4(local)) = sock.local_addr() {
                let ip = IpAddr::V4(*local.ip());
                if !local.ip().is_loopback() {
                    out.push(SocketAddr::new(ip, port));
                }
            }
        }
    }
    out
}
