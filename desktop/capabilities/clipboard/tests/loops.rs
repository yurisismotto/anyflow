//! Loop and storm behaviour, with real managers on every node.
//!
//! The security suite proves the individual rules. This one wires two and
//! three managers to each other through their session channels and lets them
//! actually talk, because a sync loop is an emergent property: every rule can
//! be right and the system can still oscillate.
//!
//! # How a loop shows up here
//!
//! [`Net::settle`] delivers messages until a full sweep of every link
//! produces nothing new. A converging system runs dry in two or three sweeps.
//! A looping one never does — so `settle` carries a hard budget on *messages
//! delivered*, checked on every single delivery rather than per sweep. That
//! matters: a loop amplifies, so a per-sweep check would let one sweep grow
//! to billions of messages before it was ever tested. A failure here is a
//! count, not a hang.

mod common;

use std::collections::HashMap;

use common::*;
use omnibridge_capability_clipboard::{limits, ClipboardPolicy, CAPABILITY_ID};
use omnibridge_core::capability::OutboundMessage;
use omnibridge_core::Fingerprint;
use omnibridge_proto::v1::capabilities as pb;
use omnibridge_proto::Message;
use tokio::sync::mpsc::Receiver;

/// Fully automatic in both directions — the configuration a loop would need.
const WIDE_OPEN: ClipboardPolicy = ClipboardPolicy {
    allow_send: true,
    allow_receive: true,
    auto_send: true,
    auto_receive: true,
};

/// What a settled network carried.
#[derive(Debug, Default, PartialEq, Eq)]
struct Traffic {
    updates: usize,
    results: usize,
}

/// A little network of devices connected by control sessions.
struct Net {
    devices: Vec<Device>,
    fps: Vec<Fingerprint>,
    /// `sessions[(from, to)]` is what device `from` has queued for device
    /// `to` — the receiving end of the channel the manager was handed.
    sessions: HashMap<(usize, usize), Receiver<OutboundMessage>>,
}

impl Net {
    async fn new(names: &[&str]) -> Self {
        let mut devices = Vec::new();
        let mut fps = Vec::new();
        for (i, name) in names.iter().enumerate() {
            devices.push(Device::new(name).await);
            // Distinct, deterministic fingerprints.
            fps.push(fp(0x10 + i as u8));
        }
        Self {
            devices,
            fps,
            sessions: HashMap::new(),
        }
    }

    /// Connects two devices and grants each the given policy on the other.
    async fn link(&mut self, a: usize, b: usize, policy: ClipboardPolicy) {
        self.devices[a].authorizer.grant(self.fps[b], policy).await;
        self.devices[b].authorizer.grant(self.fps[a], policy).await;
        let a_to_b = self.devices[a].connect(self.fps[b]).await;
        let b_to_a = self.devices[b].connect(self.fps[a]).await;
        self.sessions.insert((a, b), a_to_b);
        self.sessions.insert((b, a), b_to_a);
    }

    /// A human copies text on device `i`, and its watcher fires.
    async fn copy_on(&mut self, i: usize, text: &str) {
        self.devices[i].backend.user_copies(text);
        self.devices[i].manager.on_local_change().await;
    }

