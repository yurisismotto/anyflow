//! The protocol state machine that runs over an established TLS connection.
//!
//! # States
//!
//! ```text
//!            HELLO / HELLO_ACK
//!   Init ──────────────────────► Trusted?  ──yes──► Established
//!                                    │
//!                                    no
//!                                    │
//!                                    ▼
//!                             PairingRequired
//!                          (PAIR_REQUEST only)
//!                                    │
//!                       proof ok + user confirms
//!                                    │
//!                                    ▼
//!                              Established
//! ```
//!
//! An unauthenticated peer can send exactly two things: `HELLO` and
//! `PAIR_REQUEST`. Anything else — including a `PING` — is a protocol
//! violation that closes the connection. This is the rule that makes
//! "reachable on the LAN" worth nothing (principle: discovery is not trust).
//!
//! # Replay protection
//!
//! Two independent mechanisms, both scoped to a single TLS connection:
//!
//!   * `sequence` must strictly increase. TLS already prevents an off-path
//!     attacker from injecting or reordering records, so this is defence in
//!     depth against a buggy or hostile *peer*, not against the network.
//!   * `message_id` must not have been seen recently (bounded window).
//!
//! Note what is *not* used: `timestamp_unix_ms`. Clocks between a phone and a
//! desktop disagree routinely, and a rule based on them would either be
//! trivially bypassed or would break for honest users in a timezone or NTP
//! edge case.

use std::collections::{HashSet, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};

use rand::TryRngCore;
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt, ReadHalf, WriteHalf};
use tokio::sync::{mpsc, oneshot};

use anyflow_proto::v1;

use crate::capability::{CapabilityContext, CapabilityRegistry, OutboundMessage};
use crate::error::{Error, PairingError, Result};
use crate::fingerprint::Fingerprint;
use crate::framing;

/// Lowest protocol version this build can speak.
pub const PROTOCOL_VERSION_MIN: u32 = 1;
/// Highest protocol version this build can speak.
pub const PROTOCOL_VERSION_MAX: u32 = 1;

/// How long to wait for the peer's HELLO before giving up. Bounds the
/// resources an idle or stalling connection can hold.
pub const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(15);

/// How long to wait for the human to accept or decline a pairing prompt.
pub const PAIRING_CONFIRM_TIMEOUT: Duration = Duration::from_secs(60);

/// Number of recent message ids remembered for de-duplication.
const DEDUP_WINDOW: usize = 1024;

/// Ping payloads larger than this are refused rather than echoed. Without a
/// cap, `PING` would be a free amplification and memory primitive.
const MAX_PING_PAYLOAD: usize = 64;

// ---------------------------------------------------------------------------
// Liveness
// ---------------------------------------------------------------------------
//
// A TCP connection whose peer walks out of Wi-Fi range is not closed: no FIN
// and no RST is ever sent, so `read` simply blocks forever and the session
// looks perfectly healthy from the inside. That is how a dead session ends up
// listed as `connected` (the "zombie session" defect). Linux's own TCP
// keepalive would eventually notice, but its default first probe is two hours
// away, which is not a useful answer for a phone on a home network.
//
// So the session probes at the protocol layer. It costs one 30-odd byte frame
// per minute on an otherwise idle link, which is nothing next to the Wi-Fi
// radio already being awake for the foreground-service connection.

/// How often the session checks whether it has heard from the peer lately.
const LIVENESS_TICK: Duration = Duration::from_secs(15);

/// After this much silence, send a PING to find out whether the peer is
/// still there. Any inbound frame — including the peer's own PING — resets it.
const LIVENESS_PROBE_AFTER: Duration = Duration::from_secs(60);

/// After this much silence the session is declared dead and closed. It is
/// three times [`LIVENESS_PROBE_AFTER`] so that two probes must go unanswered
/// before a link is given up on: a single lost packet must not drop a session.
const LIVENESS_DEAD_AFTER: Duration = Duration::from_secs(180);

/// Silence past this point makes a session worth *reporting* as doubtful,
/// well before [`LIVENESS_DEAD_AFTER`] acts on it. It is longer than
/// [`LIVENESS_PROBE_AFTER`] so that a healthy idle link, which answers its
/// probe, never flickers into doubt.
pub const LIVENESS_STALE_AFTER: Duration = Duration::from_secs(90);

/// What the trust store says about a peer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PeerStatus {
    Trusted {
        device_id: String,
        device_name: String,
        granted_capabilities: Vec<String>,
    },
    Unknown,
    Revoked,
}

