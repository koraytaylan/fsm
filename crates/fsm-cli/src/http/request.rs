//! Request parsing, with every bound a stranger can reach stated here.
//!
//! Plan 0015 task 6902 fills this in.

use std::io::BufRead;

/// The request line's ceiling. Longer is `414`.
pub const MAX_REQUEST_LINE: usize = 8 * 1024;
/// How many headers one request may carry. More is `431`.
pub const MAX_HEADERS: usize = 64;
/// One header's ceiling, and the total across all of them. More is `431`.
pub const MAX_HEADER_BYTES: usize = 8 * 1024;
pub const MAX_HEADERS_BYTES: usize = 32 * 1024;
/// The body ceiling, matching `JsonLimits::DEFAULT`. More is `413`.
pub const MAX_BODY_BYTES: usize = 16 * 1024 * 1024;

/// One parsed request.
#[derive(Debug, Clone, Default)]
pub struct Request {
    pub method: String,
    pub path: String,
    pub query: String,
    /// Header names lowercased; values kept as sent.
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Request {
    /// One header by name, compared ASCII-case-insensitively.
    pub fn header(&self, _name: &str) -> Option<&str> {
        unimplemented!("plan 0015 task 6902")
    }
}

/// Read and parse one request, or say which refusal it earned.
pub fn read_request(_input: &mut dyn BufRead) -> Result<Request, Refusal> {
    unimplemented!("plan 0015 task 6902")
}

/// A request this server will not parse, and the status that says so.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    pub status: u16,
    pub message: String,
}
