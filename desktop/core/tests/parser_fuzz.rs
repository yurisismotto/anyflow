//! SEC-FUZZ-01 — a bounded randomized campaign over the network-facing parsers.
//!
//! # What this is, and what it is not
//!
//! It is **seeded, bounded, structured random testing**. It is **not**
//! coverage-guided fuzzing: `cargo-fuzz` drives libFuzzer, libFuzzer needs a
//! nightly toolchain, and there is no `rustup` on the certification host — so
//! the honest description is the one in this sentence rather than the word
//! "fuzzing" doing work it has not earned.
//!
//! What that costs: no coverage feedback, so this will not discover a path
//! that needs a specific 8-byte magic number to reach. What it still buys, and
//! why it is worth having:
//!
//! * every input below reaches a **real** parser — the same `read_envelope`
//!   the session loop calls and the same protobuf decode, with no mock in
//!   between;
//! * the generators are **shaped**, not uniform noise. Uniform bytes almost
//!   never produce a valid protobuf tag, so a uniform campaign would spend its
//!   whole budget being rejected at the first byte. These build wire-plausible
//!   structures and then corrupt them, which is where parser bugs live;
//! * it is **deterministic**. A failure prints the seed and the input, and
//!   reruns identically. A fuzzer that cannot reproduce its own finding is a
//!   rumour.
//!
//! # The property
//!
//! Every parser must **return** on every input. Not succeed — return. A
//! malformed frame from an unauthenticated peer is an ordinary event on a LAN
//! and the only acceptable outcomes are a value or an error.
//!
//! A panic is a failure of this gate. So is a hang, and so is an allocation
//! proportional to an attacker-supplied length.
//!
//! The filename sanitiser gets the same treatment in
//! `capabilities/files/tests/filename_fuzz.rs`. It is a separate file because
//! `omnibridge-core` does not depend on a capability and must not start — the
//! generator is duplicated there rather than the crate layering bent to share
//! a thirty-line test helper.

use std::time::{Duration, Instant};

use omnibridge_core::framing::{self, MAX_FRAME_LEN};

// ---------------------------------------------------------------------------
// A deterministic generator
// ---------------------------------------------------------------------------

/// xorshift64*. Chosen because it is ten lines, has no dependencies, and its
/// whole state is one `u64` that can be printed in a failure message.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        // A zero state is a fixed point for xorshift, which would make the
        // whole campaign a single repeated input.
        Self(seed | 1)
    }
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            (self.next_u64() % n as u64) as usize
        }
    }
    fn byte(&mut self) -> u8 {
        (self.next_u64() >> 24) as u8
    }
    fn bytes(&mut self, len: usize) -> Vec<u8> {
        (0..len).map(|_| self.byte()).collect()
    }
    /// A value that is interesting to a parser far more often than a uniform
    /// one: boundaries, off-by-ones and the limits themselves.
    fn interesting_u32(&mut self) -> u32 {
        const CANDIDATES: &[u32] = &[
            0,
            1,
            2,
            127,
            128,
            255,
            256,
            65_535,
            65_536,
            MAX_FRAME_LEN - 1,
            MAX_FRAME_LEN,
            MAX_FRAME_LEN + 1,
            u32::MAX - 1,
            u32::MAX,
        ];
        if self.next_u64().is_multiple_of(4) {
            self.next_u64() as u32
        } else {
            CANDIDATES[self.below(CANDIDATES.len())]
        }
    }
}

/// The campaign's size. Small enough to run in every `cargo test`, which is
/// the only way a fuzz harness stays honest — one that only runs when somebody
/// remembers to is one that never runs.
const ITERATIONS: usize = 20_000;
/// A single input must never take this long. A parser that does is a denial of
/// service whether or not it eventually returns.
const PER_INPUT_BUDGET: Duration = Duration::from_millis(250);

// ---------------------------------------------------------------------------
// Framing
// ---------------------------------------------------------------------------

/// `read_envelope` over adversarial byte streams.
///
/// The framing layer is the first thing an unauthenticated peer reaches: it
/// runs **before** the handshake has decided anything about who is talking.
#[tokio::test]
async fn sec_fuzz_01_the_framing_parser_returns_on_every_input() {
    let mut rng = Rng::new(0xF3A1_2C55_0BE7_1D04);
    let mut outcomes = [0usize; 3]; // ok, error, truncated

    for i in 0..ITERATIONS {
        let seed = rng.0;
        let input = adversarial_frame(&mut rng);

        let started = Instant::now();
        let mut cursor = std::io::Cursor::new(input.clone());
        let result = framing::read_envelope(&mut cursor).await;
        let elapsed = started.elapsed();

        assert!(
            elapsed < PER_INPUT_BUDGET,
            "iteration {i} (seed {seed:#x}) took {elapsed:?} on {} bytes — a parser \
             that is slow on attacker input is a denial of service\ninput: {:02x?}",
            input.len(),
            &input[..input.len().min(64)]
        );

        match result {
            Ok(_) => outcomes[0] += 1,
            Err(omnibridge_core::Error::Closed) => outcomes[2] += 1,
            Err(_) => outcomes[1] += 1,
        }
    }

    // The campaign must have actually exercised the parser, and must have
    // reached more than one outcome. A run that only ever hit "truncated"
    // would be testing `read_exact` and nothing else.
    println!(
        "SEC-FUZZ-01 framing: {ITERATIONS} inputs — {} decoded, {} rejected, {} truncated",
        outcomes[0], outcomes[1], outcomes[2]
    );
    assert!(
        outcomes[0] > 0,
        "no input ever decoded; the generator is not producing valid frames \
         and the campaign proves nothing"
    );
    assert!(
        outcomes[1] > 0,
        "no input was ever rejected as malformed; the generator is not \
         producing invalid frames"
    );
}

