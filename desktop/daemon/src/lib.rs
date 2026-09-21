//! `omnibridged` — the Linux row of the OmniBridge Agent.
//!
//! After Wave 0 this crate is a *composition*, not an implementation: it
//! wires the portable agent (`omnibridge-runtime`) to the Linux adapter
//! (`omnibridge-linux`) and adds a `main`. The modules below are re-exports so
//! that existing callers — the integration tests above all — keep one import
//! path while the code behind it lives where it belongs.
//!
//! | Module | Now lives in | Why |
//! | --- | --- | --- |
//! | `control` | `omnibridge-control` | the CLI/GUI contract, shared without inheriting the agent |
//! | `listener`, `mdns`, `state`, `approval` | `omnibridge-runtime` | portable; no platform surface |
//! | `server` | `omnibridge-runtime` + `omnibridge-linux` | the protocol is portable, the endpoint is not |

pub use omnibridge_runtime::{approval, listener, mdns, state};

/// The local control protocol.
pub mod control {
    pub use omnibridge_control::*;
    /// Where the Linux agent puts its control socket.
    pub use omnibridge_linux::control_socket_path;
}

/// The control-endpoint server.
///
/// [`run`] is portable and takes any bound endpoint; [`bind`] is the Linux
/// Unix-domain implementation of one.
///
/// [`run`]: omnibridge_runtime::server::run
/// [`bind`]: omnibridge_linux::bind
pub mod server {
    pub use omnibridge_runtime::server::run;

    /// Binds the Linux control socket.
    ///
    /// Kept as an `anyhow`-returning wrapper because that is the shape the
    /// agent and its tests already use; the typed
    /// [`omnibridge_control::transport::BindError`] is available from
    /// [`omnibridge_linux::bind`] for callers that need to tell "already owned"
    /// from a generic I/O failure.
    pub fn bind(path: &std::path::Path) -> anyhow::Result<omnibridge_linux::UnixControlListener> {
        Ok(omnibridge_linux::bind(path)?)
    }
}