    /// Delivers messages until nothing is left anywhere.
    ///
    /// `budget` is the total number of deliveries allowed. It is the loop
    /// detector: exceeding it means the network is amplifying rather than
    /// converging, and the assertion names the count so the failure is
    /// diagnosable.
    async fn settle(&mut self, budget: usize) -> Traffic {
        let mut traffic = Traffic::default();
        let mut delivered = 0usize;
        let links: Vec<(usize, usize)> = self.sessions.keys().copied().collect();

        loop {
            let mut moved = false;

            for &(from, to) in &links {
                let batch = {
                    let Some(rx) = self.sessions.get_mut(&(from, to)) else {
                        continue;
                    };
                    drain(rx)
                };

                for message in batch {
                    moved = true;
                    delivered += 1;
                    assert!(
                        delivered <= budget,
                        "the network delivered more than {budget} messages without \
                         settling: this is a sync loop"
                    );

                    assert_eq!(message.capability_id, CAPABILITY_ID);
                    match pb::ClipboardControl::decode(message.payload.as_slice())
                        .expect("decodable")
                        .body
                    {
                        Some(pb::clipboard_control::Body::Update(_)) => traffic.updates += 1,
                        Some(pb::clipboard_control::Body::Result(_)) => traffic.results += 1,
                        None => {}
                    }

                    let sender_fp = self.fps[from];
                    let sender_id = self.devices[from].device_id.clone();

                    let reply = self.devices[to]
                        .manager
                        .handle_control(sender_fp, &sender_id, &message.payload)
                        .await
                        .expect("a well-formed payload");

                    // A real clipboard cannot tell a remote write from a local
                    // copy, so the receiving device's watcher fires either
                    // way. This is the step a loop would ride.
                    self.devices[to].manager.on_local_change().await;

                    // The capability puts the reply on the session it arrived
                    // on; route it back the same way.
                    if let Some(reply) = reply {
                        self.deliver_reply(to, from, reply, &mut traffic, &mut delivered, budget)
                            .await;
                    }
                }
            }

            if !moved {
                return traffic;
            }
        }
    }

    /// Hands one result straight to its recipient. Results never generate
    /// further traffic — answering an answer is how two correct peers build a
    /// loop out of nothing — so this does not recurse.
    async fn deliver_reply(
        &mut self,
        from: usize,
        to: usize,
        reply: OutboundMessage,
        traffic: &mut Traffic,
        delivered: &mut usize,
        budget: usize,
    ) {
        *delivered += 1;
        assert!(*delivered <= budget, "a result storm");
        traffic.results += 1;

        let sender_fp = self.fps[from];
        let sender_id = self.devices[from].device_id.clone();
        let follow_up = self.devices[to]
            .manager
            .handle_control(sender_fp, &sender_id, &reply.payload)
            .await
            .expect("a well-formed result");
        assert!(
            follow_up.is_none(),
            "a ClipboardResult must never be answered: that is a message loop"
        );
    }

    fn clipboard(&self, i: usize) -> Option<String> {
        self.devices[i].backend.current()
    }
}

// ---------------------------------------------------------------------------
// Two peers
// ---------------------------------------------------------------------------

/// ```text
///   desktop ⇄ phone,   auto-send and auto-receive on at BOTH ends
/// ```
#[tokio::test]
async fn a_clip_between_two_fully_automatic_peers_settles_after_one_round_trip() {
    const DESKTOP: usize = 0;
    const PHONE: usize = 1;

    let mut net = Net::new(&["desktop", "phone"]).await;
    net.link(DESKTOP, PHONE, WIDE_OPEN).await;

    net.copy_on(DESKTOP, "hello from Fedora").await;

    // Generous: a correct run needs 2. Anything near it is already a bug.
    let traffic = net.settle(32).await;

    assert_eq!(
        traffic,
        Traffic {
            updates: 1,
            results: 1
        },
        "one copy must produce exactly one update and one result"
    );
    assert_eq!(net.clipboard(PHONE).as_deref(), Some("hello from Fedora"));
    assert_eq!(net.clipboard(DESKTOP).as_deref(), Some("hello from Fedora"));
}

/// The storm test: hundreds of events, both ends automatic, and a hard bound
/// on total traffic.
#[tokio::test]
async fn two_hundred_copies_produce_two_hundred_updates_and_no_storm() {
    const COPIES: usize = 200;
    const DESKTOP: usize = 0;
    const PHONE: usize = 1;

    let mut net = Net::new(&["desktop", "phone"]).await;
    net.link(DESKTOP, PHONE, WIDE_OPEN).await;

    let mut total = Traffic::default();
    for i in 0..COPIES {
        net.copy_on(DESKTOP, &format!("clip {i}")).await;
        let traffic = net.settle(32).await;
        assert_eq!(
            traffic,
            Traffic {
                updates: 1,
                results: 1
            },
            "copy {i} did not settle to one update"
        );
        total.updates += traffic.updates;
        total.results += traffic.results;
    }

    assert_eq!(
        total.updates, COPIES,
        "one update per copy, no amplification"
    );
    assert_eq!(total.results, COPIES);
    assert_eq!(
        net.clipboard(PHONE).as_deref(),
        Some(format!("clip {}", COPIES - 1).as_str())
    );

    // The caches did not grow with the traffic.
    for i in [DESKTOP, PHONE] {
        let (events, suppression) = net.devices[i].manager.cache_sizes().await;
        assert!(
            events <= limits::EVENT_CACHE_ENTRIES,
            "device {i} event cache grew to {events}"
        );
        assert!(
            suppression <= limits::SUPPRESSION_ENTRIES,
            "device {i} suppression cache grew to {suppression}"
        );
    }
}