/// A length prefix and a body, related to each other in every wrong way.
fn adversarial_frame(rng: &mut Rng) -> Vec<u8> {
    let mut out = Vec::new();
    match rng.below(6) {
        // A declared length with a body that does not match it. The classic
        // framing bug: trusting the prefix.
        0 => {
            out.extend(rng.interesting_u32().to_be_bytes());
            let body = rng.below(300);
            out.extend(rng.bytes(body));
        }
        // A well-formed prefix over random bytes: reaches the protobuf decoder.
        1 => {
            let n = 1 + rng.below(400);
            let body = rng.bytes(n);
            out.extend((body.len() as u32).to_be_bytes());
            out.extend(body);
        }
        // A real envelope, then corrupted. The most productive shape: it gets
        // past the tag parser before it goes wrong.
        2 => {
            let mut body = plausible_envelope(rng);
            let flips = 1 + rng.below(4);
            for _ in 0..flips {
                if body.is_empty() {
                    break;
                }
                let at = rng.below(body.len());
                body[at] ^= 1 << rng.below(8);
            }
            out.extend((body.len() as u32).to_be_bytes());
            out.extend(body);
        }
        // A real envelope, truncated mid-body.
        3 => {
            let body = plausible_envelope(rng);
            let keep = rng.below(body.len().max(1));
            out.extend((body.len() as u32).to_be_bytes());
            out.extend(&body[..keep]);
        }
        // Fewer than four bytes: the prefix itself is incomplete.
        4 => {
            let n = rng.below(4);
            out.extend(rng.bytes(n));
        }
        // A valid envelope, unmodified. The control: without these the
        // "something decoded" assertion could never hold.
        _ => {
            let body = plausible_envelope(rng);
            out.extend((body.len() as u32).to_be_bytes());
            out.extend(body);
        }
    }
    out
}

/// Bytes shaped like a protobuf message.
///
/// Uniform random bytes are almost never a valid protobuf, so a campaign built
/// on them would be rejected at the first tag and would never reach the
/// decoder's interesting paths. These carry real field tags and real varints.
fn plausible_envelope(rng: &mut Rng) -> Vec<u8> {
    let mut out = Vec::new();
    let fields = 1 + rng.below(6);
    for _ in 0..fields {
        let field_number = 1 + rng.below(24) as u64;
        // 0 varint, 1 fixed64, 2 length-delimited, 5 fixed32. 3 and 4 are the
        // deprecated group types and are included on purpose: they are a
        // decoder path most messages never take.
        let wire_type = [0u64, 1, 2, 5, 3, 4][rng.below(6)];
        write_varint(&mut out, (field_number << 3) | wire_type);
        match wire_type {
            0 => write_varint(&mut out, rng.next_u64()),
            1 => out.extend(rng.bytes(8)),
            5 => out.extend(rng.bytes(4)),
            2 => {
                let len = rng.below(64);
                // Half the time the declared length is a lie, which is the
                // length-delimited equivalent of the framing bug above.
                let declared = if rng.next_u64().is_multiple_of(2) {
                    len as u64
                } else {
                    rng.interesting_u32() as u64
                };
                write_varint(&mut out, declared);
                out.extend(rng.bytes(len));
            }
            _ => {}
        }
    }
    out
}

fn write_varint(out: &mut Vec<u8>, mut v: u64) {
    loop {
        let byte = (v & 0x7f) as u8;
        v >>= 7;
        if v == 0 {
            out.push(byte);
            return;
        }
        out.push(byte | 0x80);
    }
}

// ---------------------------------------------------------------------------
// A declared length must never become an allocation
// ---------------------------------------------------------------------------

