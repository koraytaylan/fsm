//! Library target for `fsm-cli` so integration tests (and later the fuzz
//! side-crate) can import `fsm_cli::mcp::serve::serve`.

#![forbid(unsafe_code)]
#![allow(
    clippy::result_large_err,
    clippy::collapsible_if,
    clippy::collapsible_match
)]

#[cfg(target_os = "linux")]
pub mod local_control;

pub mod args;
pub mod cli;
pub mod mcp;
pub mod render;

#[cfg(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
mod native_error;

// The durable store lives in `fsm-store` so embedders can depend on it without
// depending on this binary crate. Re-exported here so `crate::store::…` paths
// and existing `fsm_cli::store::…` importers keep resolving.
pub mod http;

pub use fsm_store::{clock, journal_io, snapshot, store};

#[cfg(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
pub mod standalone;