/// Repeatedly copying the *same* text is not a loop and must not be
/// suppressed as one.
#[tokio::test]
async fn fifty_copies_of_identical_text_are_all_delivered() {
    const COPIES: usize = 50;
    const DESKTOP: usize = 0;
    const PHONE: usize = 1;

    let mut net = Net::new(&["desktop", "phone"]).await;
    net.link(DESKTOP, PHONE, WIDE_OPEN).await;

    for i in 0..COPIES {
        net.copy_on(DESKTOP, "the same text every time").await;
        let traffic = net.settle(32).await;
        assert_eq!(
            traffic,
            Traffic {
                updates: 1,
                results: 1
            },
            "copy {i} of identical text should still be delivered exactly once"
        );
    }

    assert_eq!(
        net.clipboard(PHONE).as_deref(),
        Some("the same text every time")
    );
}

/// A copy on each side in turn: the direction alternates and neither end
/// echoes what it was given.
#[tokio::test]
async fn alternating_copies_on_both_sides_never_amplify() {
    const DESKTOP: usize = 0;
    const PHONE: usize = 1;

    let mut net = Net::new(&["desktop", "phone"]).await;
    net.link(DESKTOP, PHONE, WIDE_OPEN).await;

    for i in 0..25 {
        let (source, text) = if i % 2 == 0 {
            (DESKTOP, format!("from desktop {i}"))
        } else {
            (PHONE, format!("from phone {i}"))
        };
        net.copy_on(source, &text).await;

        let traffic = net.settle(32).await;
        assert_eq!(
            traffic,
            Traffic {
                updates: 1,
                results: 1
            },
            "round {i} amplified"
        );
        assert_eq!(net.clipboard(DESKTOP).as_deref(), Some(text.as_str()));
        assert_eq!(net.clipboard(PHONE).as_deref(), Some(text.as_str()));
    }
}

// ---------------------------------------------------------------------------
// Three peers
// ---------------------------------------------------------------------------

/// ```text
///   phone A ──► desktop ──X──► tablet B
/// ```
///
/// The desktop is fully automatic with both devices. A clip from A must reach
/// the desktop's clipboard and stop there.
#[tokio::test]
async fn a_desktop_between_two_phones_is_not_a_relay() {
    const DESKTOP: usize = 0;
    const PHONE_A: usize = 1;
    const TABLET_B: usize = 2;

    let mut net = Net::new(&["desktop", "phone-a", "tablet-b"]).await;
    net.link(DESKTOP, PHONE_A, WIDE_OPEN).await;
    net.link(DESKTOP, TABLET_B, WIDE_OPEN).await;

    net.copy_on(PHONE_A, "a secret from phone A").await;
    let traffic = net.settle(64).await;

    assert_eq!(
        traffic,
        Traffic {
            updates: 1,
            results: 1
        },
        "phone A's clip must travel exactly one hop"
    );
    assert_eq!(
        net.clipboard(DESKTOP).as_deref(),
        Some("a secret from phone A"),
        "the desktop applies it locally"
    );
    assert_eq!(
        net.clipboard(TABLET_B),
        None,
        "tablet B must never see a clip that came from phone A"
    );
}

