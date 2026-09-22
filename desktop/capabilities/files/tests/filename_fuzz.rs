//! SEC-FUZZ-01 — a bounded randomized campaign over the filename sanitiser.
//!
//! The companion to `core/tests/parser_fuzz.rs`, which covers framing, the
//! protobuf decoder and device names. This is a separate file because
//! `omnibridge-core` does not depend on a capability and must not start, so
//! the generator below is duplicated rather than the crate layering bent to
//! share a thirty-line test helper.
//!
//! # Why this parser earns its own campaign
//!
//! It has the worst failure mode in the product: its output becomes a **path
//! on the receiving machine**.
//! `f7_f8_a_hostile_filename_cannot_escape_the_download_directory` covers the
//! hostile cases somebody thought of; this covers the shapes nobody wrote
//! down.
//!
//! Seeded, bounded, structured random testing — **not** coverage-guided
//! fuzzing: `cargo-fuzz` needs a nightly toolchain and there is no `rustup` on
//! the certification host. A failure prints its seed and reruns identically.

use omnibridge_capability_files::filename;

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
}

/// The campaign's size. Small enough to run in every `cargo test`: a fuzz
/// harness that only runs when somebody remembers to is one that never runs.
const ITERATIONS: usize = 20_000;

// ---------------------------------------------------------------------------
// Filenames
// ---------------------------------------------------------------------------

/// No generated name, however hostile, escapes its directory.
#[test]
fn sec_fuzz_01_no_generated_filename_escapes_its_directory() {
    let mut rng = Rng::new(0x5ECF_11E7_A3E0_9C42);
    let mut accepted = 0usize;
    let mut refused = 0usize;

    for i in 0..ITERATIONS {
        let seed = rng.0;
        let raw = adversarial_filename(&mut rng);

        match filename::sanitize(&raw) {
            None => refused += 1,
            Some(safe) => {
                accepted += 1;
                assert!(
                    !safe.is_empty(),
                    "iteration {i} (seed {seed:#x}): empty name accepted from {raw:?}"
                );
                assert!(
                    !safe.contains('/') && !safe.contains('\\'),
                    "iteration {i} (seed {seed:#x}): separator survived: {safe:?} from {raw:?}"
                );
                assert!(
                    !safe.contains('\0'),
                    "iteration {i} (seed {seed:#x}): NUL survived: {safe:?} from {raw:?}"
                );
                assert!(
                    safe != "." && safe != "..",
                    "iteration {i} (seed {seed:#x}): {safe:?} is a directory reference"
                );
                // The property a traversal actually has to defeat: joining the
                // accepted name to a base must stay under the base, and must
                // add exactly one component.
                let base = std::path::Path::new("/tmp/omnibridge-downloads");
                let joined = base.join(&safe);
                assert!(
                    joined.starts_with(base),
                    "iteration {i} (seed {seed:#x}): {safe:?} escaped the base as \
                     {joined:?} (from {raw:?})"
                );
                assert_eq!(
                    joined.components().count(),
                    base.components().count() + 1,
                    "iteration {i} (seed {seed:#x}): {safe:?} added more than one \
                     path component (from {raw:?})"
                );
            }
        }
    }

    println!("SEC-FUZZ-01 filenames: {ITERATIONS} inputs — {accepted} accepted, {refused} refused");
    assert!(
        accepted > 0,
        "every generated name was refused; the campaign never exercised the \
         accepting path and proves nothing"
    );
    assert!(
        refused > 0,
        "every generated name was accepted; the generator is not producing \
         hostile names"
    );
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
