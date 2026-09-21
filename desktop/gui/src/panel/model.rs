//! What the Quick Panel shows, and what it will do — with no GTK in sight.
//!
//! Every decision the panel makes is taken here, on plain data, and the
//! widget layer in [`super`] does nothing but draw the result and hand back
//! clicks. That split is the point of the module:
//!
//! * the rules that matter — which device an action targets, whether an
//!   action is allowed at all, what a battery reading means — are testable
//!   without a display, and are tested below;
//! * the widget layer cannot quietly grow a rule of its own, because it is
//!   handed a [`Action::Ready`] with a fingerprint in it or a
//!   [`Action::Blocked`] with a sentence in it, and has nothing else to go on.
//!
//! # The one invariant everything here serves
//!
//! **A destination is a fingerprint.** Never a display name, never a list
//! index, never an address. U2 P1 found all three on Android — three call
//! sites reading `peers().firstOrNull()` — and recorded that the trust
//! store's order is not even stable, because a successful connection moves a
//! peer to the end of the list. [`Target`] is the desktop half of that fix
//! and has the same four outcomes as `store/PeerTarget.kt`.
//!
//! # This is a view, not an authority
//!
//! Nothing here grants anything. [`Action::Ready`] means "the daemon last
//! said this was possible", and the daemon re-checks every grant when the
//! request actually arrives — `FilesAuthorizer::is_authorized` reads the
//! trust store fresh, including against a stream that is already mid-copy. A
//! capability revoked between the draw and the click is refused *there*,
//! which is why this layer is allowed to work from a two-second-old poll.

use omnibridge_control::{
    transfer_direction, transfer_failure, transfer_state, ClipboardPeerReport,
    ClipboardStatusReport, DeviceReport, DeviceState, NotificationPeerReport,
    NotificationsStatusReport, Request, StatusReport, TransferReport,
};

use crate::DaemonState;

pub const FILES: &str = "files.v1";
pub const CLIPBOARD: &str = "clipboard.v1";
pub const NOTIFICATIONS: &str = "notifications.v1";
pub const BATTERY: &str = "battery.v1";

// ---------------------------------------------------------------------------
// Daemon health
// ---------------------------------------------------------------------------

/// Whether there is an OmniBridge service to talk to at all.
///
/// Three states rather than two: "we have not heard back yet" is the first
/// second of every panel and is not a failure, and drawing it as one would
/// make the panel flash an error every time it opened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Health {
    Available,
    /// The first poll is still in flight.
    Reaching,
    /// The socket could not be reached, or the daemon answered with an error.
    Unavailable {
        /// Calm and actionable. The raw error is kept out of the panel and
        /// stays in Settings and the log, per the error-state rule.
        headline: String,
    },
}

impl Health {
    pub fn is_available(&self) -> bool {
        matches!(self, Health::Available)
    }
}

// ---------------------------------------------------------------------------
// Peers
// ---------------------------------------------------------------------------

/// How a device stands right now, as the panel says it out loud.
///
/// `Stale` stays distinct from `Connected` for the reason `DeviceState` was
/// introduced: a session that has stopped answering its liveness probes is
/// exactly the case where everything the device last told us is history.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Link {
    Connected,
    Stale,
    Offline,
}

impl Link {
    /// The word. Never a colour, and never only an icon.
    pub fn label(self) -> &'static str {
        match self {
            Link::Connected => "Connected",
            Link::Stale => "Not responding",
            Link::Offline => "Offline",
        }
    }

    /// Whether an action that needs a live session can run.
    pub fn is_live(self) -> bool {
        matches!(self, Link::Connected)
    }
}

/// A peer's battery, as this desktop is entitled to describe it.
///
/// Three states, and the middle one is the whole reason this type exists.
/// U2 P2 fixed a desktop that rendered *no battery* as `0%`: `battery.v1`
/// expresses "there is nothing truthful to say" by sending no frame at all
/// (`LocalBatterySource::read` returning `None`), so a missing reading has to
/// stay missing all the way to the screen.
///
/// `Absent` and `Unavailable` are kept apart because they are answers to
/// different questions and have different fixes:
///
/// * **Absent** — the session is live and `battery.v1` is negotiated on it,
///   and the device has still reported nothing. Rendered as *no battery
///   reported*, never as a number.
/// * **Unavailable** — we are not in a position to know: the device is
///   offline, or this session never negotiated `battery.v1`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Battery {
    Present {
        /// 0..=100. **Zero is a real reading** and renders as `0%`.
        percent: u8,
        /// `Charging`, `Full`, `Not charging`, or empty when the device did
        /// not say.
        status: String,
        /// The reading is old enough to describe the past.
        stale: bool,
    },
    Absent,
    Unavailable,
}

impl Battery {
    /// The short form that sits on the device row.
    pub fn label(&self) -> String {
        match self {
            Battery::Present {
                percent,
                status,
                stale,
            } => {
                let mut s = format!("{percent}%");
                if *stale {
                    // Labelled rather than hidden: an old reading is still
                    // information, and pretending it is current is the lie.
                    s.push_str(" (last known)");
                } else if !status.is_empty() {
                    s.push_str(" · ");
                    s.push_str(status);
                }
                s
            }
            Battery::Absent => "No battery reported".to_string(),
            Battery::Unavailable => "Battery unavailable".to_string(),
        }
    }

    /// The spoken form, which never leaves a percentage as a bare number.
    pub fn announcement(&self) -> String {
        match self {
            Battery::Present { .. } => format!("battery {}", self.label()),
            _ => self.label().to_lowercase(),
        }
    }
}

/// Whether a capability is granted, and whether it is live on this session.
///
/// Both, separately, because they fail for unrelated reasons: a grant that
/// exists on a device with no session cannot move a byte, and a session that
/// negotiated nothing cannot either. Only [`Capability::usable`] is allowed
/// to enable an action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capability {
    /// The trust store says this device may do this.
    pub granted: bool,
    /// The live session negotiated it.
    pub live: bool,
}

