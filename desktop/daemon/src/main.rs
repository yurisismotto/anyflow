//! `anyflowd` — the user-session daemon.
//!
//! Runs unprivileged under `systemd --user`. It binds a high TCP port, a Unix
//! socket in `XDG_RUNTIME_DIR`, and an mDNS responder. It needs no root, no
//! capabilities, and no system-wide state.

use std::sync::Arc;

use anyflow_capability_battery::{BatteryCapability, BatteryState, UPowerReader};
use anyflow_capability_clipboard::{ClipboardCapability, ClipboardManager};
use anyflow_capability_files::{
    Destination, FilesCapability, FilesConfig, StreamRole, TransferApproval, TransferManager,
};
use anyflow_core::capability::CapabilityRegistry;
use anyflow_core::store::Store;
use anyflow_daemon::{control, listener, mdns, server, state::DaemonState};
use clap::Parser;
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

    /// Directory for received files.
    ///
    /// Defaults to `<XDG downloads>/AnyFlow`. Peers can never influence this:
    /// an offer carries a filename and no path at all.
    #[arg(long)]
    download_dir: Option<std::path::PathBuf>,

    /// Largest single file this machine will accept, in mebibytes.
    #[arg(long)]
    max_file_mib: Option<u64>,

    /// Accept incoming files without asking.
    ///
    /// Off by default and deliberately awkward to turn on: it removes the
    /// human from the loop, and the human is the last check on a paired but
    /// misbehaving device. Intended for unattended test rigs, not for daily
    /// use — a per-device "always allow" setting is the right answer for
    /// that, and does not exist yet.
    #[arg(long)]
    accept_files_without_asking: bool,
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

    // files.v1. Note what is NOT here: an entry in `auto_grant`. Writing a
    // file to someone's disk is a side effect, so the grant is explicit
    // (`anyflow grant <device> files.v1`) per ADR-0008.
    let destination = match args.download_dir {
        Some(dir) => Destination::new(dir),
        None => Destination::default_location(),
    };
    let files_config = FilesConfig {
        destination: destination.clone(),
        max_file_bytes: args
            .max_file_mib
            .map(|mib| mib.saturating_mul(1024 * 1024))
            .unwrap_or(anyflow_capability_files::limits::DEFAULT_MAX_FILE_BYTES),
        ..FilesConfig::default()
    };
    if let Err(e) = destination.prepare() {
        tracing::warn!(error = %e, "could not prepare the download directory");
    }
    tracing::info!(
        download_dir = %destination.dir().display(),
        max_file_bytes = files_config.max_file_bytes,
        "files.v1 ready"
    );

    let approval: Arc<dyn TransferApproval> = if args.accept_files_without_asking {
        tracing::warn!(
            "--accept-files-without-asking is set: incoming files will NOT be \
             confirmed by a human"
        );
        Arc::new(AcceptEverything)
    } else {
        Arc::new(ConsoleApproval)
    };

    let transfers = TransferManager::new(
        // The desktop is the stable listener, so it is always the end that
        // accepts data streams and issues their challenges.
        StreamRole::Acceptor,
        identity_fp,
        files_config,
        approval,
    );

    // clipboard.v1. Like files.v1 it is absent from `auto_grant`: a device
    // that can write your clipboard can also read what you paste next, and
    // ADR-0008 requires a side effect that large to be granted by hand.
    //
    // The backend is probed once here so that `anyflow clipboard status` can
    // report what this session can actually do — including, on GNOME, that it
    // cannot report clipboard changes at all — instead of each command
    // discovering it separately.
    let clipboard_backend = anyflow_capability_clipboard::backend::detect();
    if let Err(why) = clipboard_backend.watch_availability() {
        tracing::info!(
            reason = %why,
            "clipboard auto-send is unavailable on this session; manual \
             `anyflow clipboard send` still works"
        );
    }
    let clipboard = ClipboardManager::new(clipboard_backend, device_id.clone());
    tracing::info!(backend = %clipboard.backend().describe(), "clipboard.v1 ready");

    let registry = CapabilityRegistry::builder()
        .register(Arc::new(battery))
        .register(Arc::new(FilesCapability::new(Arc::clone(&transfers))))
        .register(Arc::new(ClipboardCapability::new(Arc::clone(&clipboard))))
        .build();
    tracing::info!(capabilities = ?registry.advertised(), "capabilities registered");

    // ---- TLS --------------------------------------------------------------
    let tls_config = anyflow_core::tls::server_config(store.identity())?;
    let acceptor = TlsAcceptor::from(tls_config);

    let state = Arc::new(
        DaemonState::new(store, registry, battery_state)
            .with_transfers(Arc::clone(&transfers))
            .with_clipboard(Arc::clone(&clipboard)),
    );

    // The state is the authorizer: every grant question is answered from the
    // trust store, freshly, rather than from a set captured at handshake time.
    transfers
        .set_authorizer(Arc::clone(&state) as Arc<dyn anyflow_capability_files::FilesAuthorizer>)
        .await;
    let _reaper = transfers.spawn_reaper();

    // Same rule for the clipboard: the grant and the per-peer policy are
    // answered from the trust store on every question, never from a set
    // captured at handshake time.
    clipboard
        .set_authorizer(
            Arc::clone(&state) as Arc<dyn anyflow_capability_clipboard::ClipboardAuthorizer>
        )
        .await;
    // Supervised, and idle until some peer actually asks for auto-send: with
    // nobody asking it holds no helper process and no X connection.
    let _clipboard_watcher = clipboard.spawn_watcher();

    // ---- listeners --------------------------------------------------------
    let bound = listener::bind_endpoints(port)?;
    let bound_port = bound.port;
    tracing::info!(
        port = bound_port,
        families = %bound.families,
        sockets = bound.listeners.len(),
        "listening"
    );

    let socket_path = control::control_socket_path();
    let control_listener = server::bind(&socket_path)?;
    tracing::info!(socket = %socket_path.display(), "control socket ready");

    // Held for the process lifetime; dropping it withdraws the mDNS record.
    let _advertisement = if args.no_mdns {
        tracing::info!("mDNS advertisement disabled");
        None
    } else {
        match mdns::Advertisement::publish(&device_id, &device_name, bound_port, bound.families) {
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
    state.set_listen_families(bound.families);

    let net = tokio::spawn(listener::run(bound.listeners, acceptor, Arc::clone(&state)));
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

/// Approval that asks nobody, for an unattended test rig.
struct AcceptEverything;

#[async_trait::async_trait]
impl TransferApproval for AcceptEverything {
    async fn confirm_receive(&self, offer: &anyflow_capability_files::IncomingOffer) -> bool {
        tracing::warn!(
            transfer = %offer.transfer_id,
            filename = %offer.filename,
            "accepting a file without asking (--accept-files-without-asking)"
        );
        true
    }
}

/// The default: refuse, loudly, and tell the operator how to accept.
///
/// The daemon has no terminal of its own — it runs under `systemd --user` —
/// so it cannot prompt. Rather than inventing a silent yes, it declines and
/// logs what happened. A desktop GUI and a `anyflow recv` command are the
/// planned ways to answer; until one exists, `--accept-files-without-asking`
/// is the documented escape hatch for a test rig.
///
/// Declining is the safe direction. An implementation that defaulted to
/// "yes, because nobody is watching" would let any granted peer write to the
/// download directory unattended, which is precisely what receiver approval
/// exists to prevent.
struct ConsoleApproval;

#[async_trait::async_trait]
impl TransferApproval for ConsoleApproval {
    async fn confirm_receive(&self, offer: &anyflow_capability_files::IncomingOffer) -> bool {
        tracing::warn!(
            transfer = %offer.transfer_id,
            filename = %offer.filename,
            size = offer.size_bytes,
            peer = %offer.peer.to_display_short(),
            "declining an incoming file: no way to ask a human. Start the \
             daemon with --accept-files-without-asking to accept unattended."
        );
        false
    }
}
