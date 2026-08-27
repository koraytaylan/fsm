//! Sessions: an id assigned at `initialize`, and required afterwards.
//!
//! Plan 0015 task 7001 fills this in.

/// The header a client carries its session in.
pub const SESSION_HEADER: &str = "mcp-session-id";

/// One client's session.
#[derive(Debug)]
pub struct Session {
    pub id: String,
}

/// Mint a session id.
///
/// Rust's standard library has no random-number API and this workspace has
/// no dependencies, so the construction is a hash over an OS seed read once
/// at start, a per-process counter, the pid and the clock. `7001` states
/// what that is and is not.
pub fn new_session_id() -> String {
    unimplemented!("plan 0015 task 7001")
}
