//! Library target for `fsm-cli` so integration tests (and later the fuzz
//! side-crate) can import `fsm_cli::mcp::serve::serve`.

#![forbid(unsafe_code)]
#![allow(
    clippy::result_large_err,
    clippy::collapsible_if,
    clippy::collapsible_match
)]

pub mod args;
pub mod cli;
pub mod clock;
pub mod journal_io;
pub mod mcp;
pub mod render;
pub mod store;
