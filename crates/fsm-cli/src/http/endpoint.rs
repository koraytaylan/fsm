//! The one MCP endpoint, over POST.
//!
//! Everything above the transport is plan 0012's and 0013's code, unchanged:
//! this routes a request to `serve::handle_request` and decides only how the
//! answer travels — a JSON body, or an event stream when the handling may
//! speak before it answers.
//!
//! Plan 0015 task 7002.

use std::collections::BTreeMap;
use std::io::Write;
use std::sync::{Condvar, Mutex};

use fsm_core::json::{JsonLimits, Value, parse};

use super::request::Request;
use super::response::{Response, StreamWriter, begin_stream, write_response};
use super::session::{SESSION_HEADER, SessionError, Sessions, VERSION_HEADER};
use crate::clock::Clock;
use crate::mcp::jsonrpc::{Incoming, WireError, parse_line};
use crate::mcp::methods::handle_request;
use crate::mcp::notify::{Notifier, SessionIo, SharedSink};
use crate::mcp::serve::{Live, negotiate};
use crate::store::Store;

/// The default path. `fsm serve --http` may name another.
pub const DEFAULT_PATH: &str = "/mcp";

/// The methods this endpoint answers, for the `Allow` header a `405` carries.
pub const ALLOWED_METHODS: &str = "POST, GET, DELETE";

/// One server's endpoint: the store every session shares, the sessions
/// themselves, and the protocol state each of them keeps.
pub struct Endpoint {
    pub path: String,
    /// The single writer. One process holds the lock and every client talks
    /// to that process, which is what turns the store's oldest constraint
    /// into the thing that makes many clients safe.
    store: Mutex<Option<Store>>,
    sessions: Sessions,
    lives: Mutex<BTreeMap<String, Live>>,
    /// Inbound responses, per session, waiting for whoever asked.
    mailboxes: Mutex<BTreeMap<String, std::sync::Arc<Mailbox>>>,
    mode_note: &'static str,
}

/// What one POST produced.
#[derive(Debug)]
pub enum Answer {
    /// A complete HTTP response.
    Complete(Response),
    /// Nothing to say: a notification or a response was accepted.
    Accepted,
}

impl Endpoint {
    pub fn new(path: &str, store: Option<Store>, mode_note: &'static str) -> Self {
        Self {
            path: path.to_string(),
            store: Mutex::new(store),
            sessions: Sessions::default(),
            lives: Mutex::new(BTreeMap::new()),
            mailboxes: Mutex::new(BTreeMap::new()),
            mode_note,
        }
    }

    pub fn sessions(&self) -> &Sessions {
        &self.sessions
    }

    /// Route one request by path and method, writing the whole response.
    pub fn serve(
        &self,
        request: &Request,
        clock: &mut dyn Clock,
        out: &mut dyn Write,
    ) -> std::io::Result<()> {
        if request.path != self.path {
            return write_response(out, &Response::error(404));
        }
        match request.method.as_str() {
            "POST" => self.post(request, clock, out),
            // `7003` fills the stream in; until then the method is routed
            // and answered rather than silently unhandled.
            "GET" => write_response(
                out,
                &Response::error(405).with_header("Allow", ALLOWED_METHODS),
            ),
            "DELETE" => {
                let id = request.header(SESSION_HEADER).unwrap_or("");
                if self.sessions.close(id) {
                    self.lives.lock_safe().remove(id);
                    self.mailboxes.lock_safe().remove(id);
                    write_response(out, &Response::text(200, "session ended"))
                } else {
                    write_response(out, &Response::error(404))
                }
            }
            _ => write_response(
                out,
                &Response::error(405).with_header("Allow", ALLOWED_METHODS),
            ),
        }
    }

    /// One JSON-RPC message in a POST body.
    fn post(
        &self,
        request: &Request,
        clock: &mut dyn Clock,
        out: &mut dyn Write,
    ) -> std::io::Result<()> {
        let body = String::from_utf8_lossy(&request.body).to_string();
        let message = match parse_line(body.trim()) {
            Ok(message) => message,
            Err(WireError::Batch) => {
                // The same refusal stdio gives, and the same reason: the
                // `2025-06-18` revision removed batching.
                return write_response(
                    out,
                    &Response::json(
                        200,
                        canon(&crate::mcp::serve::rpc_error(
                            Value::Null,
                            crate::mcp::jsonrpc::INVALID_REQUEST,
                            "batch requests are not supported",
                        )),
                    ),
                );
            }
            Err(WireError::Parse(_)) => {
                // A malformed JSON-RPC message is a protocol-level error,
                // not an HTTP-level one: the transport delivered exactly
                // what was sent.
                return write_response(
                    out,
                    &Response::json(
                        200,
                        canon(&crate::mcp::serve::rpc_error(
                            Value::Null,
                            crate::mcp::jsonrpc::PARSE_ERROR,
                            "parse error",
                        )),
                    ),
                );
            }
            Err(WireError::Invalid) => {
                return write_response(
                    out,
                    &Response::json(
                        200,
                        canon(&crate::mcp::serve::rpc_error(
                            Value::Null,
                            crate::mcp::jsonrpc::INVALID_REQUEST,
                            "invalid request",
                        )),
                    ),
                );
            }
        };

        match message {
            // Nothing to say. A body here would invite a client to parse
            // one.
            Incoming::Notification { method, params } => {
                self.notification(request, &method, params, clock);
                write_response(out, &Response::text(202, ""))
            }
            Incoming::Response { id, result, error } => {
                self.deliver_response(request, id, result, error);
                write_response(out, &Response::text(202, ""))
            }
            Incoming::Request { id, method, params } => {
                self.request(request, clock, out, id, &method, params)
            }
        }
    }