impl Capability {
    pub const NONE: Capability = Capability {
        granted: false,
        live: false,
    };

    pub fn usable(self) -> bool {
        self.granted && self.live
    }
}

/// One device, as the panel draws it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerCard {
    /// Full hex. **The routing key**, and the only thing an action carries.
    pub fingerprint: String,
    /// The grouped short form, for the secondary details affordance.
    pub fingerprint_short: String,
    pub device_id: String,
    pub name: String,
    pub platform: String,
    pub link: Link,
    /// Communicated with an icon and a word, never with colour alone.
    pub selected: bool,
    pub battery: Battery,
    pub files: Capability,
    pub clipboard: Capability,
    pub notifications: Capability,
}

impl PeerCard {
    /// Whether this device looks like a phone or tablet.
    pub fn is_mobile(&self) -> bool {
        let p = self.platform.to_ascii_lowercase();
        p.contains("android") || p.contains("ios")
    }

    /// The capabilities that are usable right now, in a fixed order.
    ///
    /// Presence carries the meaning, so the line is legible without colour
    /// and reads the same to a screen reader as it looks.
    pub fn available_capabilities(&self) -> Vec<&'static str> {
        let mut out = Vec::new();
        if self.clipboard.usable() {
            out.push("Clipboard");
        }
        if self.files.usable() {
            out.push("Files");
        }
        if self.notifications.usable() {
            out.push("Notifications");
        }
        out
    }

    /// One sentence for assistive technology, covering everything the row
    /// says visually — name, selected, link, battery, capabilities.
    pub fn announcement(&self) -> String {
        let mut parts = vec![self.name.clone()];
        if self.selected {
            parts.push("selected device".into());
        }
        parts.push(self.link.label().to_lowercase());
        if !matches!(self.battery, Battery::Unavailable) {
            parts.push(self.battery.announcement());
        }
        let caps = self.available_capabilities();
        parts.push(if caps.is_empty() {
            "no capabilities available".into()
        } else {
            format!("available: {}", caps.join(", "))
        });
        parts.join(", ")
    }
}

// ---------------------------------------------------------------------------
// Target resolution
// ---------------------------------------------------------------------------

/// Which device an action would go to.
///
/// The desktop half of `store/PeerTarget.kt`, with the same four outcomes and
/// the same rule: a destination is a function of the *set* of trusted peers
/// and the person's *choice*, and of nothing else. No branch here reads a
/// list index or a display name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// An explicit, still-valid choice.
    Selected(String),
    /// Exactly one trusted peer and no choice made. The convenience case, and
    /// safe precisely because there is nothing to be ambiguous *about*.
    OnlyTrustedPeer(String),
    /// The person has to say. Never resolves to anything on its own.
    MustChoose {
        /// A choice was stored and no longer names a trusted peer.
        ///
        /// Android's rule 7 lets a stale choice fall through to the
        /// one-peer convenience. This deliberately does not: a stored
        /// fingerprint that has gone is a person whose device was unpaired
        /// or re-paired, and quietly re-aiming their Send button at whatever
        /// is left is the "fallback to another peer" this whole model
        /// exists to refuse. It costs one click and the panel says why.
        stale_choice: bool,
    },
    NoTrustedPeer,
}

impl Target {
    /// The destination, when there is one. `None` is not an error state — it
    /// is the panel declining to guess.
    pub fn fingerprint(&self) -> Option<&str> {
        match self {
            Target::Selected(fp) | Target::OnlyTrustedPeer(fp) => Some(fp),
            Target::MustChoose { .. } | Target::NoTrustedPeer => None,
        }
    }

    /// Resolves a destination from the trusted set and the stored choice.
    ///
    /// `peers` is the set of peers the panel shows; revoked devices are not
    /// in it. Order is irrelevant by construction — the only thing read out
    /// of the slice is a fingerprint comparison and a length.
    pub fn resolve(peers: &[PeerCard], chosen: Option<&str>) -> Target {
        if peers.is_empty() {
            return Target::NoTrustedPeer;
        }
        if let Some(chosen) = chosen {
            let chosen = chosen.trim().to_ascii_lowercase();
            if let Some(peer) = peers
                .iter()
                .find(|p| p.fingerprint.eq_ignore_ascii_case(&chosen))
            {
                return Target::Selected(peer.fingerprint.clone());
            }
            return Target::MustChoose {
                stale_choice: !chosen.is_empty(),
            };
        }
        match peers {
            [only] => Target::OnlyTrustedPeer(only.fingerprint.clone()),
            _ => Target::MustChoose {
                stale_choice: false,
            },
        }
    }
}

// ---------------------------------------------------------------------------
// Actions
// ---------------------------------------------------------------------------

/// Whether a quick action can run, and against what.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Ready {
        /// Full fingerprint hex. Handed to the daemon verbatim.
        fingerprint: String,
        /// For the caption under the button — display only, never routing.
        peer_name: String,
    },
    /// Disabled, with the reason said out loud. The same sentence is the
    /// visible caption and the accessible description, so a disabled control
    /// explains itself either way it is encountered.
    Blocked { reason: String },
}

impl Action {
    fn blocked(reason: impl Into<String>) -> Action {
        Action::Blocked {
            reason: reason.into(),
        }
    }

    pub fn is_ready(&self) -> bool {
        matches!(self, Action::Ready { .. })
    }

    /// The device selector this action would send. `None` when blocked.
    pub fn target(&self) -> Option<&str> {
        match self {
            Action::Ready { fingerprint, .. } => Some(fingerprint),
            Action::Blocked { .. } => None,
        }
    }

    pub fn reason(&self) -> Option<&str> {
        match self {
            Action::Blocked { reason } => Some(reason),
            Action::Ready { .. } => None,
        }
    }
}