/// A frame that claims to be enormous is rejected on the claim, not after
/// allocating for it.
///
/// `read_envelope` allocates `vec![0u8; len]` *after* the bound check. This
/// asserts the order: a peer that says `u32::MAX` must be refused before four
/// gigabytes are reserved on its say-so.
#[tokio::test]
async fn sec_fuzz_01_an_enormous_declared_length_is_refused_before_allocating() {
    for declared in [MAX_FRAME_LEN + 1, 1 << 30, u32::MAX - 1, u32::MAX] {
        // Only the four length bytes are supplied. If the parser allocated
        // first and read second, it would reserve `declared` bytes before
        // discovering there is no body.
        let input = declared.to_be_bytes().to_vec();
        let started = Instant::now();
        let mut cursor = std::io::Cursor::new(input);
        let result = framing::read_envelope(&mut cursor).await;
        let elapsed = started.elapsed();

        assert!(
            matches!(result, Err(omnibridge_core::Error::FrameTooLarge(..))),
            "a frame declaring {declared} bytes was not refused as too large: {result:?}"
        );
        assert!(
            elapsed < PER_INPUT_BUDGET,
            "refusing a {declared}-byte claim took {elapsed:?}; the bound check \
             is happening after the allocation"
        );
    }
}

/// Zero-length frames are refused rather than treated as an empty message.
#[tokio::test]
async fn sec_fuzz_01_a_zero_length_frame_is_refused() {
    let mut cursor = std::io::Cursor::new(0u32.to_be_bytes().to_vec());
    assert!(matches!(
        framing::read_envelope(&mut cursor).await,
        Err(omnibridge_core::Error::Protocol(_))
    ));
}

/// Names built out of the pieces traversals are made of.
fn adversarial_filename(rng: &mut Rng) -> String {
    const PIECES: &[&str] = &[
        "..",
        ".",
        "/",
        "\\",
        "....//",
        "..%2f",
        "%2e%2e/",
        "\u{202e}", // right-to-left override
        "\u{ff0e}", // fullwidth full stop
        "\u{2044}", // fraction slash
        "\u{0000}", // NUL
        "\r\n",
        "etc",
        "passwd",
        "C:",
        "~",
        "$HOME",
        "con", // reserved on Windows
        "report.pdf",
        "a",
        " ",
        "\t",
        "\u{200b}", // zero-width space
        "\u{feff}", // BOM
    ];

    match rng.below(8) {
        // Absolute paths.
        0 => {
            let n = 1 + rng.below(4);
            format!("/{}", join(rng, PIECES, n))
        }
        // Long runs of dot-dot.
        1 => "../".repeat(1 + rng.below(40)) + "etc/passwd",
        // A very long name: the length limit's territory.
        2 => "A".repeat(1 + rng.below(4096)),
        // Arbitrary Unicode, including unpaired-surrogate-adjacent scalars.
        3 => (0..1 + rng.below(40))
            .map(|_| char::from_u32(rng.next_u64() as u32 & 0x10_ffff).unwrap_or('\u{fffd}'))
            .collect(),
        // Random bytes reinterpreted lossily, which is what a non-UTF-8 name
        // from a hostile peer looks like by the time it is a String.
        4 => {
            let n = 1 + rng.below(60);
            String::from_utf8_lossy(&rng.bytes(n)).into_owned()
        }
        // Empty, and whitespace-only.
        5 => " ".repeat(rng.below(4)),
        // Pieces joined by a separator.
        6 => {
            let n = 1 + rng.below(6);
            join(rng, PIECES, n)
        }
        // Pieces concatenated with no separator at all.
        _ => (0..1 + rng.below(6))
            .map(|_| PIECES[rng.below(PIECES.len())])
            .collect::<String>(),
    }
}

fn join(rng: &mut Rng, pieces: &[&str], n: usize) -> String {
    (0..n)
        .map(|_| pieces[rng.below(pieces.len())])
        .collect::<Vec<_>>()
        .join("/")
}

// ---------------------------------------------------------------------------
// Device names
// ---------------------------------------------------------------------------

/// `sanitize_device_name` over adversarial input.
///
/// A device name arrives over mDNS from anything on the LAN, before any
/// pairing, and ends up rendered in a GUI and written to a log.
#[test]
fn sec_fuzz_01_device_name_sanitising_returns_on_every_input() {
    let mut rng = Rng::new(0xD3C1_CE0A_4E77_B215);
    let mut nonempty = 0usize;

    for i in 0..ITERATIONS {
        let seed = rng.0;
        let raw = adversarial_filename(&mut rng); // the same hostile corpus
        let out = omnibridge_core::discovery::sanitize_device_name(&raw);

        assert!(
            !out.contains('\0'),
            "iteration {i} (seed {seed:#x}): NUL survived in {out:?} from {raw:?}"
        );
        assert!(
            !out.contains('\n') && !out.contains('\r'),
            "iteration {i} (seed {seed:#x}): a newline survived in {out:?} from {raw:?} — \
             a device name reaches the journal, and a newline there is log injection"
        );
        if !out.is_empty() {
            nonempty += 1;
        }
    }

    println!("SEC-FUZZ-01 device names: {ITERATIONS} inputs — {nonempty} non-empty results");
    assert!(
        nonempty > 0,
        "every name sanitised to nothing; the campaign proves nothing"
    );
}
