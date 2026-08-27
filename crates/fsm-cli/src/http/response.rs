//! Response writing, including the streaming form server-sent events need.
//!
//! Plan 0015 task 6903 fills this in.

use std::io::Write;

/// One complete response: status, headers, body.
#[derive(Debug, Clone)]
pub struct Response {
    pub status: u16,
    pub content_type: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Response {
    /// A JSON response.
    pub fn json(_status: u16, _body: Vec<u8>) -> Self {
        unimplemented!("plan 0015 task 6903")
    }

    /// A plain-text response, for the refusals a parser produces.
    pub fn text(_status: u16, _message: &str) -> Self {
        unimplemented!("plan 0015 task 6903")
    }
}

/// Write one complete response.
pub fn write_response(_out: &mut dyn Write, _response: &Response) -> std::io::Result<()> {
    unimplemented!("plan 0015 task 6903")
}

/// Begin a streaming response: no `Content-Length`, and a flush per event.
pub fn begin_stream(_out: &mut dyn Write) -> std::io::Result<()> {
    unimplemented!("plan 0015 task 6903")
}
