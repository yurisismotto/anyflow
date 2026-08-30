//! The TLS listener: one task per inbound connection.
//!
//! # Address families
//!
//! The daemon listens on every family it advertises, and advertises only what
//! it listens on. Getting that wrong is not cosmetic: an mDNS record with an
//! `AAAA` for a daemon bound to `0.0.0.0` sends the phone to an address that
//! refuses every connection, and the phone has no way to tell that from the
//! computer being asleep.
//!
//! Whether one `[::]` socket also serves IPv4 depends on `IPV6_V6ONLY`, whose
//! default comes from `net.ipv6.bindv6only` and is therefore not something to
//! assume. [`bind_endpoints`] finds out by experiment instead — see there.

use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use anyflow_core::error::Error;
use anyflow_core::session::{self, SessionHost};
use anyflow_core::tls;
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::TlsAcceptor;

use crate::state::DaemonState;

/// Which address families the daemon actually accepts connections on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Families {
    pub ipv4: bool,
    pub ipv6: bool,
}

impl std::fmt::Display for Families {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match (self.ipv4, self.ipv6) {
            (true, true) => f.write_str("IPv4+IPv6"),
            (true, false) => f.write_str("IPv4"),
            (false, true) => f.write_str("IPv6"),
            (false, false) => f.write_str("none"),
        }
    }
}

/// The bound sockets, the port they share, and the families they serve.
pub struct Bound {
    pub listeners: Vec<TcpListener>,
    pub port: u16,
    pub families: Families,
}

/// Binds the TCP listener(s), covering both address families where the host
/// has them.
///
/// The order is deliberate. Binding `[::]` first and `0.0.0.0` second is the
/// only way to learn what the first socket actually covers:
///
/// * the v4 bind **succeeds** — the v6 socket was `IPV6_V6ONLY`, and the two
///   sockets together cover both families;
/// * the v4 bind fails with `AddrInUse` — the v6 socket is dual-stack and is
///   already serving IPv4 through v4-mapped addresses;
/// * the v6 bind fails outright — the host has no IPv6, so IPv4 alone it is.
///
/// Doing it the other way round cannot distinguish those cases, and reading
/// the sysctl would be a guess about a value the socket can simply be asked.
pub fn bind_endpoints(port: u16) -> std::io::Result<Bound> {
    use std::net::TcpListener as StdListener;

    fn adopt(std_listener: StdListener) -> std::io::Result<TcpListener> {
        std_listener.set_nonblocking(true)?;
        TcpListener::from_std(std_listener)
    }

    let v6 = StdListener::bind(SocketAddr::from((Ipv6Addr::UNSPECIFIED, port)));

    let (listeners, bound_port, families) = match v6 {
        Ok(v6) => {
            let bound_port = v6.local_addr()?.port();
            match StdListener::bind(SocketAddr::from((Ipv4Addr::UNSPECIFIED, bound_port))) {
                Ok(v4) => (
                    vec![adopt(v6)?, adopt(v4)?],
                    bound_port,
                    Families {
                        ipv4: true,
                        ipv6: true,
                    },
                ),
                Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
                    tracing::debug!("the IPv6 socket is dual-stack; one socket serves both");
                    (
                        vec![adopt(v6)?],
                        bound_port,
                        Families {
                            ipv4: true,
                            ipv6: true,
                        },
                    )
                }
                Err(e) => {
                    tracing::warn!(error = %e, "could not also bind IPv4; serving IPv6 only");
                    (
                        vec![adopt(v6)?],
                        bound_port,
                        Families {
                            ipv4: false,
                            ipv6: true,
                        },
                    )
                }
            }
        }
        Err(e) => {
            tracing::info!(error = %e, "no IPv6 listener; serving IPv4 only");
            let v4 = StdListener::bind(SocketAddr::from((Ipv4Addr::UNSPECIFIED, port)))?;
            let bound_port = v4.local_addr()?.port();
            (
                vec![adopt(v4)?],
                bound_port,
                Families {
                    ipv4: true,
                    ipv6: false,
                },
            )
        }
    };

    Ok(Bound {
        listeners,
        port: bound_port,
        families,
    })
}

/// Cap on simultaneous half-open connections.
///
/// Without it, anyone on the LAN could open sockets until the daemon runs out
/// of file descriptors. Each slot is released when its task ends, including
/// on a handshake timeout.
const MAX_CONCURRENT_CONNECTIONS: usize = 32;

/// A connection that has not finished its TLS handshake within this window is
/// dropped, so a peer cannot hold a slot open by stalling.
const TLS_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);

/// Accepts on every bound socket until one of them fails fatally.
///
/// The connection limit is shared across sockets rather than per-socket: it
/// exists to bound file descriptors for the process, and a peer must not be
/// able to double it by dialling the other address family.
pub async fn run(
    listeners: Vec<TcpListener>,
    acceptor: TlsAcceptor,
    state: Arc<DaemonState>,
) -> anyhow::Result<()> {
    let slots = Arc::new(tokio::sync::Semaphore::new(MAX_CONCURRENT_CONNECTIONS));

    let mut tasks = tokio::task::JoinSet::new();
    for listener in listeners {
        tasks.spawn(accept_loop(
            listener,
            acceptor.clone(),
            Arc::clone(&state),
            Arc::clone(&slots),
        ));
    }

    match tasks.join_next().await {
        Some(Ok(r)) => r,
        Some(Err(e)) => Err(anyhow::anyhow!("accept task failed: {e}")),
        None => Err(anyhow::anyhow!("no listening socket")),
    }
}

async fn accept_loop(
    listener: TcpListener,
    acceptor: TlsAcceptor,
    state: Arc<DaemonState>,
    slots: Arc<tokio::sync::Semaphore>,
) -> anyhow::Result<()> {
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