/// The control request a Send File press makes, or `None` when blocked.
///
/// Here rather than in the widget so that "the button sends to the selected
/// fingerprint" is a property a test can assert on the request itself. The
/// widget calls this and sends what it returns; it composes no request of its
/// own.
pub fn send_file_request(action: &Action, path: String) -> Option<Request> {
    Some(Request::Send {
        device: action.target()?.to_string(),
        path,
    })
}

/// The control request a Send Clipboard press makes, or `None` when blocked.
///
/// `sensitive` is false and is not the panel's to set: asking the receiver to
/// treat a clip as a secret is a claim about the clip, and the person pressing
/// a general-purpose Send button has not made it. The Settings clipboard page
/// takes the same position.
pub fn send_clipboard_request(action: &Action) -> Option<Request> {
    Some(Request::ClipboardSend {
        device: action.target()?.to_string(),
        sensitive: false,
    })
}

// ---------------------------------------------------------------------------
// Clipboard feedback — QP-DEBT-06
// ---------------------------------------------------------------------------

/// What the panel says the instant a Send clipboard press is accepted.
///
/// # The defect this corrects
///
/// The press used to be acknowledged with **"Clipboard sent to SM-X620"**, and
/// that was optimistic by one round trip. `ClipboardSend` is answered by the
/// daemon as soon as the frame is on the session — which is all the sending
/// end can know at that moment — while the receiver's verdict arrives
/// afterwards and lands in the status row. Found on hardware: the panel said
/// "sent" and the tablet had refused the clip, because the grant on the
/// *Android* side was missing.
///
/// So the immediate message says what actually happened, and names the thing
/// that will answer the question. It is not the state authority — the status
/// row is, and the panel's own poll refreshes it within a couple of seconds —
/// which is why this is one sentence and not a second outcome model.
pub fn clipboard_submitted_message(peer: &str) -> String {
    format!("Clipboard submitted to {peer}; awaiting confirmation")
}

/// What the panel says when the daemon refuses the press outright.
///
/// This one *is* final: nothing left this computer, so there is no verdict
/// coming and no ambiguity to preserve. The daemon's message is written for a
/// person and carries no clip content — `do_clipboard_send` composes it from a
/// fingerprint and a byte count.
pub fn clipboard_send_error_message(message: &str) -> String {
    format!("Could not send the clipboard: {message}")
}

/// Whether a peer's reported outcome means the clip actually arrived.
///
/// `duplicate` counts: the peer recognised the event id, which it could only
/// have got from us, so an earlier copy of that clip reached it. The Android
/// side sorts the same outcomes the same way, in `ClipboardDelivery::of`.
pub fn clipboard_outcome_succeeded(outcome: &str) -> bool {
    matches!(outcome, "applied" | "pending" | "duplicate")
}

/// One peer verdict, in words, for the clipboard status row.
///
/// The row is the authoritative final outcome, so it says what happened when
/// the clip arrived as well as when it did not — a person who has just been
/// told "awaiting confirmation" needs somewhere for that to resolve, and an
/// empty row is not an answer.
///
/// The vocabulary is closed and none of it is a protocol string: the daemon's
/// `Outcome::as_str` values are an interface between two of our own processes,
/// not English, and `Last clip sent: not authorized by SM-X620.` is what
/// putting them on screen reads like.
pub fn clipboard_outcome_note(outcome: &str, peer: &str) -> String {
    match outcome {
        "applied" => format!(" The last clip reached {peer}."),
        "pending" => format!(" The last clip reached {peer} and is waiting to be applied there."),
        "duplicate" => format!(" {peer} already had the last clip."),
        "not authorized" => {
            format!(" {peer} is not set up to accept this computer's clipboard.")
        }
        "rejected by policy" => {
            format!(" {peer} is not accepting clipboard text from this computer.")
        }
        "rejected as sensitive" => format!(" {peer} refuses clipboard text marked sensitive."),
        "too large" => format!(" The last clip was too large for {peer}."),
        "invalid text" => format!(" {peer} could not read the last clip as text."),
        // Anything a newer daemon reports. Named as a refusal rather than
        // echoed, so an unknown value can never read as success.
        _ => format!(" {peer} could not use the last clip."),
    }
}

// ---------------------------------------------------------------------------
// Status lines
// ---------------------------------------------------------------------------

/// A one-word state for a capability row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusValue {
    On,
    Off,
    Unavailable,
}

impl StatusValue {
    pub fn label(self) -> &'static str {
        match self {
            StatusValue::On => "On",
            StatusValue::Off => "Off",
            StatusValue::Unavailable => "Unavailable",
        }
    }
}

/// One row of the panel's status block: a name, a word, and a sentence.
///
/// `detail` is the accessible description and the tooltip. It carries the
/// truthful qualification — which direction is automatic, which is manual —
/// that a one-word value cannot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusLine {
    pub value: StatusValue,
    pub detail: String,
}

impl StatusLine {
    fn new(value: StatusValue, detail: impl Into<String>) -> StatusLine {
        StatusLine {
            value,
            detail: detail.into(),
        }
    }
}

/// A transfer that is moving *right now*.
///
/// The panel's whole answer to "did my file go?", and deliberately the
/// smallest one that answers it. Only in-flight transfers are eligible: a
/// finished transfer disappears from the panel rather than accumulating, so
/// this cannot become the file history the design forbids. The completed list
/// stays in Settings, where it is scoped to the daemon run and says so.
///
/// The filename is shown because it is an *active transfer surface*, which is
/// where the files policy already allows one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferLine {
    /// Sanitised by the daemon long before it reaches here.
    pub filename: String,
    pub outgoing: bool,
    pub peer_name: String,
    /// `None` for a zero-byte file, where a percentage means nothing.
    pub percent: Option<u8>,
}

impl TransferLine {
    pub fn label(&self) -> String {
        let verb = if self.outgoing {
            "Sending"
        } else {
            "Receiving"
        };
        match self.percent {
            Some(p) => format!("{verb} {} · {p}%", self.filename),
            None => format!("{verb} {}", self.filename),
        }
    }
}

