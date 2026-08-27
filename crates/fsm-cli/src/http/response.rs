//! Response writing, including the streaming form server-sent events need.
//!
//! Two shapes only: a complete response with a `Content-Length`, and a
//! stream with none. Chunked encoding is needed for neither, which is why
//! this server does not implement it in either direction.
//!
//! Plan 0015 task 6903.

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
    pub fn json(status: u16, body: Vec<u8>) -> Self {
        Self {
            status,
            content_type: "application/json".to_string(),
            headers: Vec::new(),
            body,
        }
    }

    /// A plain-text response, which is what every refusal is.
    pub fn text(status: u16, message: &str) -> Self {
        Self {
            status,
            content_type: "text/plain; charset=utf-8".to_string(),
            headers: Vec::new(),
            body: message.as_bytes().to_vec(),
        }
    }

    /// One of the statuses this plan uses, with the sentence that goes with
    /// it.
    ///
    /// A stranger learns the status code and nothing else: no path, no
    /// internal message, no backtrace. Everything worth knowing about a
    /// failure is on this server's stderr, where its operator is.
    pub fn error(status: u16) -> Self {
        Self::text(status, reason(status))
    }

    /// Add a header. Names are written as given; this server chooses them.
    pub fn with_header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.to_string(), value.to_string()));
        self
    }
}

/// The reason phrase and the body, which are the same short sentence.
pub fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        202 => "Accepted",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        406 => "Not Acceptable",
        408 => "Request Timeout",
        409 => "Conflict",
        411 => "Length Required",
        413 => "Content Too Large",
        414 => "URI Too Long",
        415 => "Unsupported Media Type",
        429 => "Too Many Requests",
        431 => "Request Header Fields Too Large",
        500 => "Internal Server Error",
        503 => "Service Unavailable",
        505 => "HTTP Version Not Supported",
        _ => "Error",
    }
}

/// Write one complete response.
pub fn write_response(out: &mut dyn Write, response: &Response) -> std::io::Result<()> {
    write!(
        out,
        "HTTP/1.1 {} {}\r\n",
        response.status,
        reason(response.status)
    )?;
    write!(out, "Content-Type: {}\r\n", response.content_type)?;
    // Always, including for an empty body: a reader that has to guess where
    // a response ends is a reader that will guess wrong once.
    write!(out, "Content-Length: {}\r\n", response.body.len())?;
    for (name, value) in &response.headers {
        write!(out, "{name}: {value}\r\n")?;
    }
    out.write_all(b"\r\n")?;
    out.write_all(&response.body)?;
    out.flush()
}

/// Begin a streaming response: the event-stream headers, and no
/// `Content-Length`, because there is no length to state.
pub fn begin_stream(out: &mut dyn Write) -> std::io::Result<()> {
    out.write_all(
        b"HTTP/1.1 200 OK\r\n\
          Content-Type: text/event-stream\r\n\
          Cache-Control: no-cache\r\n\
          Connection: keep-alive\r\n\
          \r\n",
    )?;
    out.flush()
}

/// A stream a `Notifier` can hold.
///
/// Plan 0012's notifier takes a `Box<dyn Write + Send>` and holds a mutex
/// across bytes, newline and flush. Handing it one of these is the whole
/// design: every notification, every progress report and every elicitation
/// request reaches an HTTP client through the same code that reaches a stdio
/// one, and **nothing above the transport changes**. Each line written
/// becomes one event, flushed before the next — an event sitting in a buffer
/// is an event that did not happen, and that is what makes a live surface
/// feel broken.
pub struct StreamWriter<W: Write> {
    out: W,
    next_id: u64,
    pending: Vec<u8>,
}

impl<W: Write> StreamWriter<W> {
    /// A stream that numbers its events from `first_id`.
    pub fn new(out: W, first_id: u64) -> Self {
        Self {
            out,
            next_id: first_id,
            pending: Vec::new(),
        }
    }

    /// The id the next event will carry.
    pub fn next_id(&self) -> u64 {
        self.next_id
    }

    /// A comment line, which keeps an idle connection alive without being an
    /// event. Written when the caller asks and never otherwise: a writer with
    /// a timer inside it is a writer with a clock inside it.
    pub fn keepalive(&mut self) -> std::io::Result<()> {
        self.out.write_all(b": keepalive\n\n")?;
        self.out.flush()
    }

    /// One event, framed and flushed.
    pub fn event(&mut self, data: &[u8]) -> std::io::Result<()> {
        writeln!(self.out, "id: {}", self.next_id)?;
        self.out.write_all(b"data: ")?;
        self.out.write_all(data)?;
        self.out.write_all(b"\n\n")?;
        self.next_id = self.next_id.saturating_add(1);
        self.out.flush()
    }
}

impl<W: Write> Write for StreamWriter<W> {
    /// Buffer until a newline, then send what precedes it as one event.
    ///
    /// The protocol above writes one whole message per line and flushes; the
    /// canonical encoder guarantees no message contains a bare newline, so a
    /// line here is exactly one message and one event.
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        for byte in buf {
            if *byte == b'\n' {
                let data = std::mem::take(&mut self.pending);
                if !data.is_empty() {
                    self.event(&data)?;
                }
            } else {
                self.pending.push(*byte);
            }
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.out.flush()
    }
}
