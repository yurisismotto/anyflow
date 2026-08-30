//! Listener address-family correctness.
//!
//! The defect these guard: the daemon advertised an `fe80::` address over
//! mDNS while bound to `0.0.0.0`, so the phone dialled an endpoint that could
//! never answer. The rule is now one sentence — the daemon listens on every
//! family it reports, and reports only families it listens on — and these
//! tests are what hold it.

mod common;

use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;

use anyflow_daemon::listener;
use tokio::net::TcpStream;

/// True if this host can actually use IPv6 loopback. A build machine or a
/// container with IPv6 switched off is not a failure of this code.
fn host_has_ipv6() -> bool {
    std::net::TcpListener::bind(SocketAddr::from((Ipv6Addr::LOCALHOST, 0))).is_ok()
}

// `bind_endpoints` adopts std sockets into tokio, so it needs a runtime.
#[tokio::test]
async fn an_ephemeral_bind_reports_the_port_it_actually_got() {
    let bound = listener::bind_endpoints(0).expect("bind");
    assert_ne!(bound.port, 0, "port 0 must be resolved to the real port");
    for l in &bound.listeners {
        assert_eq!(
            l.local_addr().expect("addr").port(),
            bound.port,
            "every socket must share the reported port"
        );
    }
}

// `bind_endpoints` adopts std sockets into tokio, so it needs a runtime.
#[tokio::test]
async fn ipv4_is_always_served() {
    let bound = listener::bind_endpoints(0).expect("bind");
    assert!(bound.families.ipv4, "IPv4 must never be dropped silently");
}

// `bind_endpoints` adopts std sockets into tokio, so it needs a runtime.
#[tokio::test]
async fn ipv6_is_reported_exactly_when_the_host_has_it() {
    let bound = listener::bind_endpoints(0).expect("bind");
    assert_eq!(
        bound.families.ipv6,
        host_has_ipv6(),
        "the reported family set must match reality, in both directions"
    );
}

/// The real check: every family the daemon claims accepts a TCP connection.
///
/// This is what makes the mDNS record trustworthy — the advertisement is
/// derived from `families`, so if `families` lies, the record lies.
#[tokio::test]
async fn every_reported_family_accepts_a_connection() {
    common::init_crypto();
    let bound = listener::bind_endpoints(0).expect("bind");
    let port = bound.port;
    let families = bound.families;

    // A bare accept loop: this test is about reachability, not the protocol.
    let mut listeners = bound.listeners;
    let accepted = Arc::new(tokio::sync::Semaphore::new(0));
    for l in listeners.drain(..) {
        let accepted = Arc::clone(&accepted);
        tokio::spawn(async move {
            while let Ok((stream, _)) = l.accept().await {
                drop(stream);
                accepted.add_permits(1);
            }
        });
    }

    if families.ipv4 {
        TcpStream::connect(SocketAddr::from((Ipv4Addr::LOCALHOST, port)))
            .await
            .expect("IPv4 was reported as served, so it must accept");
    }
    if families.ipv6 {
        TcpStream::connect(SocketAddr::from((Ipv6Addr::LOCALHOST, port)))
            .await
            .expect("IPv6 was reported as served, so it must accept");
    }
}

/// A full pairing over IPv6, through the real listener and real TLS.
#[tokio::test]
async fn a_phone_can_pair_over_ipv6() {
    if !host_has_ipv6() {
        eprintln!("skipped: this host has no IPv6 loopback");
        return;
    }

    let server = common::TestServer::start_dual_stack().await;
    if !server.families.ipv6 {
        eprintln!("skipped: the listener did not obtain IPv6 on this host");
        return;
    }

    let client = common::TestClient::new("Phone");
    let token = server
        .open_pairing(std::time::Duration::from_secs(30))
        .await;

    let v6 = SocketAddr::from((Ipv6Addr::LOCALHOST, server.addr.port()));
    let session = client
        .connect(v6, server.fingerprint, Some(&token))
        .await
        .expect("pair over IPv6");

    assert!(client.is_trusted(&server.fingerprint).await);
    session.close().await;
}
