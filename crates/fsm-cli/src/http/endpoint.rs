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
use super::security::{Policy, origin_allowed, presented_token, token_matches};
use super::session::{SESSION_HEADER, SessionError, Sessions, VERSION_HEADER};
use super::sse::{Stream, write_event};
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
    /// into the thing that makes many clients safe. Every call in this file
    /// goes through it — a guard nothing calls guards nothing.
    store: super::writer::SerializedWriter,
    sessions: Sessions,
    lives: Mutex<BTreeMap<String, Live>>,
    /// Inbound responses, per session, waiting for whoever asked.
    mailboxes: Mutex<BTreeMap<String, std::sync::Arc<Mailbox>>>,
    /// Each session's event stream: what it has sent, and whether anybody is
    /// reading it.
    streams: Mutex<BTreeMap<String, std::sync::Arc<Stream>>>,
    /// What this server will answer, and from whom.
    policy: Option<Policy>,
    mode_note: &'static str,
    /// The server's stop flag, when this endpoint is being served by one.
    ///
    /// Its presence is also what tells `stream` whether it may hold a
    /// connection: an endpoint a caller drives synchronously — every test in
    /// `http_sse.rs`, and anything else that hands it one request at a time —
    /// has no thread to block and no way to be told to stop, so it is handed
    /// its headers and left alone. `watch::ByHand` splits the change feed the
    /// same way and for the same reason.
    stop: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
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
            store: super::writer::SerializedWriter::new(store),
            sessions: Sessions::default(),
            lives: Mutex::new(BTreeMap::new()),
            mailboxes: Mutex::new(BTreeMap::new()),
            streams: Mutex::new(BTreeMap::new()),
            policy: None,
            mode_note,
            stop: None,
        }
    }

    /// The same endpoint, told which flag ends its streams.
    ///
    /// Pass the flag the server itself was given, so a stop reaches the
    /// connection threads parked in `stream` as well as the accept loop.
    pub fn with_stop(mut self, stop: std::sync::Arc<std::sync::atomic::AtomicBool>) -> Self {
        self.stop = Some(stop);
        self
    }

    /// The same endpoint, with a posture to enforce.
    pub fn with_policy(mut self, policy: Policy) -> Self {
        self.path = policy.path.clone();
        self.policy = Some(policy);
        self
    }

    pub fn sessions(&self) -> &Sessions {
        &self.sessions
    }

    /// Whether this request may be answered at all, and what to say if not.
    ///
    /// Ordered deliberately: `Origin` first, because it is the
    /// DNS-rebinding defence and it costs one string comparison; then the
    /// token. A request failing both is told about the origin, which is the
    /// one it can fix without a credential.
    pub fn admits(&self, method: &str, headers: &[(String, String)]) -> Option<Response> {
        let Some(policy) = &self.policy else {
            return None;
        };
        let header = |name: &str| {
            headers
                .iter()
                .find(|(header, _)| header == name)
                .map(|(_, value)| value.as_str())
        };
        let _ = method;
        if !origin_allowed(header("origin"), &policy.origins) {
            return Some(Response::error(403));
        }
        if let Some(expected) = &policy.token {
            let presented = presented_token(header("authorization"));
            if !presented.is_some_and(|token| token_matches(token, expected)) {
                // No detail: not "wrong token", not "no token", not a length
                // hint. A stranger learns that credentials are required.
                return Some(Response::error(401).with_header("WWW-Authenticate", "Bearer"));
            }
        }
        None
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
            "GET" => self.stream(request, clock, out),
            "DELETE" => {
                let id = request.header(SESSION_HEADER).unwrap_or("");
                if self.sessions.close(id) {
                    self.lives.lock_safe().remove(id);
                    self.mailboxes.lock_safe().remove(id);
                    // The stream closes with the session, and says nothing
                    // on its way out: there is nothing to say and the client
                    // may already be gone.
                    if let Some(stream) = self.streams.lock_safe().remove(id) {
                        stream.release();
                        // A disconnected client's buffer must not outlive
                        // the session it belonged to.
                        stream.forget();
                    }
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
        // Anything that outlives this request — the change feed a
        // `resources/subscribe` starts — writes into the session's stream
        // instead, because `sink` is this POST's body and stops being read
        // the moment it is answered.
        let feed_out = super::sse::recorder_for(self.stream_state(&session_id));
        let mailbox = self.mailbox(&session_id);
        let mut reader = MailboxReader::new(std::sync::Arc::clone(&mailbox));
        let io = std::cell::RefCell::new(SessionIo::new(&notifier, &mut reader));

        {
            let mut lives = self.lives.lock_safe();
            let live = lives.entry(session_id.clone()).or_default();
            let mut initialized = true;
            // Every session's call, through one lock, for the whole call.
            self.store.with_store(|store| {
                let _ = handle_request(
                    &notifier,
                    store,
                    clock,
                    &mut initialized,
                    live,
                    id,
                    method,
                    params,
                    self.mode_note,
                    Some(&io),
                    Some(&feed_out),
                );
            });
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

    /// The stream state for one session, created on first use.
    pub fn stream_state(&self, session_id: &str) -> std::sync::Arc<Stream> {
        let mut streams = self.streams.lock_safe();
        streams
            .entry(session_id.to_string())
            .or_insert_with(|| std::sync::Arc::new(Stream::default()))
            .clone()
    }

    /// GET: the session's one event stream.
    ///
    /// Everything a server says unprompted travels here — plan 0012's
    /// notifications, plan 0013's questions — through a notifier this file
    /// hands a socket instead of a pipe.
    fn stream(
        &self,
        request: &Request,
        clock: &mut dyn Clock,
        out: &mut dyn Write,
    ) -> std::io::Result<()> {
        // A stream a client did not ask for is not a stream to open.
        if !request
            .header("accept")
            .is_some_and(|accept| accept.contains("text/event-stream"))
        {
            return write_response(out, &Response::error(406));
        }
        let session_id = match self.sessions.touch(
            request.header(SESSION_HEADER),
            request.header(VERSION_HEADER),
            clock.now_ms(),
        ) {
            Ok(id) => id,
            Err(error) => return write_response(out, &Response::error(error.status())),
        };
        let stream = self.stream_state(&session_id);
        // One stream per session. Two would split notification ordering with
        // nothing to reassemble it; a client that wants two opens two
        // sessions, which costs nothing.
        if !stream.claim() {
            return write_response(out, &Response::error(409));
        }
        // A client that comes back after a disconnect resumes from where it
        // stopped. The two ways that can fail are different failures and get
        // different answers: an id whose events were evicted is `409` and
        // "re-initialize", because resuming from the oldest retained event
        // would hand the client a gap it cannot detect; an id this session
        // never issued is `400`, because that is the client's mistake rather
        // than time passing.
        let resume = match request
            .header("last-event-id")
            .and_then(|id| id.parse::<u64>().ok())
        {
            None => Vec::new(),
            Some(last) => match stream.resume_after(last) {
                Ok(missed) => missed,
                Err(error) => {
                    stream.release();
                    return write_response(out, &Response::text(error.status(), error.message()));
                }
            },
        };
        begin_stream(out)?;
        let mut last_id = stream.next_id();
        for event in resume {
            // The bytes that were sent, not bytes regenerated now.
            write_event(out, event.id, &event.data)?;
            last_id = event.id;
        }
        // A caller driving this endpoint by hand has its headers; it owns no
        // thread this could park.
        let Some(stop) = self.stop.clone() else {
            return Ok(());
        };
        self.deliver(&session_id, &stream, last_id, &stop, out);
        // The slot goes back so the same client can reconnect. The session,
        // and its subscriptions, are deliberately untouched.
        stream.release();
        Ok(())
    }

    /// Hold one session's stream open and write what the server says on it.
    ///
    /// Everything that speaks unprompted — the change feed, progress,
    /// elicitation — records into `Stream`, which assigns each event its id.
    /// This is the other half: the loop that carries those recorded events to
    /// the socket. Without it the server produces notifications nobody can
    /// read, which is what `resources/subscribe` did over HTTP before.
    ///
    /// It ends when the client goes away, the session does, or the server is
    /// stopped — the last of those is why an endpoint without a stop flag
    /// never enters here.
    fn deliver(
        &self,
        session_id: &str,
        stream: &std::sync::Arc<Stream>,
        from_id: u64,
        stop: &std::sync::atomic::AtomicBool,
        out: &mut dyn Write,
    ) {
        use std::sync::atomic::Ordering;
        // Short enough that a stop is honoured promptly, long enough that an
        // idle stream costs one lock and one comparison four times a second.
        const TICK: std::time::Duration = std::time::Duration::from_millis(250);
        let mut last_id = from_id;
        let mut silent_for = std::time::Duration::ZERO;
        while !stop.load(Ordering::Relaxed) {
            // Gone means expired or `DELETE`d; either way there is nothing
            // left to speak for.
            if self.sessions.with(session_id, |_| ()).is_none() {
                return;
            }
            let (events, _gap) = stream.replay_after(last_id);
            for event in events {
                if write_event(out, event.id, &event.data).is_err() {
                    return;
                }
                last_id = event.id;
                silent_for = std::time::Duration::ZERO;
            }
            std::thread::sleep(TICK);
            silent_for += TICK;
            // A proxy in the middle must not decide a quiet stream is dead.
            if silent_for >= std::time::Duration::from_millis(super::sse::KEEPALIVE_MS) {
                if out.write_all(b": keepalive\n\n").is_err() || out.flush().is_err() {
                    return;
                }
                silent_for = std::time::Duration::ZERO;
            }
        }
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

/// One endpoint, answering connections.
///
/// The order is the security posture: a head, then `Origin`, then the token,
/// and only then the body. A rejected request costs this server one header
/// block rather than sixteen megabytes.
pub struct EndpointHandler {
    endpoint: std::sync::Arc<Endpoint>,
}

impl EndpointHandler {
    pub fn new(endpoint: std::sync::Arc<Endpoint>) -> Self {
        Self { endpoint }
    }
}

impl super::server::Handler for EndpointHandler {
    fn handle(
        &self,
        input: &mut dyn std::io::BufRead,
        output: &mut dyn Write,
    ) -> std::io::Result<super::server::Flow> {
        let head = match super::request::read_head(input) {
            // The client closed the connection between requests, which is
            // how keep-alive ends. Writing a refusal here would put a
            // response nobody asked for onto a socket nobody is reading —
            // and a client that read it back would find two responses to one
            // request.
            Ok(None) => return Ok(super::server::Flow::Close),
            Ok(Some(head)) => head,
            Err(refusal) => {
                write_response(output, &Response::text(refusal.status, &refusal.message))?;
                return Ok(super::server::Flow::Close);
            }
        };
        if let Some(refusal) = self.endpoint.admits(&head.method, &head.headers) {
            write_response(output, &refusal)?;
            // The body was never read, so this connection cannot carry
            // another request: closing is the honest end.
            return Ok(super::server::Flow::Close);
        }
        let body = match super::request::read_body(input, &head) {
            Ok(body) => body,
            Err(refusal) => {
                write_response(output, &Response::text(refusal.status, &refusal.message))?;
                return Ok(super::server::Flow::Close);
            }
        };
        let request = Request {
            method: head.method,
            path: head.path,
            query: head.query,
            headers: head.headers,
            body,
        };
        let mut clock = crate::clock::SystemClock;
        self.endpoint.serve(&request, &mut clock, output)?;
        Ok(super::server::Flow::KeepAlive)
    }
}
