//! Asking a human about an incoming file, over the real control socket.
//!
//! The unit tests in `omnibridge_runtime::approval` pin the seam's own rules.
//! These pin the *product*: a phone offers a file, the question reaches a
//! desktop client over the same Unix socket the GUI uses, and the answer that
//! travels back decides what happens to a real TLS data stream and a real
//! file on disk. Nothing here is stubbed to make an outcome happen — the
//! transfer state machine, the hash check and the reaper are the ones that
//! ship.
//!
//! The defect of record: before this, a desktop with a graphical session had
//! no way to answer at all, and certification had to fall back to
//! `--accept-files-without-asking` to complete a single transfer. No test in
//! this file starts a daemon with that flag.

mod common;

use std::sync::Arc;
use std::time::Duration;

use common::*;
use omnibridge_capability_files::transfer::{TransferId, TransferState};
use omnibridge_daemon::control::{Event, Request, Response};
use omnibridge_daemon::server;
use omnibridge_daemon::state::DaemonState;
use omnibridge_proto::v1::capabilities as pb;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

const GRACE: Duration = Duration::from_secs(20);

/// A control socket for one test, in a temp dir of its own.
struct Control {
    path: std::path::PathBuf,
    _dir: tempfile::TempDir,
}

impl Control {
    fn start(state: Arc<DaemonState>) -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("control.sock");
        let listener = server::bind(&path).expect("bind control socket");
        tokio::spawn(async move {
            let _ = server::run(listener, state).await;
        });
        Self { path, _dir: dir }
    }
}

/// A desktop client acting as the approval provider.
///
/// Exactly what the GTK application is from the daemon's point of view: one
/// connection that says `watch_file_offers`, reads prompts, and writes
/// decisions back down the same connection.
struct Provider {
    write: tokio::net::unix::OwnedWriteHalf,
    lines: tokio::io::Lines<BufReader<tokio::net::unix::OwnedReadHalf>>,
}

impl Provider {
    async fn attach(control: &Control) -> Self {
        let stream = UnixStream::connect(&control.path).await.expect("connect");
        let (read, mut write) = stream.into_split();
        let mut bytes = serde_json::to_vec(&Request::WatchFileOffers).expect("encode");
        bytes.push(b'\n');
        write.write_all(&bytes).await.expect("write");
        write.flush().await.expect("flush");

        let mut provider = Self {
            write,
            lines: BufReader::new(read).lines(),
        };
        match provider.next_event().await {
            Event::FileApprovalReady { unattended } => {
                assert!(!unattended, "no test here runs an unattended daemon");
            }
            other => panic!("expected FileApprovalReady, got {other:?}"),
        }
        provider
    }

    /// The next event, or a panic. Bounded so a defect fails the test rather
    /// than hanging the suite.
    async fn next_event(&mut self) -> Event {
        let line = tokio::time::timeout(GRACE, self.lines.next_line())
            .await
            .expect("an event should arrive")
            .expect("read")
            .expect("the daemon should not hang up");
        if let Ok(Response::Error { message }) = serde_json::from_str::<Response>(&line) {
            panic!("the daemon refused the approval stream: {message}");
        }
        serde_json::from_str(&line).expect("decode event")
    }

    /// The next event, or `None` if the daemon stays silent for `window`.
    async fn next_event_or_silence(&mut self, window: Duration) -> Option<Event> {
        let line = tokio::time::timeout(window, self.lines.next_line())
            .await
            .ok()?
            .expect("read")?;
        Some(serde_json::from_str(&line).expect("decode event"))
    }

    async fn decide(&mut self, transfer: &str, accept: bool) {
        let mut bytes = serde_json::to_vec(&Request::FileDecision {
            transfer: transfer.to_string(),
            accept,
        })
        .expect("encode");
        bytes.push(b'\n');
        self.write.write_all(&bytes).await.expect("write");
        self.write.flush().await.expect("flush");
    }

