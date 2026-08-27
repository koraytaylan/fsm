//! The one MCP endpoint, over POST and GET.
//!
//! Plan 0015 tasks 7002 and 7003 fill this in.

use super::request::Request;
use super::response::Response;

/// The default path. `fsm serve --http` may name another.
pub const DEFAULT_PATH: &str = "/mcp";

/// Answer one request against a session.
pub fn handle(_request: &Request) -> Response {
    unimplemented!("plan 0015 task 7002")
}
