//! AnyFlow daemon.
//!
//! Exposed as a library as well as a binary so that the CLI can share the
//! control-protocol types, and so integration tests can start a daemon
//! in-process instead of spawning one.

pub mod control;
pub mod listener;
pub mod mdns;
pub mod server;
pub mod state;