    /// Hangs up, the way a GUI that was closed or crashed does.
    async fn disconnect(self) {
        drop(self.lines);
        let mut write = self.write;
        let _ = write.shutdown().await;
    }
}

fn offer_request(event: Event) -> omnibridge_daemon::control::FileOfferRequest {
    match event {
        Event::FileOfferRequest(request) => request,
        other => panic!("expected a file offer prompt, got {other:?}"),
    }
}

/// A paired phone whose desktop has granted it `files.v1`.
async fn paired(server: &TestServer, phone: &TestClient) -> ConnectedSession {
    let token = server.open_pairing(Duration::from_secs(30)).await;
    let first = phone
        .connect(server.addr, server.fingerprint, Some(&token))
        .await
        .expect("pairing");
    server.set_grant(phone.fingerprint, "files.v1", true).await;
    first.close().await;

    phone
        .connect(server.addr, server.fingerprint, None)
        .await
        .expect("reconnect with the grant in place")
}

/// Offers a file over the raw control channel, so the test can drive the
/// prompt without also driving a data stream.
async fn offer_bytes(session: &ConnectedSession, id: TransferId, name: &str, payload: &[u8]) {
    send_files_control(
        session,
        pb::file_control::Body::Offer(pb::FileOffer {
            transfer_id: id.to_vec(),
            filename: name.into(),
            size_bytes: payload.len() as u64,
            mime_type: "text/plain".into(),
            sha256: sha256_of(payload).to_vec(),
            timestamp_unix_ms: 0,
        }),
    )
    .await;
}

// ---------------------------------------------------------------------------
// The default, and the override
// ---------------------------------------------------------------------------

/// O. A desktop nobody is watching declines, exactly as it did before this
/// sprint. The seam existing is not the same as somebody answering it.
#[tokio::test]
async fn with_no_provider_attached_the_desktop_still_declines() {
    let server = TestServer::start_with_approval_provider().await;
    let _control = Control::start(Arc::clone(&server.state));
    let (phone, captured) = TestClient::new_raw("phone");
    let session = paired(&server, &phone).await;

    let id = TransferId::from_bytes(&[0x11; 16]).expect("id");
    offer_bytes(&session, id, "unwatched.txt", b"nobody is home").await;

    match captured.next_control(GRACE).await.body {
        Some(pb::file_control::Body::Cancel(c)) => {
            assert_eq!(c.reason(), pb::TransferFailureReason::DeclinedByUser);
        }
        other => panic!("expected a decline, got {other:?}"),
    }

    let snapshot = server.transfers.snapshot_one(id).await.expect("record");
    assert_eq!(snapshot.state, TransferState::Cancelled);
    assert!(server.completed_files().is_empty());
    assert!(server.partial_files().is_empty());
    session.close().await;
}

/// Attaching a provider is not, by itself, an acceptance. The daemon has to
/// wait for an answer, and the phone must see nothing until it gets one.
#[tokio::test]
async fn attaching_a_provider_does_not_decide_anything_on_its_own() {
    let server = TestServer::start_with_approval_provider().await;
    let control = Control::start(Arc::clone(&server.state));
    let mut provider = Provider::attach(&control).await;
    let (phone, captured) = TestClient::new_raw("phone");
    let session = paired(&server, &phone).await;

    let id = TransferId::from_bytes(&[0x12; 16]).expect("id");
    offer_bytes(&session, id, "waiting.txt", b"waiting").await;

    let request = offer_request(provider.next_event().await);
    assert_eq!(request.transfer_id, id.to_hex());

    // The prompt is on screen and unanswered: the phone has heard nothing.
    assert!(
        captured
            .next_control_or_silence(Duration::from_millis(400))
            .await
            .is_none(),
        "no verdict may travel before a human answers"
    );
    let snapshot = server.transfers.snapshot_one(id).await.expect("record");
    assert_eq!(snapshot.state, TransferState::WaitingAccept);

    session.close().await;
}

