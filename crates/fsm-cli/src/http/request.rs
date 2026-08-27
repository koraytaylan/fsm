//! Request parsing, with every bound a stranger can reach stated here.
//!
//! A hand-rolled parser on a socket is where a zero-dependency project earns
//! or loses its safety claim. So: every length is checked before it is
//! trusted, no buffer is sized from a number a client supplied until that
//! number has been compared to its cap, nothing is `unwrap`ed, and a request
//! this parser does not understand is **refused rather than repaired** — a
//! parser that repairs input is a parser two parties can disagree about, and
//! disagreement between two parsers is what request smuggling is.
//!
//! Plan 0015 task 6902.

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
    pub fn header(&self, name: &str) -> Option<&str> {
        let wanted = name.to_ascii_lowercase();
        self.headers
            .iter()
            .find(|(header, _)| *header == wanted)
            .map(|(_, value)| value.as_str())
    }
}

/// A request this server will not parse, and the status that says so.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    pub status: u16,
    pub message: String,
}

impl Refusal {
    fn new(status: u16, message: &str) -> Self {
        Self {
            status,
            message: message.to_string(),
        }
    }
}

/// A request's head: everything before the body.
///
/// Separate from the body because the checks that refuse a request —
/// `Origin`, then authentication — are decided entirely from the head, and a
/// stranger's rejected request should cost this server one header block
/// rather than sixteen megabytes.
#[derive(Debug, Clone, Default)]
pub struct Head {
    pub method: String,
    pub path: String,
    pub query: String,
    pub headers: Vec<(String, String)>,
    /// How many bytes of body the client said it is sending.
    pub content_length: usize,
}

impl Head {
    /// One header by name, compared ASCII-case-insensitively.
    pub fn header(&self, name: &str) -> Option<&str> {
        let wanted = name.to_ascii_lowercase();
        self.headers
            .iter()
            .find(|(header, _)| *header == wanted)
            .map(|(_, value)| value.as_str())
    }
}

/// Read and parse one request, or say which refusal it earned.
///
/// The socket's own read timeout is what bounds a client that stops
/// mid-request: a short read becomes `408`, never a wait without end.
pub fn read_request(input: &mut dyn BufRead) -> Result<Request, Refusal> {
    let head = read_head(input)?;
    let body = read_body(input, &head)?;
    Ok(Request {
        method: head.method,
        path: head.path,
        query: head.query,
        headers: head.headers,
        body,
    })
}

/// The body a head declared, refused if it is over the limit or short.
pub fn read_body(input: &mut dyn BufRead, head: &Head) -> Result<Vec<u8>, Refusal> {
    // Checked before the buffer exists: a length a client supplied is a
    // claim, not an allocation.
    if head.content_length > MAX_BODY_BYTES {
        return Err(Refusal::new(413, "body over the limit"));
    }
    let mut body = vec![0u8; head.content_length];
    if head.content_length > 0 && input.read_exact(&mut body).is_err() {
        // Short, or the socket went quiet. Either way the request never
        // arrived.
        return Err(Refusal::new(408, "the body did not arrive"));
    }
    Ok(body)
}

/// Read the request line and the headers, and nothing after them.
pub fn read_head(input: &mut dyn BufRead) -> Result<Head, Refusal> {
    let line = read_line(input, MAX_REQUEST_LINE, 414)?;
    if line.is_empty() {
        return Err(Refusal::new(400, "empty request line"));
    }
    let mut parts = line.split(' ');
    let (Some(method), Some(target), Some(version)) = (parts.next(), parts.next(), parts.next())
    else {
        return Err(Refusal::new(400, "malformed request line"));
    };
    if parts.next().is_some() {
        return Err(Refusal::new(400, "malformed request line"));
    }
    if !version.starts_with("HTTP/1.") {
        return Err(Refusal::new(505, "this server speaks HTTP/1.1"));
    }
    if method.is_empty() || !method.bytes().all(is_token_byte) {
        return Err(Refusal::new(400, "malformed method"));
    }
    if !target.starts_with('/') {
        return Err(Refusal::new(
            400,
            "the request target must be an origin-form path",
        ));
    }
    let (path, query) = match target.split_once('?') {
        Some((path, query)) => (path, query),
        None => (target, ""),
    };

    let headers = read_headers(input)?;
    let count = |name: &str| headers.iter().filter(|(header, _)| header == name).count();
    // Two lengths, or a length and an encoding, are two answers to one
    // question. Reconciling them is how smuggling bugs happen, and no
    // legitimate client sends either shape here.
    if count("content-length") > 1 {
        return Err(Refusal::new(400, "duplicate Content-Length"));
    }
    if count("host") > 1 {
        return Err(Refusal::new(400, "duplicate Host"));
    }
    let has_length = count("content-length") == 1;
    let has_encoding = count("transfer-encoding") == 1;
    if has_length && has_encoding {
        return Err(Refusal::new(
            400,
            "Content-Length with Transfer-Encoding is a request-smuggling shape and is refused",
        ));
    }
    if has_encoding {
        return Err(Refusal::new(
            411,
            "this server reads Content-Length bodies only; chunked requests are not accepted",
        ));
    }

    let declared = match headers
        .iter()
        .find(|(header, _)| header == "content-length")
    {
        None => 0,
        Some((_, value)) => match value.trim().parse::<usize>() {
            Ok(length) => length,
            Err(_) => return Err(Refusal::new(400, "malformed Content-Length")),
        },
    };
    Ok(Head {
        method: method.to_string(),
        path: path.to_string(),
        query: query.to_string(),
        headers,
        content_length: declared,
    })
}