/// The same topology, but the copy starts on the desktop: it *should* reach
/// both devices, and neither should echo it back.
#[tokio::test]
async fn a_local_copy_reaches_every_peer_exactly_once() {
    const DESKTOP: usize = 0;
    const PHONE_A: usize = 1;
    const TABLET_B: usize = 2;

    let mut net = Net::new(&["desktop", "phone-a", "tablet-b"]).await;
    net.link(DESKTOP, PHONE_A, WIDE_OPEN).await;
    net.link(DESKTOP, TABLET_B, WIDE_OPEN).await;

    net.copy_on(DESKTOP, "broadcast").await;
    let traffic = net.settle(64).await;

    assert_eq!(
        traffic,
        Traffic {
            updates: 2,
            results: 2
        },
        "one update per peer; a third would be an echo"
    );
    assert_eq!(net.clipboard(PHONE_A).as_deref(), Some("broadcast"));
    assert_eq!(net.clipboard(TABLET_B).as_deref(), Some("broadcast"));
}

/// A hundred copies across a three-device mesh. This is the shape most likely
/// to produce a storm, so it gets the largest run and the tightest bound.
#[tokio::test]
async fn a_three_device_mesh_stays_quiet_over_a_hundred_copies() {
    const COPIES: usize = 100;
    const DESKTOP: usize = 0;
    const PHONE_A: usize = 1;
    const TABLET_B: usize = 2;

    let mut net = Net::new(&["desktop", "phone-a", "tablet-b"]).await;
    net.link(DESKTOP, PHONE_A, WIDE_OPEN).await;
    net.link(DESKTOP, TABLET_B, WIDE_OPEN).await;

    let mut total = Traffic::default();
    for i in 0..COPIES {
        // Rotate which device the human copies on.
        let source = match i % 3 {
            0 => DESKTOP,
            1 => PHONE_A,
            _ => TABLET_B,
        };
        net.copy_on(source, &format!("mesh clip {i}")).await;

        let traffic = net.settle(64).await;
        let expected = if source == DESKTOP {
            // The desktop is linked to both, so a local copy fans out twice.
            Traffic {
                updates: 2,
                results: 2,
            }
        } else {
            // A leaf reaches only the desktop, and the desktop must not relay.
            Traffic {
                updates: 1,
                results: 1,
            }
        };
        assert_eq!(traffic, expected, "copy {i} from device {source} amplified");
        total.updates += traffic.updates;
        total.results += traffic.results;
    }

    // 100 copies: 34 from the desktop (2 updates each) + 66 from leaves (1).
    assert_eq!(total.updates, 34 * 2 + 66);

    for i in 0..3 {
        let (events, suppression) = net.devices[i].manager.cache_sizes().await;
        assert!(events <= limits::EVENT_CACHE_ENTRIES);
        assert!(suppression <= limits::SUPPRESSION_ENTRIES);
    }
}

/// A device with `auto_receive` on but `auto_send` off is a sink: it applies
/// what it is given and never answers with an update.
#[tokio::test]
async fn a_receive_only_peer_never_echoes() {
    const DESKTOP: usize = 0;
    const PHONE: usize = 1;

    let mut net = Net::new(&["desktop", "phone"]).await;
    // Linked symmetrically first, then the phone's own view is narrowed.
    net.link(DESKTOP, PHONE, WIDE_OPEN).await;
    net.devices[PHONE]
        .authorizer
        .set_policy(
            net.fps[DESKTOP],
            ClipboardPolicy {
                allow_send: true,
                allow_receive: true,
                auto_send: false,
                auto_receive: true,
            },
        )
        .await;

    net.copy_on(DESKTOP, "one way").await;
    let traffic = net.settle(32).await;

    assert_eq!(
        traffic,
        Traffic {
            updates: 1,
            results: 1
        }
    );
    assert_eq!(net.clipboard(PHONE).as_deref(), Some("one way"));

    // And a copy on the phone goes nowhere, because auto-send is off there.
    net.copy_on(PHONE, "a phone-local copy").await;
    assert_eq!(net.settle(32).await, Traffic::default());
    assert_eq!(net.clipboard(DESKTOP).as_deref(), Some("one way"));
}