// ---------------------------------------------------------------------------
// The two answers, end to end
// ---------------------------------------------------------------------------

/// C and M. The sprint's headline: a real file, accepted at a real prompt,
/// arriving through the existing sink with its hash intact — and the daemon
/// was never started with `--accept-files-without-asking`.
#[tokio::test]
async fn accepting_at_the_prompt_completes_the_transfer_through_the_existing_sink() {
    let server = TestServer::start_with_approval_provider().await;
    let control = Control::start(Arc::clone(&server.state));
    let mut provider = Provider::attach(&control).await;
    let phone = TestClient::new("Galaxy S25");
    let session = paired(&server, &phone).await;

    let dir = tempfile::tempdir().expect("tempdir");
    let source = dir.path().join("holiday photo.jpg");
    let payload = write_sample_file(&source, 300_000);

    let id = phone
        .transfers
        .offer_file(server.fingerprint, source)
        .await
        .expect("offer");

    // What the human is shown, and that it is enough to decide on.
    let request = offer_request(provider.next_event().await);
    assert_eq!(request.transfer_id, id.to_hex());
    assert_eq!(request.filename, "holiday photo.jpg");
    assert_eq!(request.size_bytes, payload.len() as u64);
    assert_eq!(request.fingerprint, phone.fingerprint.to_hex());
    assert_eq!(request.device_name, "Galaxy S25");

    provider.decide(&request.transfer_id, true).await;

    let snapshot = wait_for_terminal(&phone.transfers, id, GRACE).await;
    assert_eq!(
        snapshot.state,
        TransferState::Completed,
        "{:?}",
        snapshot.failure
    );

    // Through the existing destination, byte for byte. The provider is
    // consent; it is not on the data path.
    let stored = server.downloads.join("holiday photo.jpg");
    assert!(stored.exists(), "stored: {:?}", server.received_files());
    let written = std::fs::read(&stored).expect("read");
    assert_eq!(sha256_of(&written), sha256_of(&payload));
    assert!(server.partial_files().is_empty());

    // And the switch was never consulted: this ran on the production seam.
    assert_eq!(server.approvals.asked(), 0);
    session.close().await;
}

/// D and N. Declining is terminal, tells the phone, and leaves nothing.
#[tokio::test]
async fn declining_at_the_prompt_leaves_no_file_and_tells_the_phone() {
    let server = TestServer::start_with_approval_provider().await;
    let control = Control::start(Arc::clone(&server.state));
    let mut provider = Provider::attach(&control).await;
    let (phone, captured) = TestClient::new_raw("phone");
    let session = paired(&server, &phone).await;

    let id = TransferId::from_bytes(&[0x21; 16]).expect("id");
    offer_bytes(&session, id, "unwanted.txt", b"unwanted").await;

    let request = offer_request(provider.next_event().await);
    provider.decide(&request.transfer_id, false).await;

    match captured.next_control(GRACE).await.body {
        Some(pb::file_control::Body::Cancel(c)) => {
            assert_eq!(c.reason(), pb::TransferFailureReason::DeclinedByUser);
        }
        other => panic!("expected a decline, got {other:?}"),
    }

    let snapshot = server.transfers.snapshot_one(id).await.expect("record");
    assert_eq!(snapshot.state, TransferState::Cancelled);
    assert!(server.completed_files().is_empty());
    assert!(server.partial_files().is_empty());
    session.close().await;
}

