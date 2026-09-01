//! The AnyFlow Agent, minus the platform it runs on.
//!
//! Everything the always-on user-session process does that is not specific to
//! one operating system: the TCP listener, the mDNS advertisement, the shared
//! daemon state, and the control-endpoint server. It is a library rather than
//! a binary so that the integration tests can start an agent in-process
//! instead of spawning one, and so that a second host process — a Windows
//! user-session agent, a macOS `LoginItem` — can compose it with a different
//! adapter without a fork.
//!
//! # What "platform-free" means here
//!
//! Concretely: this crate binds no Unix socket, resolves no XDG path, reads
//! no `/proc`, and sets no file mode. Where it needs one of those it takes a
//! trait — [`anyflow_control::transport::ControlTransport`] for the control
//! endpoint, [`anyflow_core::secret_store::SecretStore`] for persistence —
//! and the adapter crate supplies it. `anyflow-linux` is the only adapter
//! Wave 0 ships.
//!
//! It is *not* claimed to compile for a non-Unix target today: `mdns-sd`'s
//! Windows behaviour is an open question (V-12 / POC-WIN-02) and deliberately
//! outside Wave 0, which is why this crate is excluded from the portable
//! compile gate while `anyflow-core`, `anyflow-control` and the capability
//! crates are in it.

/// The control-protocol types, re-exported so the agent's own code and its
/// tests keep one import path for them.
pub use anyflow_control as control;

pub mod listener;
pub mod mdns;
pub mod server;
pub mod state;