/// Which way a transfer is going, from the transfer itself.
///
/// The *only* admissible source of a direction. Not the filename, not which
/// button was pressed, not which window it was pressed in — the daemon says
/// which end of the copy this machine is, and nothing else here gets a vote.
///
/// `None` for a value this build does not recognise, and a row with no known
/// direction is not shown: every line in the recent list reads "to" or
/// "from" somebody, so a guess would not be a degraded label but a false
/// statement about where a file went.
fn outgoing(direction: &str) -> Option<bool> {
    match direction {
        transfer_direction::SENDING => Some(true),
        transfer_direction::RECEIVING => Some(false),
        _ => None,
    }
}

/// The one in-flight transfer worth a line, preferring the target device's.
fn active_transfer(state: &DaemonState, peer: Option<&PeerCard>) -> Option<TransferLine> {
    let transfers = state.transfers.as_deref()?;
    let live: Vec<&TransferReport> = transfers
        .iter()
        .filter(|t| !transfer_state::is_terminal(&t.state))
        .collect();
    let chosen = live
        .iter()
        .copied()
        .find(|t| peer.is_some_and(|p| p.fingerprint_short == t.fingerprint_short))
        .or_else(|| live.first().copied())?;
    Some(TransferLine {
        filename: chosen.filename.clone(),
        outgoing: outgoing(&chosen.direction).unwrap_or(false),
        peer_name: chosen.device_name.clone(),
        percent: chosen.percentage,
    })
}

// ---------------------------------------------------------------------------
// Recent transfers
// ---------------------------------------------------------------------------

/// How many finished transfers the panel will name.
///
/// Three. The panel answers "where did that file just go" and then stops; a
/// fourth row would be the beginning of a history, which is what Settings is
/// for and what this surface is not.
pub const RECENT_LIMIT: usize = 3;

/// How a transfer ended, in the panel's own words.
///
/// Derived from the daemon's terminal state and its failure *token* — never
/// from its failure prose, which is copy that must stay free to reword. The
/// daemon has three terminal states (`completed`, `failed`, `cancelled`) and
/// twelve failure reasons; this is the small set of things a person actually
/// wants told, and every arm below is reachable from a real reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// `completed`, outgoing.
    Sent,
    /// `completed`, incoming.
    Received,
    /// Somebody said no: `declined_by_user`.
    Declined,
    /// Somebody pressed cancel: `cancelled_by_user`, or a bare `cancelled`.
    Cancelled,
    /// `timed_out` — nobody answered in time.
    TimedOut,
    /// `transport` — the connection ended mid-copy.
    Disconnected,
    /// Every other reason. The detail stays in Settings, which shows the
    /// daemon's own sentence; a panel row is not the place for it.
    Failed,
}

impl Outcome {
    /// The word on the row. Calm, and never a raw error.
    pub fn label(self) -> &'static str {
        match self {
            Outcome::Sent => "Sent",
            Outcome::Received => "Received",
            Outcome::Declined => "Declined",
            Outcome::Cancelled => "Cancelled",
            Outcome::TimedOut => "Timed out",
            Outcome::Disconnected => "Disconnected",
            Outcome::Failed => "Failed",
        }
    }

    /// Whether this is the happy ending, for the row's status vocabulary.
    pub fn succeeded(self) -> bool {
        matches!(self, Outcome::Sent | Outcome::Received)
    }

    /// Reads a terminal transfer. `None` while it is still moving.
    fn read(state: &str, failure_code: Option<&str>, outgoing: bool) -> Option<Outcome> {
        if !transfer_state::is_terminal(state) {
            return None;
        }
        // The token, not the sentence. `failure` is prose for a person and
        // rewording it is a copy change; branching on it would make that
        // reword a silent behaviour change here.
        Some(match failure_code {
            Some(transfer_failure::DECLINED_BY_USER) => Outcome::Declined,
            Some(transfer_failure::CANCELLED_BY_USER) => Outcome::Cancelled,
            Some(transfer_failure::TIMED_OUT) => Outcome::TimedOut,
            Some(transfer_failure::TRANSPORT) => Outcome::Disconnected,
            // A reason this build does not know reads as a plain failure
            // rather than as success — the safe direction for a newer agent.
            Some(_) => Outcome::Failed,
            None => match state {
                transfer_state::COMPLETED if outgoing => Outcome::Sent,
                transfer_state::COMPLETED => Outcome::Received,
                // Terminal, unsuccessful, and the daemon named no reason.
                transfer_state::CANCELLED => Outcome::Cancelled,
                _ => Outcome::Failed,
            },
        })
    }
}

/// One finished transfer, as the panel names it.
///
/// # What this is not
///
/// Not history. These are read out of the daemon's in-memory list for the
/// current run and nothing here is written anywhere: restarting `omnibridged`
/// empties it, which is the intended behaviour and not a defect.
///
/// # What it deliberately cannot carry
///
/// No file contents, no hash, no transfer id, no full fingerprint, no stored
/// path. A filename and a peer name, which is what "find that file again"
/// needs and is the same metadata an active transfer surface already shows.
///
/// # Identity
///
/// `peer_name` and `peer_fingerprint_short` come from **this transfer**, not
/// from the device currently selected for sending. A finished transfer
/// happened with whoever it happened with, and re-labelling it with the
/// current choice would be the multi-peer defect wearing a different hat.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecentTransfer {
    /// Sanitised by the daemon long before it reaches here.
    pub filename: String,
    /// The *trust store's* name for the peer this transfer was with — never
    /// a name the peer asserted, and never the selected device's name.
    pub peer_name: String,
    /// The short form only, and only in the accessible description and the
    /// tooltip, where it disambiguates two devices with the same display
    /// name. Never the full fingerprint, and never on the visible row.
    pub peer_fingerprint_short: String,
    pub outgoing: bool,
    pub outcome: Outcome,
    /// The daemon's creation counter: higher is newer. Ordering only — it is
    /// never shown and never routed on.
    pub sort_key: u64,
}