/// E. Closing the prompt is the same act as declining. A GUI that dismisses a
/// dialog sends the decline, and the daemon's own fallback — the connection
/// ending with the question unanswered — declines too.
#[tokio::test]
async fn a_provider_that_hangs_up_mid_prompt_declines_rather_than_accepting() {
    let server = TestServer::start_with_approval_provider().await;
    let control = Control::start(Arc::clone(&server.state));
    let mut provider = Provider::attach(&control).await;
    let (phone, captured) = TestClient::new_raw("phone");
    let session = paired(&server, &phone).await;

    let id = TransferId::from_bytes(&[0x22; 16]).expect("id");
    offer_bytes(&session, id, "abandoned.txt", b"abandoned").await;
    let request = offer_request(provider.next_event().await);
    assert_eq!(request.transfer_id, id.to_hex());

    // The window is closed, or the process died. Either way nobody answered.
    provider.disconnect().await;

    match captured.next_control(GRACE).await.body {
        Some(pb::file_control::Body::Cancel(c)) => {
            assert_eq!(c.reason(), pb::TransferFailureReason::DeclinedByUser);
        }
        other => panic!("expected a decline, got {other:?}"),
    }
    assert!(server.completed_files().is_empty());
    session.close().await;
}

// ---------------------------------------------------------------------------
// One decision, one offer
// ---------------------------------------------------------------------------

/// F. Two offers from one peer. Answering one must not answer the other, and
/// the untouched one must still be answerable on its own terms.
#[tokio::test]
async fn a_decision_for_one_offer_never_resolves_another() {
    let server = TestServer::start_with_approval_provider().await;
    let control = Control::start(Arc::clone(&server.state));
    let mut provider = Provider::attach(&control).await;
    let (phone, captured) = TestClient::new_raw("phone");
    let session = paired(&server, &phone).await;

    let first = TransferId::from_bytes(&[0x31; 16]).expect("id");
    let second = TransferId::from_bytes(&[0x32; 16]).expect("id");
    offer_bytes(&session, first, "first.txt", b"first").await;
    let first_prompt = offer_request(provider.next_event().await);
    offer_bytes(&session, second, "second.txt", b"second").await;
    let second_prompt = offer_request(provider.next_event().await);

    assert_eq!(first_prompt.transfer_id, first.to_hex());
    assert_eq!(second_prompt.transfer_id, second.to_hex());

    // Decline the first only.
    provider.decide(&first_prompt.transfer_id, false).await;
    match captured.next_control(GRACE).await.body {
        Some(pb::file_control::Body::Cancel(c)) => {
            assert_eq!(c.transfer_id, first.to_vec(), "the wrong offer was ended");
            assert_eq!(c.reason(), pb::TransferFailureReason::DeclinedByUser);
        }
        other => panic!("expected a decline for the first offer, got {other:?}"),
    }

    // The second is untouched and still waiting on a human.
    let snapshot = server.transfers.snapshot_one(second).await.expect("record");
    assert_eq!(snapshot.state, TransferState::WaitingAccept);

    // And it is still answerable, separately.
    provider.decide(&second_prompt.transfer_id, false).await;
    match captured.next_control(GRACE).await.body {
        Some(pb::file_control::Body::Cancel(c)) => {
            assert_eq!(c.transfer_id, second.to_vec());
        }
        other => panic!("expected a decline for the second offer, got {other:?}"),
    }
    session.close().await;
}