/// Everything the session needs from its host application.
///
/// Keeping this a trait is what lets the protocol be tested end-to-end
/// without a daemon, a filesystem, or a human.
#[async_trait::async_trait]
pub trait SessionHost: Send + Sync + 'static {
    fn local_device_info(&self) -> v1::DeviceInfo;
    fn registry(&self) -> CapabilityRegistry;

    async fn lookup_peer(&self, fingerprint: &Fingerprint) -> PeerStatus;

    /// True while a pairing window is open and usable.
    async fn pairing_mode_active(&self) -> bool;

    /// Verifies a pairing proof and, on success, consumes the window.
    /// Returns the confirmation MAC to send back.
    async fn verify_pairing_proof(
        &self,
        initiator: &Fingerprint,
        nonce: &[u8],
        proof: &[u8],
    ) -> std::result::Result<[u8; 32], PairingError>;

    /// Asks the human to confirm. Implementations must show the peer's short
    /// fingerprint so the user can compare it against the other device.
    async fn confirm_pairing(&self, device: &v1::DeviceInfo, fingerprint: &Fingerprint) -> bool;

    /// Persists a newly paired peer.
    async fn store_peer(
        &self,
        device: &v1::DeviceInfo,
        fingerprint: &Fingerprint,
        negotiated_capabilities: &[String],
        protocol_version: u32,
    ) -> Result<()>;

    /// Notifies the host that a session became fully established.
    async fn on_established(&self, _peer: &Fingerprint, _handle: SessionHandle) {}

    /// Notifies the host that a session ended.
    ///
    /// `session_id` identifies *which* session ended, and hosts must use it.
    /// A peer that reconnects before its previous socket has finished dying
    /// produces two overlapping sessions for one fingerprint; a host that
    /// unregistered by fingerprint alone would let the older session's late
    /// close evict the newer, live one and report the device as offline while
    /// it is connected.
    async fn on_closed(&self, _peer: &Fingerprint, _session_id: SessionId) {}
}

/// Identifies one session, distinctly from any other session with the same
/// peer. Process-local and never sent on the wire.
pub type SessionId = u64;

