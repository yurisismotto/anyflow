//! Host-specific implementations of the core's seams.
//!
//! # Why these live in `anyflow-core` at all
//!
//! They do not, in the sense that matters: every module here is behind a
//! Cargo feature, and with `--no-default-features` `anyflow-core` contains no
//! `std::os` anything and no environment or filesystem assumption. That is
//! the boundary the Wave 0 compile gate checks:
//!
//! ```text
//! cargo check -p anyflow-core --no-default-features --target x86_64-pc-windows-msvc
//! ```
//!
//! What keeps them in this crate rather than in `anyflow-linux` is
//! compatibility constraint **CC-5**: `Store::open(dir)` is called by the
//! existing test suite, which no refactor is permitted to edit. Moving the
//! constructor to an adapter crate would mean editing tests to make the
//! refactor pass, and a refactor that needs a test edited has changed
//! behaviour.
//!
//! The rule for anything added here: it must be feature-gated, it must be the
//! only place in the crate that names a platform, and the portable code above
//! it must reach it exclusively through a trait in [`crate::secret_store`] or
//! [`crate::identity`] — never by calling into this module directly.

#[cfg(feature = "unix-fs")]
pub mod unix_fs;