/// G. The same across two peers. A decision naming Alice's transfer must
/// leave Bob's alone, whatever order the prompts arrived in.
#[tokio::test]
async fn a_decision_for_one_peer_never_resolves_anothers_offer() {
    let server = TestServer::start_with_approval_provider().await;
    let control = Control::start(Arc::clone(&server.state));
    let mut provider = Provider::attach(&control).await;

    let (alice, alice_capture) = TestClient::new_raw("alice");
    let alice_session = paired(&server, &alice).await;
    let (bob, bob_capture) = TestClient::new_raw("bob");
    let bob_session = paired(&server, &bob).await;

    let from_alice = TransferId::from_bytes(&[0x41; 16]).expect("id");
    let from_bob = TransferId::from_bytes(&[0x42; 16]).expect("id");
    offer_bytes(&alice_session, from_alice, "alice.txt", b"alice").await;
    let alice_prompt = offer_request(provider.next_event().await);
    offer_bytes(&bob_session, from_bob, "bob.txt", b"bob").await;
    let bob_prompt = offer_request(provider.next_event().await);

    // The identity on each prompt is the authenticated one, not a name.
    assert_eq!(alice_prompt.fingerprint, alice.fingerprint.to_hex());
    assert_eq!(bob_prompt.fingerprint, bob.fingerprint.to_hex());
    assert_ne!(alice_prompt.fingerprint, bob_prompt.fingerprint);

    provider.decide(&alice_prompt.transfer_id, false).await;

    match alice_capture.next_control(GRACE).await.body {
        Some(pb::file_control::Body::Cancel(c)) => {
            assert_eq!(c.transfer_id, from_alice.to_vec());
        }
        other => panic!("expected alice's offer to be declined, got {other:?}"),
    }
    assert!(
        bob_capture
            .next_control_or_silence(Duration::from_millis(400))
            .await
            .is_none(),
        "a decision about alice must not reach bob's transfer"
    );
    let bobs = server
        .transfers
        .snapshot_one(from_bob)
        .await
        .expect("record");
    assert_eq!(bobs.state, TransferState::WaitingAccept);

    alice_session.close().await;
    bob_session.close().await;
}

/// J, K and L over the socket: a second decision, of either polarity, is
/// inert. The phone hears one verdict and only one.
#[tokio::test]
async fn a_repeated_or_reversed_decision_changes_nothing() {
    let server = TestServer::start_with_approval_provider().await;
    let control = Control::start(Arc::clone(&server.state));
    let mut provider = Provider::attach(&control).await;
    let (phone, captured) = TestClient::new_raw("phone");
    let session = paired(&server, &phone).await;

    let id = TransferId::from_bytes(&[0x51; 16]).expect("id");
    offer_bytes(&session, id, "once.txt", b"once").await;
    let request = offer_request(provider.next_event().await);

    provider.decide(&request.transfer_id, false).await;
    match captured.next_control(GRACE).await.body {
        Some(pb::file_control::Body::Cancel(c)) => {
            assert_eq!(c.reason(), pb::TransferFailureReason::DeclinedByUser);
        }
        other => panic!("expected a decline, got {other:?}"),
    }

    // A stale Accept, then duplicates of both answers.
    provider.decide(&request.transfer_id, true).await;
    provider.decide(&request.transfer_id, true).await;
    provider.decide(&request.transfer_id, false).await;

    assert!(
        captured
            .next_control_or_silence(Duration::from_millis(600))
            .await
            .is_none(),
        "a declined transfer must not be resurrected by a late Accept"
    );
    let snapshot = server.transfers.snapshot_one(id).await.expect("record");
    assert_eq!(snapshot.state, TransferState::Cancelled);
    assert!(server.completed_files().is_empty());
    assert!(server.partial_files().is_empty());
    session.close().await;
}

/// A client cannot answer a question it was never asked. The id here is real
/// and live — it is simply not one this provider has a prompt open for.
#[tokio::test]
async fn a_decision_for_an_offer_the_provider_was_never_shown_is_ignored() {
    let server = TestServer::start_with_approval_provider().await;
    let control = Control::start(Arc::clone(&server.state));
    let mut provider = Provider::attach(&control).await;
    let (phone, captured) = TestClient::new_raw("phone");
    let session = paired(&server, &phone).await;

    let id = TransferId::from_bytes(&[0x61; 16]).expect("id");
    offer_bytes(&session, id, "real.txt", b"real").await;
    let request = offer_request(provider.next_event().await);

    // A second provider replaces the first, inheriting no prompts.
    let mut newcomer = Provider::attach(&control).await;
    // The displaced session is told it was replaced, and its pending question
    // was declined on the way out.
    match captured.next_control(GRACE).await.body {
        Some(pb::file_control::Body::Cancel(c)) => {
            assert_eq!(c.reason(), pb::TransferFailureReason::DeclinedByUser);
        }
        other => panic!("expected the abandoned offer to be declined, got {other:?}"),
    }

    // The newcomer knows the id — it could have read it from `omnibridge
    // transfers` — and answering it does nothing at all.
    newcomer.decide(&request.transfer_id, true).await;
    assert!(captured
        .next_control_or_silence(Duration::from_millis(600))
        .await
        .is_none());
    assert!(server.completed_files().is_empty());

    let _ = provider
        .next_event_or_silence(Duration::from_millis(200))
        .await;
    session.close().await;
}

