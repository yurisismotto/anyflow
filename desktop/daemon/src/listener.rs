//! The TLS listener: one task per inbound connection.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyflow_core::error::Error;
use anyflow_core::session::{self, SessionHost};
use anyflow_core::tls;
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::TlsAcceptor;

use crate::state::DaemonState;

/// Cap on simultaneous half-open connections.
///
/// Without it, anyone on the LAN could open sockets until the daemon runs out
/// of file descriptors. Each slot is released when its task ends, including
/// on a handshake timeout.
const MAX_CONCURRENT_CONNECTIONS: usize = 32;

/// A connection that has not finished its TLS handshake within this window is
/// dropped, so a peer cannot hold a slot open by stalling.
const TLS_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);

pub async fn run(
    listener: TcpListener,
    acceptor: TlsAcceptor,
    state: Arc<DaemonState>,
) -> anyhow::Result<()> {
    let slots = Arc::new(tokio::sync::Semaphore::new(MAX_CONCURRENT_CONNECTIONS));

    loop {
        let (stream, addr) = match listener.accept().await {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(error = %e, "accept failed");
                continue;
            }
        };

        let Ok(permit) = Arc::clone(&slots).try_acquire_owned() else {
            tracing::warn!("connection limit reached; dropping inbound connection");
            drop(stream);
            continue;
        };

        let acceptor = acceptor.clone();
        let state = Arc::clone(&state);
        tokio::spawn(async move {
            let _permit = permit;
            if let Err(e) = handle_connection(stream, addr, acceptor, state).await {
                // The peer address is logged because it is operational data,
                // not user content. Nothing from the protocol payload is.
                tracing::info!(peer_addr = %addr, error = %e, "connection ended");
            }
        });
    }
}

async fn handle_connection(
    stream: TcpStream,
    addr: SocketAddr,
    acceptor: TlsAcceptor,
    state: Arc<DaemonState>,
) -> Result<(), Error> {
    // Latency matters for ping and for battery updates; these are tiny
    // messages that must not wait on Nagle.
    let _ = stream.set_nodelay(true);

    let mut tls = tokio::time::timeout(TLS_HANDSHAKE_TIMEOUT, acceptor.accept(stream))
        .await
        .map_err(|_| Error::Protocol("TLS handshake timed out"))??;

    // The identity comes from the completed handshake, never from anything
    // the peer asserts later.
    let fingerprint = {
        let (_, conn) = tls.get_ref();
        tls::peer_fingerprint(conn)?
    };

    tracing::debug!(
        peer_addr = %addr,
        peer = %fingerprint.to_display_short(),
        "TLS established"
    );

    let host: Arc<dyn SessionHost> = state.clone();
    let (established, envelope_state) =
        session::accept_handshake(&mut tls, &host, fingerprint).await?;

    tracing::info!(
        device = %established.device.device_id,
        peer = %fingerprint.to_display_short(),
        capabilities = ?established.negotiated_capabilities,
        "session established"
    );

    session::run_session(tls, host, established, envelope_state).await
}