    /// A request, which is the only shape with an answer.
    fn request(
        &self,
        http: &Request,
        clock: &mut dyn Clock,
        out: &mut dyn Write,
        id: Value,
        method: &str,
        params: Option<Value>,
    ) -> std::io::Result<()> {
        let now_ms = clock.now_ms();
        // `initialize` mints the session; everything else must name one.
        let session_id = if method == "initialize" {
            let version = negotiate(
                params
                    .as_ref()
                    .and_then(|p| p.get("protocolVersion"))
                    .and_then(Value::as_str),
            );
            match self.sessions.open(version, now_ms) {
                Ok(id) => id,
                Err(error) => return write_response(out, &Response::error(error.status())),
            }
        } else {
            match self.sessions.touch(
                http.header(SESSION_HEADER),
                http.header(VERSION_HEADER),
                now_ms,
            ) {
                Ok(id) => id,
                Err(SessionError::VersionMismatch) => {
                    return write_response(
                        out,
                        &Response::text(400, "MCP-Protocol-Version is not the negotiated version"),
                    );
                }
                Err(error) => return write_response(out, &Response::error(error.status())),
            }
        };

        // Whether the answer needs a stream: a call that reports progress or
        // asks the client something speaks before it answers. Decided from
        // the request rather than guessed from a method name.
        let streaming = speaks_first(method, params.as_ref());
        if streaming && !accepts_events(http) {
            // A stream the client cannot read is worse than a refusal.
            return write_response(out, &Response::error(406));
        }

        let sink = SharedSink::new();
        let notifier = Notifier::new(Box::new(sink.writer()));
        let mailbox = self.mailbox(&session_id);
        let mut reader = MailboxReader::new(std::sync::Arc::clone(&mailbox));
        let io = std::cell::RefCell::new(SessionIo::new(&notifier, &mut reader));

        {
            let mut lives = self.lives.lock_safe();
            let live = lives.entry(session_id.clone()).or_default();
            let mut initialized = true;
            let mut store = self.store.lock_safe();
            let _ = handle_request(
                &notifier,
                store.as_mut(),
                clock,
                &mut initialized,
                live,
                id,
                method,
                params,
                self.mode_note,
                Some(&io),
            );
        }

        let written = sink.text();
        if streaming {
            begin_stream(out)?;
            let mut stream = StreamWriter::new(&mut *out, 1);
            for line in written.lines() {
                stream.event(line.as_bytes())?;
            }
            return Ok(());
        }
        // The last line is the response; anything before it was written by a
        // handler that had nothing to stream to.
        let response = written.lines().next_back().unwrap_or("{}").to_string();
        let mut answer = Response::json(200, response.into_bytes());
        if method == "initialize" {
            answer = answer.with_header("Mcp-Session-Id", &session_id);
        }
        write_response(out, &answer)
    }

    /// A notification, which is handled and not answered.
    fn notification(
        &self,
        http: &Request,
        method: &str,
        params: Option<Value>,
        clock: &mut dyn Clock,
    ) {
        let Ok(session_id) = self.sessions.touch(
            http.header(SESSION_HEADER),
            http.header(VERSION_HEADER),
            clock.now_ms(),
        ) else {
            return;
        };
        if method == "notifications/cancelled"
            && let Some(requested) = params.as_ref().and_then(|p| p.get("requestId"))
        {
            let mut lives = self.lives.lock_safe();
            let live = lives.entry(session_id).or_default();
            live.cancellations.cancel(requested);
        }
    }

