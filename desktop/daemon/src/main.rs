//! `anyflowd` — the user-session daemon.
//!
//! Runs unprivileged under `systemd --user`. It binds a high TCP port, a Unix
//! socket in `XDG_RUNTIME_DIR`, and an mDNS responder. It needs no root, no
//! capabilities, and no system-wide state.

use std::sync::Arc;

use anyflow_capability_battery::{BatteryCapability, BatteryState, UPowerReader};
use anyflow_core::capability::CapabilityRegistry;
use anyflow_core::store::Store;
use anyflow_daemon::{control, listener, mdns, server, state::DaemonState};
use clap::Parser;
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;

#[derive(Parser, Debug)]
#[command(name = "anyflowd", about = "AnyFlow daemon", version)]
struct Args {
    /// Data directory (identity and trust store).
    #[arg(long)]
    data_dir: Option<std::path::PathBuf>,

    /// TCP port to listen on. Overrides the stored setting.
    #[arg(long)]
    port: Option<u16>,

    /// Do not advertise over mDNS. The daemon still accepts connections from
    /// peers that already know an address.
    #[arg(long)]
    no_mdns: bool,

    /// Log filter, e.g. `info`, `anyflow_core=debug`.
    #[arg(long, default_value = "info")]
    log: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| args.log.clone().into()),
        )
        // No timestamps: journald adds its own, and duplicating them makes
        // `journalctl` output harder to read.
        .without_time()
        .init();

    // Installing the process-wide crypto provider explicitly, rather than
    // relying on a default, so that the choice of backend is visible in the
    // source and cannot be changed by a transitive dependency's feature flag.
    rustls::crypto::ring::default_provider()
        .install_default()
        .map_err(|_| anyhow::anyhow!("a rustls crypto provider was already installed"))?;

    let data_dir = args
        .data_dir
        .unwrap_or_else(anyflow_core::store::default_data_dir);
    let store = Store::open(&data_dir)?;

    // A `--port` override applies to this run only. Silently rewriting the
    // user's stored configuration from a command-line flag is a surprise
    // nobody wants.
    if let Some(port) = args.port {
        tracing::info!(port, "using command-line port override");
    }
    let port = args.port.unwrap_or(store.settings().listen_port);

    let identity_fp = store.identity().fingerprint();
    let device_id = store.identity().device_id().to_string();
    let device_name = store.settings().device_name.clone();

    tracing::info!(
        device = %device_id,
        name = %device_name,
        fingerprint = %identity_fp.to_display_short(),
        "local identity"
    );

    // ---- capabilities -----------------------------------------------------
    let battery_state = Arc::new(BatteryState::default());
    let mut battery = BatteryCapability::new(Arc::clone(&battery_state));
    if let Some(upower) = UPowerReader::connect().await {
        tracing::info!("UPower available; this machine will report its own battery");
        battery = battery.with_local_source(Arc::new(upower));
    } else {
        tracing::info!("no local battery source; battery.v1 is receive-only");
    }

    let registry = CapabilityRegistry::builder()
        .register(Arc::new(battery))
        .build();
    tracing::info!(capabilities = ?registry.advertised(), "capabilities registered");

    // ---- TLS --------------------------------------------------------------
    let tls_config = anyflow_core::tls::server_config(store.identity())?;
    let acceptor = TlsAcceptor::from(tls_config);

    let state = Arc::new(DaemonState::new(store, registry, battery_state));

    // ---- listeners --------------------------------------------------------
    let tcp = TcpListener::bind(("0.0.0.0", port)).await?;
    let bound_port = tcp.local_addr()?.port();
    tracing::info!(port = bound_port, "listening");

    let socket_path = control::control_socket_path();
    let control_listener = server::bind(&socket_path)?;
    tracing::info!(socket = %socket_path.display(), "control socket ready");

    // Held for the process lifetime; dropping it withdraws the mDNS record.
    let _advertisement = if args.no_mdns {
        tracing::info!("mDNS advertisement disabled");
        None
    } else {
        match mdns::Advertisement::publish(&device_id, &device_name, bound_port) {
            Ok(a) => Some(a),
            Err(e) => {
                // Not fatal: pairing by QR carries explicit addresses, so the
                // daemon is still usable without a working mDNS responder.
                tracing::warn!(error = %e, "mDNS advertisement failed; discovery unavailable");
                None
            }
        }
    };

    state.set_listen_port(bound_port);

    let net = tokio::spawn(listener::run(tcp, acceptor, Arc::clone(&state)));
    let ctl = tokio::spawn(server::run(control_listener, Arc::clone(&state)));

    tokio::select! {
        r = net => r??,
        r = ctl => r??,
        _ = tokio::signal::ctrl_c() => {
            tracing::info!("shutting down");
        }
    }

    let _ = std::fs::remove_file(&socket_path);
    Ok(())
}