fn next_session_id() -> SessionId {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// A command sent into a running session from outside.
#[derive(Debug)]
pub enum Command {
    Ping { reply: oneshot::Sender<Duration> },
    SendCapability(OutboundMessage),
    Shutdown,
}

/// Outside-world handle to a live session.
#[derive(Clone)]
pub struct SessionHandle {
    id: SessionId,
    peer: Fingerprint,
    device_id: String,
    device_name: String,
    negotiated_capabilities: Arc<Vec<String>>,
    commands: mpsc::Sender<Command>,
    /// When the session began, and how long ago the last frame arrived,
    /// as milliseconds since that start. Shared with the running loop so
    /// callers can ask how live a session is without messaging it.
    started: tokio::time::Instant,
    last_inbound_ms: Arc<std::sync::atomic::AtomicU64>,
}

impl SessionHandle {
    /// Which session this is. Two sessions with the same peer never share one.
    pub fn id(&self) -> SessionId {
        self.id
    }

    pub fn peer(&self) -> Fingerprint {
        self.peer
    }

    /// False once the session's message loop has stopped receiving commands,
    /// which happens as soon as it ends for any reason.
    pub fn is_live(&self) -> bool {
        !self.commands.is_closed()
    }

    /// How long since anything at all arrived from the peer.
    ///
    /// This, not the age of some capability's last reading, is what says
    /// whether a session is healthy. An idle link answering its liveness
    /// probes is perfectly alive even though nothing has changed on it; a
    /// link whose battery happens not to have moved is not sick.
    pub fn silent_for(&self) -> Duration {
        let last = Duration::from_millis(
            self.last_inbound_ms
                .load(std::sync::atomic::Ordering::Relaxed),
        );
        self.started.elapsed().saturating_sub(last)
    }

    /// True once silence has passed [`LIVENESS_STALE_AFTER`], meaning at
    /// least one liveness probe has gone unanswered.
    pub fn is_stale(&self) -> bool {
        self.silent_for() >= LIVENESS_STALE_AFTER
    }
    pub fn device_id(&self) -> &str {
        &self.device_id
    }
    pub fn device_name(&self) -> &str {
        &self.device_name
    }
    pub fn negotiated_capabilities(&self) -> &[String] {
        &self.negotiated_capabilities
    }

    /// Round-trips a PING. `None` if the session ended first.
    pub async fn ping(&self, timeout: Duration) -> Option<Duration> {
        let (tx, rx) = oneshot::channel();
        self.commands.send(Command::Ping { reply: tx }).await.ok()?;
        tokio::time::timeout(timeout, rx).await.ok()?.ok()
    }

    pub async fn send_capability(&self, msg: OutboundMessage) -> bool {
        self.commands
            .send(Command::SendCapability(msg))
            .await
            .is_ok()
    }

    pub async fn shutdown(&self) {
        let _ = self.commands.send(Command::Shutdown).await;
    }
}

/// Result of the handshake half of a session.
pub struct Established {
    pub peer: Fingerprint,
    pub device: v1::DeviceInfo,
    pub negotiated_capabilities: Vec<String>,
    pub protocol_version: u32,
}

// ---------------------------------------------------------------------------
// Envelope construction
// ---------------------------------------------------------------------------

fn random_message_id() -> Vec<u8> {
    let mut buf = [0u8; 16];
    // A failure of the OS CSPRNG is not recoverable and must not be papered
    // over with a predictable id, so fail loudly by poisoning the id with a
    // value that will not pass de-duplication.
    if rand::rngs::OsRng.try_fill_bytes(&mut buf).is_err() {
        return Vec::new();
    }
    buf.to_vec()
}

fn now_unix_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

struct EnvelopeFactory {
    protocol_version: u32,
    sequence: u64,
}

impl EnvelopeFactory {
    fn new(protocol_version: u32) -> Self {
        Self {
            protocol_version,
            sequence: 0,
        }
    }

    fn build(&mut self, body: v1::envelope::Body) -> v1::Envelope {
        self.sequence += 1;
        v1::Envelope {
            protocol_version: self.protocol_version,
            message_id: random_message_id(),
            sequence: self.sequence,
            timestamp_unix_ms: now_unix_ms(),
            correlation_id: Vec::new(),
            body: Some(body),
        }
    }

    fn build_reply(&mut self, body: v1::envelope::Body, correlate: &[u8]) -> v1::Envelope {
        let mut e = self.build(body);
        e.correlation_id = correlate.to_vec();
        e
    }
}

// ---------------------------------------------------------------------------
// Replay guard
// ---------------------------------------------------------------------------

struct ReplayGuard {
    last_sequence: u64,
    seen: HashSet<Vec<u8>>,
    order: VecDeque<Vec<u8>>,
}

impl ReplayGuard {
    fn new() -> Self {
        Self {
            last_sequence: 0,
            seen: HashSet::new(),
            order: VecDeque::new(),
        }
    }

    /// `Ok(())` if this envelope is fresh; `Err` describes the violation.
    fn admit(&mut self, envelope: &v1::Envelope) -> Result<()> {
        if envelope.message_id.len() != 16 {
            return Err(Error::Protocol("message_id must be 16 bytes"));
        }
        if envelope.sequence <= self.last_sequence {
            return Err(Error::Protocol("sequence number did not increase"));
        }
        if !self.seen.insert(envelope.message_id.clone()) {
            return Err(Error::Protocol("duplicate message_id"));
        }

        self.last_sequence = envelope.sequence;
        self.order.push_back(envelope.message_id.clone());
        if self.order.len() > DEDUP_WINDOW {
            if let Some(old) = self.order.pop_front() {
                self.seen.remove(&old);
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Handshake: responder (server) side
// ---------------------------------------------------------------------------

/// Negotiates a protocol version, or `None` if the ranges do not overlap.
pub fn negotiate_version(peer_min: u32, peer_max: u32) -> Option<u32> {
    let chosen = peer_max.min(PROTOCOL_VERSION_MAX);
    let floor = peer_min.max(PROTOCOL_VERSION_MIN);
    if chosen >= floor {
        Some(chosen)
    } else {
        None
    }
}

/// Runs the responder half of the handshake.
///
/// `peer_fingerprint` must come from the completed TLS handshake, never from
/// anything the peer asserted in a message.
pub async fn accept_handshake<S>(
    stream: &mut S,
    host: &Arc<dyn SessionHost>,
    peer_fingerprint: Fingerprint,
) -> Result<(Established, EnvelopeState)>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let mut guard = ReplayGuard::new();

    let hello_env = tokio::time::timeout(HANDSHAKE_TIMEOUT, framing::read_envelope(stream))
        .await
        .map_err(|_| Error::Protocol("timed out waiting for HELLO"))??;
    guard.admit(&hello_env)?;

    let hello = match hello_env.body {
        Some(v1::envelope::Body::Hello(h)) => h,
        _ => return Err(Error::Protocol("expected HELLO as the first message")),
    };

    let device = hello
        .device
        .clone()
        .ok_or(Error::Protocol("HELLO without device info"))?;

    // The peer's claimed fingerprint must match the key it actually
    // authenticated with. Otherwise a trusted device could be impersonated in
    // the application layer while a different key held the TLS session.
    let claimed = Fingerprint::from_hex(&device.identity_fingerprint)
        .map_err(|_| Error::Protocol("HELLO fingerprint is malformed"))?;
    if claimed != peer_fingerprint {
        return Err(Error::Protocol(
            "HELLO fingerprint does not match the TLS peer certificate",
        ));
    }

    let registry = host.registry();
    let local_info = host.local_device_info();

    // Version negotiation happens before anything else is trusted.
    let Some(version) = negotiate_version(hello.min_protocol_version, hello.max_protocol_version)
    else {
        let mut factory = EnvelopeFactory::new(PROTOCOL_VERSION_MAX);
        let ack = factory.build_reply(
            v1::envelope::Body::HelloAck(v1::HelloAck {
                device: Some(local_info),
                status: v1::HelloStatus::VersionUnsupported as i32,
                negotiated_protocol_version: 0,
                capabilities: Vec::new(),
                pairing_nonce: Vec::new(),
            }),
            &hello_env.message_id,
        );
        let _ = framing::write_envelope(stream, &ack).await;
        return Err(Error::VersionUnsupported);
    };

    let mut factory = EnvelopeFactory::new(version);
    let status = host.lookup_peer(&peer_fingerprint).await;

    // A revoked device is refused — unless the owner has deliberately opened a
    // pairing window, in which case it may pair again from scratch.
    //
    // Revocation must be permanent against the *device*, not against the
    // person holding it. Refusing a revoked fingerprint unconditionally made
    // revocation irreversible: the phone could never be paired again, because
    // it was rejected before it was ever offered the chance to prove it holds
    // a fresh token. That is a foot-gun, not a security property.
    //
    // Nothing is weakened by letting it through here. It takes the same path
    // as a device that was never known: it must present a proof of the
    // single-use token that was generated seconds ago, and a human at the
    // desktop must confirm its fingerprint. Reachability still grants
    // nothing, and a revoked device with no pairing window open is still
    // rejected outright.
    let treat_as_unknown =
        matches!(status, PeerStatus::Revoked) && host.pairing_mode_active().await;

    match status {
        PeerStatus::Revoked if !treat_as_unknown => {
            let ack = factory.build_reply(
                v1::envelope::Body::HelloAck(v1::HelloAck {
                    device: Some(local_info),
                    status: v1::HelloStatus::Rejected as i32,
                    negotiated_protocol_version: version,
                    capabilities: Vec::new(),
                    pairing_nonce: Vec::new(),
                }),
                &hello_env.message_id,
            );
            let _ = framing::write_envelope(stream, &ack).await;
            Err(Error::NotAuthorized)
        }

        PeerStatus::Trusted {
            device_id,
            device_name,
            granted_capabilities,
        } => {
            // Two filters, deliberately separate: what both sides *can* do,
            // intersected with what this peer is *allowed* to do.
            let mutual = registry.negotiate(&hello.capabilities);
            let effective: Vec<String> = mutual
                .into_iter()
                .filter(|c| granted_capabilities.iter().any(|g| g == c))
                .collect();

            let ack = factory.build_reply(
                v1::envelope::Body::HelloAck(v1::HelloAck {
                    device: Some(local_info),
                    status: v1::HelloStatus::Trusted as i32,
                    negotiated_protocol_version: version,
                    capabilities: registry.advertised(),
                    pairing_nonce: Vec::new(),
                }),
                &hello_env.message_id,
            );
            framing::write_envelope(stream, &ack).await?;

            let mut info = device;
            info.device_id = device_id;
            info.device_name = device_name;

            Ok((
                Established {
                    peer: peer_fingerprint,
                    device: info,
                    negotiated_capabilities: effective,
                    protocol_version: version,
                },
                EnvelopeState { factory, guard },
            ))
        }

        PeerStatus::Unknown | PeerStatus::Revoked => {
            if matches!(status, PeerStatus::Revoked) {
                tracing::info!(
                    peer = %peer_fingerprint.to_display_short(),
                    "a revoked device is pairing again; it must prove the new \
                     token and be confirmed by hand"
                );
            }

            let active = host.pairing_mode_active().await;
            let nonce = if active {
                crate::pairing::generate_nonce()?.to_vec()
            } else {
                Vec::new()
            };

            let ack = factory.build_reply(
                v1::envelope::Body::HelloAck(v1::HelloAck {
                    device: Some(local_info),
                    status: v1::HelloStatus::PairingRequired as i32,
                    negotiated_protocol_version: version,
                    capabilities: registry.advertised(),
                    pairing_nonce: nonce.clone(),
                }),
                &hello_env.message_id,
            );
            framing::write_envelope(stream, &ack).await?;

            if !active {
                // Nothing further is permitted from an unknown peer while no
                // pairing window is open.
                return Err(Error::Pairing(PairingError::NotInPairingMode));
            }

            let established = accept_pairing(
                stream,
                host,
                &mut factory,
                &mut guard,
                peer_fingerprint,
                &device,
                &nonce,
                version,
                &hello.capabilities,
            )
            .await?;

            Ok((established, EnvelopeState { factory, guard }))
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn accept_pairing<S>(
    stream: &mut S,
    host: &Arc<dyn SessionHost>,
    factory: &mut EnvelopeFactory,
    guard: &mut ReplayGuard,
    peer_fingerprint: Fingerprint,
    device: &v1::DeviceInfo,
    nonce: &[u8],
    version: u32,
    peer_capabilities: &[String],
) -> Result<Established>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let env = tokio::time::timeout(HANDSHAKE_TIMEOUT, framing::read_envelope(stream))
        .await
        .map_err(|_| Error::Protocol("timed out waiting for PAIR_REQUEST"))??;
    guard.admit(&env)?;

    if env.protocol_version != version {
        return Err(Error::Protocol("protocol version changed mid-connection"));
    }

    let request = match env.body {
        Some(v1::envelope::Body::PairRequest(r)) => r,
        // An unknown peer that tries anything else is not confused, it is
        // probing. Close.
        _ => return Err(Error::Protocol("expected PAIR_REQUEST from unknown peer")),
    };

    let reject = |factory: &mut EnvelopeFactory, status: v1::PairStatus| {
        factory.build_reply(
            v1::envelope::Body::PairResponse(v1::PairResponse {
                status: status as i32,
                confirmation: Vec::new(),
            }),
            &env.message_id,
        )
    };

    let confirmation = match host
        .verify_pairing_proof(&peer_fingerprint, nonce, &request.proof)
        .await
    {
        Ok(c) => c,
        Err(e) => {
            let status = match e {
                PairingError::NotInPairingMode => v1::PairStatus::NotInPairingMode,
                PairingError::RateLimited => v1::PairStatus::RateLimited,
                _ => v1::PairStatus::Rejected,
            };
            let resp = reject(factory, status);
            let _ = framing::write_envelope(stream, &resp).await;
            return Err(Error::Pairing(e));
        }
    };

    // Only after the proof verifies do we bother a human. Asking first would
    // turn any stranger on the network into a source of prompts.
    let confirmed = tokio::time::timeout(
        PAIRING_CONFIRM_TIMEOUT,
        host.confirm_pairing(device, &peer_fingerprint),
    )
    .await
    .unwrap_or(false);

    if !confirmed {
        let resp = reject(factory, v1::PairStatus::DeclinedByUser);
        let _ = framing::write_envelope(stream, &resp).await;
        return Err(Error::Pairing(PairingError::DeclinedByUser));
    }

    let mutual = host.registry().negotiate(peer_capabilities);
    host.store_peer(device, &peer_fingerprint, &mutual, version)
        .await?;

    let resp = factory.build_reply(
        v1::envelope::Body::PairResponse(v1::PairResponse {
            status: v1::PairStatus::Accepted as i32,
            confirmation: confirmation.to_vec(),
        }),
        &env.message_id,
    );
    framing::write_envelope(stream, &resp).await?;

    // Re-read the grants: `store_peer` decides what is auto-granted.
    let granted = match host.lookup_peer(&peer_fingerprint).await {
        PeerStatus::Trusted {
            granted_capabilities,
            ..
        } => granted_capabilities,
        _ => Vec::new(),
    };
    let effective = mutual
        .into_iter()
        .filter(|c| granted.iter().any(|g| g == c))
        .collect();

    Ok(Established {
        peer: peer_fingerprint,
        device: device.clone(),
        negotiated_capabilities: effective,
        protocol_version: version,
    })
}

/// Sequence/replay state carried from the handshake into the message loop.
pub struct EnvelopeState {
    factory: EnvelopeFactory,
    guard: ReplayGuard,
}

// ---------------------------------------------------------------------------
// Message loop
// ---------------------------------------------------------------------------

/// Runs the established-session message loop until the connection ends.
pub async fn run_session<S>(
    stream: S,
    host: Arc<dyn SessionHost>,
    established: Established,
    state: EnvelopeState,
) -> Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let EnvelopeState {
        mut factory,
        mut guard,
    } = state;

    let (mut reader, mut writer): (ReadHalf<S>, WriteHalf<S>) = tokio::io::split(stream);

    // The reader lives in its own task rather than as a branch of the
    // `select!` below. `read_exact` is NOT cancellation-safe: if `select!`
    // dropped a half-completed frame read to service an outbound message, the
    // bytes already consumed would be lost and every subsequent frame would
    // be misaligned. Channel receives are cancellation-safe, so the loop
    // selects on a channel instead and the read is never interrupted.
    let (inbound_tx, mut inbound_rx) = mpsc::channel::<Result<v1::Envelope>>(8);
    let reader_task = tokio::spawn(async move {
        loop {
            let item = framing::read_envelope(&mut reader).await;
            let fatal = item.is_err();
            if inbound_tx.send(item).await.is_err() || fatal {
                break;
            }
        }
    });

    let (cmd_tx, mut cmd_rx) = mpsc::channel::<Command>(32);
    let (cap_tx, mut cap_rx) = mpsc::channel::<OutboundMessage>(32);

    let session_id = next_session_id();
    let started = tokio::time::Instant::now();
    let last_inbound_ms = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let handle = SessionHandle {
        id: session_id,
        peer: established.peer,
        device_id: established.device.device_id.clone(),
        device_name: established.device.device_name.clone(),
        negotiated_capabilities: Arc::new(established.negotiated_capabilities.clone()),
        commands: cmd_tx,
        started,
        last_inbound_ms: Arc::clone(&last_inbound_ms),
    };

    let registry = host.registry();
    let ctx = CapabilityContext {
        peer: established.peer,
        peer_device_id: established.device.device_id.clone(),
        outbound: cap_tx.clone(),
    };

    for id in &established.negotiated_capabilities {
        if let Some(cap) = registry.get(id) {
            if let Err(e) = cap.on_peer_connected(&ctx).await {
                tracing::warn!(capability = %id, error = %e, "capability failed on connect");
            }
        }
    }

    host.on_established(&established.peer, handle.clone()).await;

    let mut pending_ping: Option<(Vec<u8>, Instant, oneshot::Sender<Duration>)> = None;

    // Liveness. `last_inbound` is reset by *any* frame from the peer, so a
    // busy session never probes and an idle one probes once a minute.
    //
    // `tokio::time::Instant`, not `std::time::Instant`: it is the same clock
    // the interval below runs on, so the two cannot disagree — and it lets a
    // test drive three minutes of silence without waiting three minutes.
    let mut last_inbound = started;
    let mut liveness = tokio::time::interval(LIVENESS_TICK);
    liveness.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    // The first tick of a tokio interval completes immediately; consume it so
    // the first real check happens one tick from now.
    liveness.tick().await;

    let result = loop {
        tokio::select! {
            // Biased so that inbound traffic is always drained before new
            // outbound work is queued; keeps a chatty peer from starving the
            // reader and growing buffers.
            biased;

            incoming = inbound_rx.recv() => {
                let env = match incoming {
                    Some(Ok(e)) => e,
                    Some(Err(Error::Closed)) | None => break Ok(()),
                    Some(Err(e)) => break Err(e),
                };
                last_inbound = tokio::time::Instant::now();
                last_inbound_ms.store(
                    (last_inbound - started).as_millis() as u64,
                    std::sync::atomic::Ordering::Relaxed,
                );

                if env.protocol_version != established.protocol_version {
                    break Err(Error::Protocol("protocol version changed mid-connection"));
                }
                if let Err(e) = guard.admit(&env) {
                    // A replayed or duplicated frame is treated as fatal
                    // rather than ignored: a well-behaved peer never produces
                    // one, so it means either a bug or an attack.
                    break Err(e);
                }

                match env.body {
                    Some(v1::envelope::Body::Ping(p)) => {
                        if p.payload.len() > MAX_PING_PAYLOAD {
                            break Err(Error::Protocol("ping payload too large"));
                        }
                        let pong = factory.build_reply(
                            v1::envelope::Body::Pong(v1::Pong { payload: p.payload }),
                            &env.message_id,
                        );
                        if let Err(e) = framing::write_envelope(&mut writer, &pong).await {
                            break Err(e);
                        }
                    }

                    Some(v1::envelope::Body::Pong(_)) => {
                        if let Some((id, sent_at, reply)) = pending_ping.take() {
                            if env.correlation_id == id {
                                let _ = reply.send(sent_at.elapsed());
                            } else {
                                // Not ours: put it back so the real reply can
                                // still land.
                                pending_ping = Some((id, sent_at, reply));
                            }
                        }
                    }

                    Some(v1::envelope::Body::CapabilityMessage(m)) => {
                        // Authorization is re-checked here, per message, from
                        // the negotiated+granted list. It is never inferred
                        // from the message itself.
                        if !established.negotiated_capabilities.contains(&m.capability_id) {
                            let err = factory.build_reply(
                                v1::envelope::Body::Error(v1::Error {
                                    code: v1::ErrorCode::UnsupportedCapability as i32,
                                    message: "capability not negotiated".into(),
                                    fatal: false,
                                }),
                                &env.message_id,
                            );
                            if let Err(e) = framing::write_envelope(&mut writer, &err).await {
                                break Err(e);
                            }
                            continue;
                        }

                        if let Some(cap) = registry.get(&m.capability_id) {
                            if let Err(e) = cap.on_message(&ctx, &m.payload).await {
                                // Logged without the payload: capability
                                // payloads may carry user content.
                                tracing::warn!(
                                    capability = %m.capability_id,
                                    error = %e,
                                    "capability rejected a message"
                                );
                            }
                        }
                    }

                    Some(v1::envelope::Body::CapabilityAnnounce(_)) => {
                        // Accepted and ignored in v1: re-negotiating mid
                        // connection is not needed yet, and silently widening
                        // permissions from a peer's assertion would be wrong.
                    }

                    Some(v1::envelope::Body::Error(e)) => {
                        tracing::warn!(code = e.code, fatal = e.fatal, "peer reported an error");
                        if e.fatal {
                            break Err(Error::PeerError(
                                v1::ErrorCode::try_from(e.code)
                                    .unwrap_or(v1::ErrorCode::Unspecified),
                            ));
                        }
                    }

                    // HELLO / PAIR_* after the handshake, or an empty body
                    // from a newer peer: not acceptable on an established
                    // session.
                    _ => break Err(Error::Protocol("unexpected message on established session")),
                }
            }

            Some(cmd) = cmd_rx.recv() => {
                match cmd {
                    Command::Shutdown => break Ok(()),
                    Command::Ping { reply } => {
                        let mut payload = [0u8; 8];
                        let _ = rand::rngs::OsRng.try_fill_bytes(&mut payload);
                        let env = factory.build(v1::envelope::Body::Ping(v1::Ping {
                            payload: payload.to_vec(),
                        }));
                        let id = env.message_id.clone();
                        if let Err(e) = framing::write_envelope(&mut writer, &env).await {
                            break Err(e);
                        }
                        pending_ping = Some((id, Instant::now(), reply));
                    }
                    Command::SendCapability(msg) => {
                        if let Err(e) = send_capability(&mut writer, &mut factory, msg).await {
                            break Err(e);
                        }
                    }
                }
            }

            Some(msg) = cap_rx.recv() => {
                if let Err(e) = send_capability(&mut writer, &mut factory, msg).await {
                    break Err(e);
                }
            }

            _ = liveness.tick() => {
                let silent_for = last_inbound.elapsed();
                if silent_for >= LIVENESS_DEAD_AFTER {
                    // Nothing has arrived for long enough that two probes
                    // went unanswered. The socket is half-open: the peer is
                    // gone but the kernel has no way to know it. Ending the
                    // session here is what stops it being reported as live.
                    tracing::info!(
                        peer = %established.peer.to_display_short(),
                        silent_secs = silent_for.as_secs(),
                        "peer stopped responding; closing session"
                    );
                    break Err(Error::Protocol("peer stopped responding"));
                }
                if silent_for >= LIVENESS_PROBE_AFTER {
                    let mut payload = [0u8; 8];
                    let _ = rand::rngs::OsRng.try_fill_bytes(&mut payload);
                    let env = factory.build(v1::envelope::Body::Ping(v1::Ping {
                        payload: payload.to_vec(),
                    }));
                    // A probe carries no reply channel: the PONG is not
                    // matched to anything, it only has to arrive, and any
                    // frame at all is proof enough that the peer is there.
                    if let Err(e) = framing::write_envelope(&mut writer, &env).await {
                        break Err(e);
                    }
                }
            }

            else => break Ok(()),
        }
    };

    for id in &established.negotiated_capabilities {
        if let Some(cap) = registry.get(id) {
            let _ = cap.on_peer_disconnected(&established.peer).await;
        }
    }
    host.on_closed(&established.peer, session_id).await;
    reader_task.abort();
    let _ = writer.shutdown().await;

    result
}

async fn send_capability<W: AsyncWrite + Unpin>(
    writer: &mut W,
    factory: &mut EnvelopeFactory,
    msg: OutboundMessage,
) -> Result<()> {
    let env = factory.build(v1::envelope::Body::CapabilityMessage(
        v1::CapabilityMessage {
            capability_id: msg.capability_id,
            payload: msg.payload,
        },
    ));
    framing::write_envelope(writer, &env).await
}

// ---------------------------------------------------------------------------
// Initiator (client) side
// ---------------------------------------------------------------------------

/// Outcome of a client handshake.
///
/// The size difference between the variants is deliberate and harmless: this
/// value is produced exactly once per connection and consumed immediately, so
/// boxing it would add an allocation to buy nothing.
#[allow(clippy::large_enum_variant)]
pub enum ClientHandshake {
    Established(Established, EnvelopeState),
    /// Server does not know us and we had no pairing token to offer.
    PairingRequired,
}

/// Runs the initiator half of the handshake.
///
/// If `pairing` is provided, a `PAIR_REQUEST` is sent when the responder asks
/// for one. The responder's `confirmation` MAC is verified before the session
/// is considered established, so a responder that did not know the token is
/// rejected even though it already passed the pinned-key check.
pub async fn connect_handshake<S>(
    stream: &mut S,
    host: &Arc<dyn SessionHost>,
    server_fingerprint: Fingerprint,
    pairing: Option<&crate::pairing::PairingToken>,
) -> Result<ClientHandshake>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let local_info = host.local_device_info();
    let local_fp = Fingerprint::from_hex(&local_info.identity_fingerprint)?;
    let registry = host.registry();

    let mut factory = EnvelopeFactory::new(PROTOCOL_VERSION_MAX);
    let mut guard = ReplayGuard::new();

    let hello = factory.build(v1::envelope::Body::Hello(v1::Hello {
        device: Some(local_info.clone()),
        min_protocol_version: PROTOCOL_VERSION_MIN,
        max_protocol_version: PROTOCOL_VERSION_MAX,
        capabilities: registry.advertised(),
    }));
    framing::write_envelope(stream, &hello).await?;

    let ack_env = tokio::time::timeout(HANDSHAKE_TIMEOUT, framing::read_envelope(stream))
        .await
        .map_err(|_| Error::Protocol("timed out waiting for HELLO_ACK"))??;
    guard.admit(&ack_env)?;

    let ack = match ack_env.body {
        Some(v1::envelope::Body::HelloAck(a)) => a,
        _ => return Err(Error::Protocol("expected HELLO_ACK")),
    };

    let remote = ack
        .device
        .clone()
        .ok_or(Error::Protocol("HELLO_ACK without device info"))?;
    let claimed = Fingerprint::from_hex(&remote.identity_fingerprint)
        .map_err(|_| Error::Protocol("HELLO_ACK fingerprint is malformed"))?;
    if claimed != server_fingerprint {
        return Err(Error::Protocol(
            "HELLO_ACK fingerprint does not match the pinned server identity",
        ));
    }

    let status = v1::HelloStatus::try_from(ack.status).unwrap_or(v1::HelloStatus::Unspecified);
    let version = ack.negotiated_protocol_version;

    match status {
        v1::HelloStatus::VersionUnsupported => return Err(Error::VersionUnsupported),
        v1::HelloStatus::Rejected => return Err(Error::NotAuthorized),
        v1::HelloStatus::Trusted => {
            if !(PROTOCOL_VERSION_MIN..=PROTOCOL_VERSION_MAX).contains(&version) {
                return Err(Error::VersionUnsupported);
            }
            factory.protocol_version = version;
            let mutual = registry.negotiate(&ack.capabilities);
            return Ok(ClientHandshake::Established(
                Established {
                    peer: server_fingerprint,
                    device: remote,
                    negotiated_capabilities: mutual,
                    protocol_version: version,
                },
                EnvelopeState { factory, guard },
            ));
        }
        v1::HelloStatus::PairingRequired => {}
        v1::HelloStatus::Unspecified => {
            return Err(Error::Protocol("HELLO_ACK with unspecified status"))
        }
    }

    let Some(token) = pairing else {
        return Ok(ClientHandshake::PairingRequired);
    };
    if ack.pairing_nonce.len() != crate::pairing::NONCE_LEN {
        return Err(Error::Pairing(PairingError::NotInPairingMode));
    }
    if !(PROTOCOL_VERSION_MIN..=PROTOCOL_VERSION_MAX).contains(&version) {
        return Err(Error::VersionUnsupported);
    }
    factory.protocol_version = version;

    let proof =
        crate::pairing::compute_proof(token, &server_fingerprint, &local_fp, &ack.pairing_nonce);
    let req = factory.build(v1::envelope::Body::PairRequest(v1::PairRequest {
        proof: proof.to_vec(),
    }));
    framing::write_envelope(stream, &req).await?;

    let resp_env = tokio::time::timeout(
        HANDSHAKE_TIMEOUT + PAIRING_CONFIRM_TIMEOUT,
        framing::read_envelope(stream),
    )
    .await
    .map_err(|_| Error::Protocol("timed out waiting for PAIR_RESPONSE"))??;
    guard.admit(&resp_env)?;

    let resp = match resp_env.body {
        Some(v1::envelope::Body::PairResponse(r)) => r,
        _ => return Err(Error::Protocol("expected PAIR_RESPONSE")),
    };

    let pair_status = v1::PairStatus::try_from(resp.status).unwrap_or(v1::PairStatus::Unspecified);
    if pair_status != v1::PairStatus::Accepted {
        return Err(Error::Pairing(match pair_status {
            v1::PairStatus::DeclinedByUser => PairingError::DeclinedByUser,
            v1::PairStatus::NotInPairingMode => PairingError::NotInPairingMode,
            v1::PairStatus::RateLimited => PairingError::RateLimited,
            _ => PairingError::BadProof,
        }));
    }

    // Mutual: confirm the responder also held the token.
    let expected = crate::pairing::compute_confirmation(
        token,
        &server_fingerprint,
        &local_fp,
        &ack.pairing_nonce,
    );
    if !crate::pairing::verify_proof(&expected, &resp.confirmation) {
        return Err(Error::Pairing(PairingError::BadProof));
    }

    host.store_peer(
        &remote,
        &server_fingerprint,
        &registry.negotiate(&ack.capabilities),
        version,
    )
    .await?;

    let mutual = registry.negotiate(&ack.capabilities);
    Ok(ClientHandshake::Established(
        Established {
            peer: server_fingerprint,
            device: remote,
            negotiated_capabilities: mutual,
            protocol_version: version,
        },
        EnvelopeState { factory, guard },
    ))
}