impl RecentTransfer {
    /// The arrow's textual twin. Never the arrow alone.
    pub fn direction_word(&self) -> &'static str {
        if self.outgoing {
            "To"
        } else {
            "From"
        }
    }

    /// The second line: who, and how it ended.
    pub fn line(&self) -> String {
        format!(
            "{} {} · {}",
            self.direction_word(),
            self.peer_name,
            self.outcome.label()
        )
    }

    /// What a screen reader says for the row, as a sentence with the
    /// direction in words.
    pub fn accessible_label(&self) -> String {
        let preposition = if self.outgoing { "to" } else { "from" };
        format!(
            "{} {} {} {}",
            self.outcome.label(),
            self.filename,
            preposition,
            self.peer_name
        )
    }

    /// The longer sentence, for the tooltip and the accessible description.
    ///
    /// Says *who* declined, which the one-word status deliberately does not:
    /// an outgoing transfer the peer refused and an incoming one this person
    /// refused are the same word and opposite events.
    pub fn detail(&self) -> String {
        let what = match (self.outcome, self.outgoing) {
            (Outcome::Sent, _) => format!("{} reached {}.", self.filename, self.peer_name),
            (Outcome::Received, _) => {
                format!("{} arrived from {}.", self.filename, self.peer_name)
            }
            (Outcome::Declined, true) => format!("{} declined this file.", self.peer_name),
            (Outcome::Declined, false) => {
                format!(
                    "This computer declined {} from {}.",
                    self.filename, self.peer_name
                )
            }
            (Outcome::Cancelled, _) => format!("{} was cancelled.", self.filename),
            (Outcome::TimedOut, _) => {
                format!("{} was not answered in time.", self.filename)
            }
            (Outcome::Disconnected, _) => format!(
                "The connection to {} ended before {} finished.",
                self.peer_name, self.filename
            ),
            (Outcome::Failed, _) => format!("{} did not finish.", self.filename),
        };
        format!(
            "{what} Device {}. Open OmniBridge Settings for the full list.",
            widgets_group(&self.peer_fingerprint_short)
        )
    }
}

/// The short fingerprint as the rest of the application spaces it.
///
/// Duplicated here rather than reached for from `widgets` because this module
/// holds no GTK and is tested with no display.
fn widgets_group(short: &str) -> String {
    let compact: String = short.chars().filter(|c| !c.is_whitespace()).collect();
    if compact.len() != 16 || !compact.chars().all(|c| c.is_ascii_hexdigit()) {
        return short.to_string();
    }
    compact
        .to_uppercase()
        .as_bytes()
        .chunks(4)
        .map(|c| String::from_utf8_lossy(c).into_owned())
        .collect::<Vec<_>>()
        .join(" ")
}

/// The finished transfers worth naming: newest first, at most
/// [`RECENT_LIMIT`].
///
/// # Why the daemon's counter and not the list's order
///
/// The daemon keeps transfers in a map keyed by transfer id, and a transfer id
/// is 128 random bits — so the order this list arrives in is the order of a
/// random number, and "the first one" means nothing. `seq` is the daemon's own
/// creation counter and is the only thing here that knows which is newer.
/// Sorting by anything else would be the list-position defect again.
fn recent_transfers(state: &DaemonState) -> Vec<RecentTransfer> {
    let Some(transfers) = state.transfers.as_deref() else {
        return Vec::new();
    };
    let mut done: Vec<RecentTransfer> = transfers
        .iter()
        .filter_map(|t| {
            // Terminal only. An in-flight transfer has its own line above and
            // must not appear twice, once as progress and once as an outcome.
            let outgoing = outgoing(&t.direction)?;
            let outcome = Outcome::read(&t.state, t.failure_code.as_deref(), outgoing)?;
            Some(RecentTransfer {
                filename: t.filename.clone(),
                peer_name: t.device_name.clone(),
                peer_fingerprint_short: t.fingerprint_short.clone(),
                outgoing,
                outcome,
                sort_key: t.seq,
            })
        })
        .collect();
    // Newest first. The tie-break keeps the order total, so the panel cannot
    // shuffle two rows under the pointer between polls.
    done.sort_by(|a, b| {
        b.sort_key
            .cmp(&a.sort_key)
            .then_with(|| a.filename.cmp(&b.filename))
    });
    done.truncate(RECENT_LIMIT);
    done
}

// ---------------------------------------------------------------------------
// The panel
// ---------------------------------------------------------------------------

/// Everything the Quick Panel draws, derived once per poll.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PanelModel {
    pub health: Health,
    /// This computer's name, for the header. `None` until the daemon answers.
    pub this_device: Option<String>,
    /// Trusted, non-revoked peers, in a stable display order.
    pub peers: Vec<PeerCard>,
    pub target: Target,
    pub send_file: Action,
    pub send_clipboard: Action,
    pub files: StatusLine,
    pub clipboard: StatusLine,
    pub notifications: StatusLine,
    /// The one transfer in flight, if any. In-flight only — see
    /// [`TransferLine`].
    pub transfer: Option<TransferLine>,
    /// The last few finished transfers, newest first, at most
    /// [`RECENT_LIMIT`]. Empty when there are none — the panel draws no
    /// section rather than an empty one.
    ///
    /// Not a history and not an authority: see [`RecentTransfer`]. Nothing
    /// here is routed on, and reading it cannot change [`PanelModel::target`].
    pub recent: Vec<RecentTransfer>,
}

