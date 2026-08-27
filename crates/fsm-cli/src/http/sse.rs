//! Server-sent events: how a server that speaks first does it over HTTP.
//!
//! Plan 0012's notifications and plan 0013's elicitation requests reach a
//! client through this instead of stdout, and nothing above the transport
//! changes.
//!
//! Plan 0015 tasks 7003 and 7004 fill this in.

use std::io::Write;

/// One event, with the id a client resumes from.
pub fn write_event(_out: &mut dyn Write, _id: u64, _data: &[u8]) -> std::io::Result<()> {
    unimplemented!("plan 0015 task 7003")
}