// ---------------------------------------------------------------------------
// The prompt outliving what it asks about
// ---------------------------------------------------------------------------

/// H. The phone goes away while the dialog is up. The prompt must be
/// withdrawn, and pressing Accept afterwards must not resurrect anything.
#[tokio::test]
async fn a_peer_that_disconnects_withdraws_the_prompt_and_a_late_accept_does_nothing() {
    let server = TestServer::start_with_approval_provider().await;
    let control = Control::start(Arc::clone(&server.state));
    let mut provider = Provider::attach(&control).await;
    let (phone, _captured) = TestClient::new_raw("phone");
    let session = paired(&server, &phone).await;

    let id = TransferId::from_bytes(&[0x71; 16]).expect("id");
    offer_bytes(&session, id, "vanishing.txt", b"vanishing").await;
    let request = offer_request(provider.next_event().await);

    // The phone drops off the network with the question still on screen.
    session.close().await;

    match provider.next_event().await {
        Event::FileOfferWithdrawn { transfer_id, .. } => {
            assert_eq!(transfer_id, request.transfer_id);
        }
        other => panic!("expected a withdrawal, got {other:?}"),
    }

    // The human presses Accept on a dialog that is already gone.
    provider.decide(&request.transfer_id, true).await;

    let snapshot = server.transfers.snapshot_one(id).await.expect("record");
    assert!(
        snapshot.state.is_terminal(),
        "the transfer should have ended with the session, not be waiting: {:?}",
        snapshot.state
    );
    assert_ne!(snapshot.state, TransferState::Completed);
    assert!(server.completed_files().is_empty());
    assert!(server.partial_files().is_empty());
}

/// I. Nobody answers. The offer expires on the daemon's own timeout, the
/// prompt is withdrawn, and a late Accept finds nothing.
#[tokio::test]
async fn an_unanswered_prompt_expires_and_cannot_be_accepted_afterwards() {
    let server = TestServer::start_with_approval_provider().await;
    let control = Control::start(Arc::clone(&server.state));
    let mut provider = Provider::attach(&control).await;
    let (phone, captured) = TestClient::new_raw("phone");
    let session = paired(&server, &phone).await;

    let id = TransferId::from_bytes(&[0x81; 16]).expect("id");
    offer_bytes(&session, id, "ignored.txt", b"ignored").await;
    let request = offer_request(provider.next_event().await);

    // The harness's accept timeout is seconds, not the production minutes.
    // The reaper is what ends it, so the peer hears "timed out" — not
    // "declined by the user", which would be a lie about a prompt nobody
    // touched.
    match captured.next_control(GRACE).await.body {
        Some(pb::file_control::Body::Failed(f)) => {
            assert_eq!(f.reason(), pb::TransferFailureReason::TimedOut);
        }
        other => panic!("expected a timeout, got {other:?}"),
    }

    match provider.next_event().await {
        Event::FileOfferWithdrawn { transfer_id, .. } => {
            assert_eq!(transfer_id, request.transfer_id);
        }
        other => panic!("expected a withdrawal, got {other:?}"),
    }

    provider.decide(&request.transfer_id, true).await;
    assert!(
        captured
            .next_control_or_silence(Duration::from_millis(600))
            .await
            .is_none(),
        "an expired offer must not come back"
    );
    let snapshot = server.transfers.snapshot_one(id).await.expect("record");
    assert_eq!(snapshot.state, TransferState::Failed);
    assert!(server.completed_files().is_empty());
    assert!(server.partial_files().is_empty());
    session.close().await;
}