impl PanelModel {
    /// Builds the panel from one poll of the daemon and the stored choice.
    pub fn build(state: &DaemonState, chosen: Option<&str>) -> PanelModel {
        let health = health_of(state);
        let peers = peer_cards(state, chosen);
        // Resolved from the peers the panel actually shows, after the stored
        // choice has been marked on them, so "what is selected" and "where
        // this sends" cannot disagree.
        let target = Target::resolve(&peers, chosen);
        let peer = target
            .fingerprint()
            .and_then(|fp| peers.iter().find(|p| p.fingerprint == fp));

        let send_file = file_action(&health, &target, peer);
        let send_clipboard = clipboard_action(&health, &target, peer, state.clipboard.as_ref());

        PanelModel {
            files: files_status(&health, peer),
            clipboard: clipboard_status(&health, peer, state.clipboard.as_ref()),
            notifications: notifications_status(&health, peer, state.notifications.as_ref()),
            transfer: active_transfer(state, peer),
            // Derived from the transfers themselves, with no reference to
            // `target` or `chosen`: a finished transfer's identity is its
            // own. Passing the selection in here is what would let a
            // historical row be re-labelled with the current choice.
            recent: recent_transfers(state),
            this_device: state.status.as_ref().map(|s| s.device_name.clone()),
            health,
            peers,
            target,
            send_file,
            send_clipboard,
        }
    }

    /// The peer an action would reach, if any.
    pub fn target_peer(&self) -> Option<&PeerCard> {
        let fp = self.target.fingerprint()?;
        self.peers.iter().find(|p| p.fingerprint == fp)
    }
}

fn health_of(state: &DaemonState) -> Health {
    if let Some(_error) = &state.error {
        // The raw error is deliberately not the headline. It is a socket
        // path and an errno, which tells the person nothing they can act on;
        // Settings and the log still carry it verbatim.
        return Health::Unavailable {
            headline: "OmniBridge service is not available".into(),
        };
    }
    match state.status {
        Some(_) => Health::Available,
        None => Health::Reaching,
    }
}

/// The live session for one device, when exactly one connection names it.
///
/// Matched on **both** the device id and the short fingerprint, and only when
/// exactly one connection matches. Two devices that share a display name are
/// therefore never confusable here, and an ambiguous match resolves to "no
/// session" — which disables actions rather than enabling the wrong one.
fn connection_for<'a>(
    status: Option<&'a StatusReport>,
    device: &DeviceReport,
) -> Option<&'a omnibridge_control::ConnectionReport> {
    let status = status?;
    let mut hits = status.connections.iter().filter(|c| {
        c.device_id == device.device_id && c.fingerprint_short == device.fingerprint_short
    });
    let first = hits.next()?;
    match hits.next() {
        None => Some(first),
        Some(_) => None,
    }
}

fn peer_cards(state: &DaemonState, chosen: Option<&str>) -> Vec<PeerCard> {
    let chosen = chosen.map(|c| c.trim().to_ascii_lowercase());
    let devices = state
        .devices
        .as_deref()
        .or(state.status.as_ref().map(|s| s.devices.as_slice()))
        .unwrap_or(&[]);

    let mut cards: Vec<PeerCard> = devices
        .iter()
        // Revoked devices are not trusted, so they are not an everyday
        // surface. Recovering one is a Settings job and stays there.
        .filter(|d| !d.revoked && d.paired)
        .map(|device| {
            let connection = connection_for(state.status.as_ref(), device);
            let negotiated: &[String] = connection
                .map(|c| c.negotiated_capabilities.as_slice())
                .unwrap_or(&[]);

            let link = match device.state {
                DeviceState::Connected => Link::Connected,
                DeviceState::Stale => Link::Stale,
                // Revoked is filtered out above; anything else is "no live
                // session", which is what Offline means here.
                DeviceState::Disconnected | DeviceState::Revoked => Link::Offline,
            };

            let capability = |id: &str| Capability {
                granted: device.granted_capabilities.iter().any(|c| c == id),
                live: link.is_live() && negotiated.iter().any(|c| c == id),
            };

            PeerCard {
                selected: chosen
                    .as_deref()
                    .is_some_and(|c| device.fingerprint.eq_ignore_ascii_case(c)),
                battery: battery_of(device, link, negotiated),
                files: capability(FILES),
                clipboard: capability(CLIPBOARD),
                notifications: capability(NOTIFICATIONS),
                fingerprint: device.fingerprint.to_ascii_lowercase(),
                fingerprint_short: device.fingerprint_short.clone(),
                device_id: device.device_id.clone(),
                name: device.device_name.clone(),
                platform: device.platform.clone(),
                link,
            }
        })
        .collect();

    // A *display* order, and deliberately not the trust store's. U2 P1
    // measured that order changing underneath the user, because a successful
    // connection appends the peer. Rows that swap places while someone is
    // reaching for one are a hazard even though nothing routes by position.
    cards.sort_by(|a, b| {
        let rank = |p: &PeerCard| match p.link {
            Link::Connected => 0,
            Link::Stale => 1,
            Link::Offline => 2,
        };
        rank(a)
            .cmp(&rank(b))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            .then_with(|| a.fingerprint.cmp(&b.fingerprint))
    });
    cards
}

fn battery_of(device: &DeviceReport, link: Link, negotiated: &[String]) -> Battery {
    match (&device.battery, link) {
        // A reading is a reading, including zero.
        (Some(b), _) => Battery::Present {
            percent: b.percentage.min(100) as u8,
            status: charge_status(&b.charging_state).to_string(),
            // A reading arriving over a session that has stopped answering
            // its probes is history whatever its own age says.
            stale: b.stale || link == Link::Stale,
        },
        // Live, running battery.v1, and saying nothing. `battery.v1` reports
        // absence by sending no frame, so this is the closest to "no battery"
        // this end can honestly get — and it is still not a number.
        (None, Link::Connected | Link::Stale) if negotiated.iter().any(|c| c == BATTERY) => {
            Battery::Absent
        }
        // Offline, or a session that never negotiated battery.v1. We are not
        // in a position to say anything.
        (None, _) => Battery::Unavailable,
    }
}

