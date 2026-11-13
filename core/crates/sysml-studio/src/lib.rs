//! An editable canvas over a SysML v2 model.
//!
//! The design constraint this crate is built around: **the picture is a projection, the source is
//! the truth.** The canvas does not hold a model of its own. It holds two things — an arrangement
//! of boxes (a sidecar, no semantic content) and a set of operations that become text edits in the
//! `.sysml` files a person already maintains. Every edit is verified by reparsing before it is
//! written, so a refused edit leaves the files byte-identical.
//!
//! That is why the same geometry function serves the SVG export and the canvas: two layout passes
//! over one model would be two pictures, and the drift would show up as "the editor moved my
//! boxes".
//!
//! The server is single-user and local: it binds loopback, serves one project, and has no
//! authentication because it is not reachable from anywhere else.

pub mod conflict;
pub mod document;
pub mod edit;
pub mod layout;
pub mod next;
mod server;
pub mod translate;

pub use conflict::{Conflict, Digest, Fingerprint};
pub use document::{CanvasDocument, Session};
pub use layout::{Layout, Placement};

/// Run the studio for one project until interrupted.
///
/// # Errors
///
/// Returns a message when the port cannot be bound or the model cannot be opened. The server runs
/// until it is stopped, so an `Err` here always comes from start-up.
pub fn serve(paths: Vec<std::path::PathBuf>, port: u16) -> Result<(), String> {
    server::serve(paths, port)
}

/// Run the studio on a listener the caller already bound, until interrupted.
///
/// For a caller that binds port 0 and needs to know which port it got before the server starts.
///
/// # Errors
///
/// Returns a message when the model cannot be opened. The listener is already bound by the
/// caller, so binding is not one of the failure modes here.
pub fn serve_on(
    listener: std::net::TcpListener,
    paths: Vec<std::path::PathBuf>,
) -> Result<(), String> {
    server::serve_on(listener, paths)
}