    /// A client answering something this server asked.
    ///
    /// Over stdio that answer arrived as a line on the same stream; here it
    /// arrives as a POST. That is a routing difference and not a protocol
    /// one, so it is put where the waiting `request_and_await` is reading.
    fn deliver_response(
        &self,
        http: &Request,
        id: Value,
        result: Option<Value>,
        error: Option<Value>,
    ) {
        let Some(session_id) = http.header(SESSION_HEADER) else {
            return;
        };
        // Only into this session's mailbox: one client's answer must never
        // complete another client's question.
        let Some(mailbox) = self.mailboxes.lock_safe().get(session_id).cloned() else {
            return;
        };
        let mut message = BTreeMap::from([
            ("jsonrpc".to_string(), Value::Str("2.0".into())),
            ("id".to_string(), id),
        ]);
        if let Some(result) = result {
            message.insert("result".to_string(), result);
        }
        if let Some(error) = error {
            message.insert("error".to_string(), error);
        }
        mailbox.post(Value::Obj(message));
    }

    fn mailbox(&self, session_id: &str) -> std::sync::Arc<Mailbox> {
        let mut boxes = self.mailboxes.lock_safe();
        boxes
            .entry(session_id.to_string())
            .or_insert_with(|| std::sync::Arc::new(Mailbox::default()))
            .clone()
    }
}

/// Whether handling this request may produce server-initiated messages
/// before its answer.
fn speaks_first(method: &str, params: Option<&Value>) -> bool {
    if method != "tools/call" {
        return false;
    }
    let name = params
        .and_then(|p| p.get("name"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let progress = params
        .and_then(|p| p.get("_meta"))
        .and_then(|meta| meta.get("progressToken"))
        .is_some();
    // An elicitation asks the client something; a progress token asks to be
    // told where the call has got to. Everything else answers once, and a
    // stream for one message is overhead a client has to unwrap.
    name == "instance_elicit" || progress
}

/// Whether a client said it can read an event stream.
fn accepts_events(request: &Request) -> bool {
    request
        .header("accept")
        .map(|accept| accept.contains("text/event-stream") || accept.contains("*/*"))
        // A client that said nothing is not a client that refused.
        .unwrap_or(true)
}

fn canon(value: &Value) -> Vec<u8> {
    fsm_core::canon::canon_bytes(value)
}

/// Inbound responses for one session, and whoever is waiting for them.
#[derive(Default)]
pub struct Mailbox {
    waiting: Mutex<Vec<Value>>,
    arrived: Condvar,
}

impl Mailbox {
    /// Put an answer in, waking whoever is waiting.
    pub fn post(&self, message: Value) {
        self.waiting
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(message);
        self.arrived.notify_all();
    }

    /// Take the next answer, waiting up to `timeout` for one.
    pub fn take(&self, timeout: std::time::Duration) -> Option<Value> {
        let mut waiting = self
            .waiting
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if waiting.is_empty() {
            let (next, _) = self
                .arrived
                .wait_timeout(waiting, timeout)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            waiting = next;
        }
        if waiting.is_empty() {
            None
        } else {
            Some(waiting.remove(0))
        }
    }
}

/// A reader over a mailbox, so plan 0013's `request_and_await` reads an
/// HTTP client's answer exactly as it reads a stdio one.
pub struct MailboxReader {
    mailbox: std::sync::Arc<Mailbox>,
    pending: Vec<u8>,
    at: usize,
}

impl MailboxReader {
    pub fn new(mailbox: std::sync::Arc<Mailbox>) -> Self {
        Self {
            mailbox,
            pending: Vec::new(),
            at: 0,
        }
    }
}

impl std::io::Read for MailboxReader {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        let available = std::io::BufRead::fill_buf(self)?;
        let n = available.len().min(out.len());
        out[..n].copy_from_slice(&available[..n]);
        std::io::BufRead::consume(self, n);
        Ok(n)
    }
}

impl std::io::BufRead for MailboxReader {
    fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
        if self.at == self.pending.len() {
            // One poll interval, not the whole elicitation timeout: the
            // caller's own deadline is what bounds the wait, and this loop
            // is what lets it see it.
            match self.mailbox.take(std::time::Duration::from_millis(50)) {
                Some(message) => {
                    self.pending = canon(&message);
                    self.pending.push(b'\n');
                    self.at = 0;
                }
                // Nothing arrived: an empty read, which the caller reads as
                // end of input and treats as the client having gone.
                None => return Ok(&[]),
            }
        }
        Ok(&self.pending[self.at..])
    }

    fn consume(&mut self, amount: usize) {
        self.at = (self.at + amount).min(self.pending.len());
    }
}

/// A lock that hands back the data even when a holder panicked, matching how
/// every other lock in this workspace behaves.
trait LockSafe<T> {
    fn lock_safe(&self) -> std::sync::MutexGuard<'_, T>;
}

impl<T> LockSafe<T> for Mutex<T> {
    fn lock_safe(&self) -> std::sync::MutexGuard<'_, T> {
        self.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// Parse a JSON body, for callers that have one.
pub fn json_body(body: &[u8]) -> Option<Value> {
    parse(body, &JsonLimits::DEFAULT).ok()
}