/// The proto enum's Rust name, as a phrase.
fn charge_status(raw: &str) -> &str {
    match raw {
        "Charging" => "Charging",
        "Full" => "Full",
        "NotCharging" => "Not charging",
        // `Discharging` is the ordinary case and adds nothing next to a
        // percentage; `Unspecified` is the device declining to say.
        _ => "",
    }
}

// ---------------------------------------------------------------------------
// Action availability
// ---------------------------------------------------------------------------

/// The refusals every quick action shares, in the order they are asked.
///
/// Returns the peer when there is nothing in the way.
fn preconditions<'a>(
    health: &Health,
    target: &Target,
    peer: Option<&'a PeerCard>,
) -> Result<&'a PeerCard, Action> {
    match health {
        Health::Unavailable { headline } => return Err(Action::blocked(headline.clone())),
        Health::Reaching => return Err(Action::blocked("Connecting to the OmniBridge service…")),
        Health::Available => {}
    }
    match target {
        Target::NoTrustedPeer => {
            return Err(Action::blocked(
                "No device is paired yet. Pair one in OmniBridge Settings.",
            ))
        }
        Target::MustChoose { stale_choice: true } => {
            return Err(Action::blocked(
                "The device you chose is no longer paired. Choose a device.",
            ))
        }
        Target::MustChoose {
            stale_choice: false,
        } => return Err(Action::blocked("Choose which device to send to.")),
        Target::Selected(_) | Target::OnlyTrustedPeer(_) => {}
    }
    // `target` named a fingerprint and the peers were built from the same
    // poll, so this is unreachable in practice; refusing rather than
    // unwrapping keeps it unreachable in principle too.
    let peer = peer.ok_or_else(|| Action::blocked("That device is no longer available."))?;
    if !peer.link.is_live() {
        return Err(Action::blocked(format!(
            "{} is {}.",
            peer.name,
            peer.link.label().to_lowercase()
        )));
    }
    Ok(peer)
}

fn ready(peer: &PeerCard) -> Action {
    Action::Ready {
        fingerprint: peer.fingerprint.clone(),
        peer_name: peer.name.clone(),
    }
}

fn file_action(health: &Health, target: &Target, peer: Option<&PeerCard>) -> Action {
    let peer = match preconditions(health, target, peer) {
        Ok(peer) => peer,
        Err(blocked) => return blocked,
    };
    if !peer.files.granted {
        return Action::blocked(format!("Files are not enabled for {}.", peer.name));
    }
    if !peer.files.live {
        return Action::blocked(format!(
            "This session with {} has not negotiated file transfer.",
            peer.name
        ));
    }
    ready(peer)
}

fn clipboard_action(
    health: &Health,
    target: &Target,
    peer: Option<&PeerCard>,
    report: Option<&ClipboardStatusReport>,
) -> Action {
    let peer = match preconditions(health, target, peer) {
        Ok(peer) => peer,
        Err(blocked) => return blocked,
    };
    let Some(report) = report else {
        return Action::blocked("Waiting for the OmniBridge service…");
    };
    if !report.enabled {
        return Action::blocked("Clipboard sharing is not enabled on this computer.");
    }
    if !report.backend_available {
        return Action::blocked("This desktop session has no working clipboard backend.");
    }
    if !peer.clipboard.granted {
        return Action::blocked(format!("Clipboard is not enabled for {}.", peer.name));
    }
    if !peer.clipboard.live {
        return Action::blocked(format!(
            "This session with {} has not negotiated the clipboard.",
            peer.name
        ));
    }
    match clipboard_peer(report, peer) {
        Some(p) if p.allow_send => ready(peer),
        Some(_) => Action::blocked(format!(
            "Sending the clipboard to {} is turned off.",
            peer.name
        )),
        None => Action::blocked(format!(
            "The OmniBridge service has no clipboard policy for {}.",
            peer.name
        )),
    }
}

/// One peer's clipboard policy, matched the way [`connection_for`] matches —
/// on device id and short fingerprint together, and only when unambiguous.
fn clipboard_peer<'a>(
    report: &'a ClipboardStatusReport,
    peer: &PeerCard,
) -> Option<&'a ClipboardPeerReport> {
    let mut hits = report
        .peers
        .iter()
        .filter(|p| p.device_id == peer.device_id && p.fingerprint_short == peer.fingerprint_short);
    let first = hits.next()?;
    hits.next().is_none().then_some(first)
}

fn notification_peer<'a>(
    report: &'a NotificationsStatusReport,
    peer: &PeerCard,
) -> Option<&'a NotificationPeerReport> {
    let mut hits = report
        .peers
        .iter()
        .filter(|p| p.device_id == peer.device_id && p.fingerprint_short == peer.fingerprint_short);
    let first = hits.next()?;
    hits.next().is_none().then_some(first)
}

// ---------------------------------------------------------------------------
// Status rows
// ---------------------------------------------------------------------------

fn files_status(health: &Health, peer: Option<&PeerCard>) -> StatusLine {
    if !health.is_available() {
        return StatusLine::new(
            StatusValue::Unavailable,
            "The OmniBridge service is not running.",
        );
    }
    let Some(peer) = peer else {
        return StatusLine::new(StatusValue::Unavailable, "No device selected.");
    };
    if !peer.files.granted {
        return StatusLine::new(
            StatusValue::Off,
            format!("Files are not enabled for {}.", peer.name),
        );
    }
    if peer.files.usable() {
        StatusLine::new(
            StatusValue::On,
            format!(
                "Available. Files {} sends still need your approval here.",
                peer.name
            ),
        )
    } else {
        StatusLine::new(
            StatusValue::Off,
            format!("Available once {} is connected.", peer.name),
        )
    }
}