/// Read the header block: names lowercased, values verbatim.
fn read_headers(input: &mut dyn BufRead) -> Result<Vec<(String, String)>, Refusal> {
    let mut headers: Vec<(String, String)> = Vec::new();
    let mut total = 0usize;
    loop {
        let line = read_line(input, MAX_HEADER_BYTES, 431)?;
        if line.is_empty() {
            return Ok(headers);
        }
        total = total.saturating_add(line.len());
        if total > MAX_HEADERS_BYTES {
            return Err(Refusal::new(431, "headers over the limit"));
        }
        if headers.len() >= MAX_HEADERS {
            return Err(Refusal::new(431, "too many headers"));
        }
        // Obsolete line folding: refused rather than un-folded, which is
        // what RFC 9112 recommends and what keeps two parsers from
        // disagreeing about where a value ends.
        if line.starts_with(' ') || line.starts_with('\t') {
            return Err(Refusal::new(400, "obsolete header line folding is refused"));
        }
        let Some((name, value)) = line.split_once(':') else {
            return Err(Refusal::new(400, "a header without a colon"));
        };
        if name.is_empty() || !name.bytes().all(is_token_byte) {
            return Err(Refusal::new(400, "malformed header name"));
        }
        let value = value.trim_matches([' ', '\t']);
        if !value.bytes().all(is_value_byte) {
            return Err(Refusal::new(400, "malformed header value"));
        }
        headers.push((name.to_ascii_lowercase(), value.to_string()));
    }
}

/// One CRLF-terminated line, refused at `cap` with `status`.
///
/// Bytes are taken one at a time rather than through `read_until`, because
/// `read_until` grows a buffer as far as the input goes and the cap is the
/// whole point: a client must not be able to make this allocate more than
/// the limit it is about to be refused for.
fn read_line(input: &mut dyn BufRead, cap: usize, status: u16) -> Result<String, Refusal> {
    let mut line: Vec<u8> = Vec::new();
    loop {
        let mut byte = [0u8; 1];
        match input.read_exact(&mut byte) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof && line.is_empty() => {
                return Err(Refusal::new(400, "the client closed the connection"));
            }
            Err(_) => return Err(Refusal::new(408, "the request did not arrive")),
        }
        match byte[0] {
            b'\n' => {
                if line.last() == Some(&b'\r') {
                    line.pop();
                }
                return String::from_utf8(line)
                    .map_err(|_| Refusal::new(400, "a request line that is not UTF-8"));
            }
            other => {
                if line.len() >= cap {
                    return Err(Refusal::new(status, "over the limit"));
                }
                line.push(other);
            }
        }
    }
}

/// The token characters RFC 9110 allows in a method or a header name.
fn is_token_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric()
        || matches!(
            byte,
            b'!' | b'#'
                | b'$'
                | b'%'
                | b'&'
                | b'\''
                | b'*'
                | b'+'
                | b'-'
                | b'.'
                | b'^'
                | b'_'
                | b'`'
                | b'|'
                | b'~'
        )
}

/// What a header value may contain: visible ASCII, space and tab. A control
/// character in a value is refused rather than stripped.
fn is_value_byte(byte: u8) -> bool {
    byte == b'\t' || (0x20..=0x7E).contains(&byte) || byte >= 0x80
}