/// The clipboard row, and the one place the Android platform limit has to be
/// told truthfully.
///
/// The two directions are separate settings and only one of them can ever be
/// automatic:
///
/// * **this computer → the device** may be automatic, if this session can
///   observe clipboard changes at all (`watch_available`; GNOME cannot);
/// * **the device → this computer** covers only what arrives. A modern
///   Android cannot read its own clipboard in the background, so nothing
///   arrives unless a person asks it to on the phone — and `auto_receive`
///   decides what happens to a clip *after* it lands, not whether it is sent.
///
/// Calling any of that "sync" would be the false claim the brief forbids, so
/// the word does not appear.
fn clipboard_status(
    health: &Health,
    peer: Option<&PeerCard>,
    report: Option<&ClipboardStatusReport>,
) -> StatusLine {
    if !health.is_available() {
        return StatusLine::new(
            StatusValue::Unavailable,
            "The OmniBridge service is not running.",
        );
    }
    let Some(report) = report else {
        return StatusLine::new(
            StatusValue::Unavailable,
            "Waiting for the OmniBridge service.",
        );
    };
    if !report.enabled {
        return StatusLine::new(
            StatusValue::Unavailable,
            "Clipboard sharing is not enabled on this computer.",
        );
    }
    if !report.backend_available {
        return StatusLine::new(
            StatusValue::Unavailable,
            "This desktop session has no working clipboard backend.",
        );
    }
    let Some(peer) = peer else {
        return StatusLine::new(StatusValue::Unavailable, "No device selected.");
    };
    if !peer.clipboard.granted {
        return StatusLine::new(
            StatusValue::Off,
            format!("Clipboard is not enabled for {}.", peer.name),
        );
    }
    let Some(policy) = clipboard_peer(report, peer) else {
        return StatusLine::new(
            StatusValue::Unavailable,
            format!("No clipboard policy for {}.", peer.name),
        );
    };

    let sending = if !policy.allow_send {
        format!("Not sending to {}.", peer.name)
    } else if policy.auto_send && report.watch_available {
        format!(
            "Sends this computer's clipboard to {} as it changes.",
            peer.name
        )
    } else {
        "Sends this computer's clipboard only when you press Send clipboard.".to_string()
    };

    let receiving = if !policy.allow_receive {
        format!("Not accepting clips from {}.", peer.name)
    } else if policy.auto_receive {
        format!(
            "Clips from {} replace this clipboard as they arrive.",
            peer.name
        )
    } else {
        format!("Clips from {} are held until you apply them.", peer.name)
    };

    // Stated for a mobile peer because it is the half a person will
    // otherwise assume works both ways.
    let caveat = if peer.is_mobile() {
        format!(" {} sends only when you ask it to there.", peer.name)
    } else {
        String::new()
    };

    // What the *peer* said about the last clip this computer sent it.
    //
    // This row is where "Clipboard submitted to X; awaiting confirmation"
    // resolves, which is why it now states the confirming outcomes as well as
    // the refusing ones (QP-DEBT-06). A `ClipboardSend` is answered as soon as
    // the frame is on the session — that is all the sending end can know at
    // the time — and the receiver's verdict arrives afterwards. Found on real
    // hardware: the panel said "Clipboard sent to SM-X620" and the tablet had
    // refused it, because the grant on the *Android* side was missing. The
    // desktop cannot know that in advance; it can stop claiming otherwise, and
    // it can say so here when the answer comes.
    let verdict = match policy.last_outcome.as_deref() {
        None => String::new(),
        Some(outcome) => clipboard_outcome_note(outcome, &peer.name),
    };

    let value = if policy.allow_send || policy.allow_receive {
        StatusValue::On
    } else {
        StatusValue::Off
    };
    StatusLine::new(value, format!("{sending} {receiving}{caveat}{verdict}"))
}

/// The notifications row: grant and policy, never mere advertisement.
///
/// A device advertising `notifications.v1` says only that it *could*. What is
/// reported here is whether this desktop has a server to show them on,
/// whether the grant exists, and whether the per-peer mirror switch is on —
/// which is what actually decides whether anything appears.
///
/// No notification content reaches this function. `NotificationsStatusReport`
/// has no field that could carry a title, a body or an application name, so
/// the panel is structurally incapable of becoming the history the design
/// forbids.
fn notifications_status(
    health: &Health,
    peer: Option<&PeerCard>,
    report: Option<&NotificationsStatusReport>,
) -> StatusLine {
    if !health.is_available() {
        return StatusLine::new(
            StatusValue::Unavailable,
            "The OmniBridge service is not running.",
        );
    }
    let Some(report) = report else {
        return StatusLine::new(
            StatusValue::Unavailable,
            "Waiting for the OmniBridge service.",
        );
    };
    if !report.enabled {
        return StatusLine::new(
            StatusValue::Unavailable,
            "Notification mirroring is not enabled on this computer.",
        );
    }
    if !report.available {
        return StatusLine::new(
            StatusValue::Unavailable,
            "No notification server is running in this desktop session.",
        );
    }
    let Some(peer) = peer else {
        return StatusLine::new(StatusValue::Unavailable, "No device selected.");
    };
    let Some(policy) = notification_peer(report, peer) else {
        return StatusLine::new(
            StatusValue::Off,
            format!("Notifications are not enabled for {}.", peer.name),
        );
    };
    if !policy.granted || policy.revoked {
        return StatusLine::new(
            StatusValue::Off,
            format!("Notifications are not enabled for {}.", peer.name),
        );
    }
    if !policy.allow_mirror {
        return StatusLine::new(
            StatusValue::Off,
            format!("Mirroring is turned off for {}.", peer.name),
        );
    }

    let mut detail = format!("Showing notifications from {}.", peer.name);
    if policy.connected && !policy.peer_is_source {
        detail.push_str(" That device is not sending any.");
    }
    match policy.when_locked.as_str() {
        "suppress" => detail.push_str(" Hidden while this screen is locked."),
        "app-only" => detail.push_str(" Only the app name is shown while this screen is locked."),
        _ => {}
    }
    StatusLine::new(StatusValue::On, detail)
}

#[cfg(test)]
mod tests;
