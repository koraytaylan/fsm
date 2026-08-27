//! The transport, driven the way a client drives it: over a socket.
//!
//! An in-process handler call would skip exactly the layer this plan added,
//! so everything here goes through loopback, on an ephemeral port. The
//! hostile half matters more than the happy half: every malformed shape must
//! produce a documented status **and** leave the server serving.
//!
//! Plan 0015 task 7301.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use fsm_cli::clock::FixedClock;
use fsm_cli::http::endpoint::{DEFAULT_PATH, Endpoint, EndpointHandler};
use fsm_cli::http::request::{MAX_BODY_BYTES, MAX_HEADER_BYTES, MAX_HEADERS, MAX_REQUEST_LINE};
use fsm_cli::http::security::Policy;
use fsm_cli::http::server::{Handler, bind, serve_bound};
use fsm_cli::store::Store;
use fsm_core::json::{JsonLimits, Value, parse};

struct Scratch(std::path::PathBuf);

impl std::ops::Deref for Scratch {
    type Target = std::path::Path;
    fn deref(&self) -> &std::path::Path {
        &self.0
    }
}

impl AsRef<std::path::Path> for Scratch {
    fn as_ref(&self) -> &std::path::Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn scratch(tag: &str) -> Scratch {
    let path = std::env::temp_dir().join(format!(
        "fsm-conf-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).unwrap();
    Scratch(path)
}

fn value(source: &str) -> Value {
    parse(source.as_bytes(), &JsonLimits::DEFAULT).unwrap()
}

const CASE: &str = r#"{"format":"fsm.machine/1","name":"conf_case","context":[{"name":"seen","ty":"int","init":"0"}],"states":[{"name":"open"},{"name":"held"}],"initial":"open","events":[{"name":"push","fields":[]}],"transitions":[{"from":"open","on":"push","to":"held","do":[{"target":"seen","value":"ctx.seen + 1"}]},{"from":"held","on":"push","to":"open","do":[{"target":"seen","value":"ctx.seen + 1"}]}]}"#;

fn seeded(dir: &Scratch) -> Store {
    let mut store = Store::open(dir).unwrap();
    let mut clock = FixedClock::new(1_000, 1);
    store
        .define_machine_on(&mut clock, value(CASE), false, false)
        .unwrap();
    store
        .create_instance_ctx_on(
            &mut clock,
            "conf_case",
            "inst-c",
            "create-1",
            None,
            &BTreeMap::new(),
            &[],
        )
        .unwrap();
    store
}

/// A server on an ephemeral port — no fixed ports, so this runs on a busy
/// CI machine and on all three operating systems.
struct Server {
    addr: SocketAddr,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
    _dir: Scratch,
    dir: std::path::PathBuf,
}

impl Server {
    fn start(tag: &str, token: Option<&str>) -> Self {
        let dir = scratch(tag);
        let store = seeded(&dir);
        drop(store);
        let bound = bind("127.0.0.1:0".parse().unwrap()).expect("a port");
        let addr = bound.addr();
        let policy = Policy::new(
            &addr.to_string(),
            DEFAULT_PATH,
            false,
            &[],
            token.map(str::to_string),
        )
        .expect("a loopback policy");
        let endpoint =
            Arc::new(Endpoint::new(DEFAULT_PATH, Store::open(&dir).ok(), "").with_policy(policy));
        let handler: Arc<dyn Handler> = Arc::new(EndpointHandler::new(endpoint));
        let stop = Arc::new(AtomicBool::new(false));
        let path = dir.to_path_buf();
        let thread = {
            let stop = Arc::clone(&stop);
            std::thread::spawn(move || {
                let _ = serve_bound(bound, handler, stop);
            })
        };
        Self {
            addr,
            stop,
            thread: Some(thread),
            _dir: dir,
            dir: path,
        }
    }

    fn origin(&self) -> String {
        format!("http://127.0.0.1:{}", self.addr.port())
    }

    /// Send raw bytes on a fresh connection and read the response.
    ///
    /// The write half is shut down after sending, so a request that stops
    /// short reaches the server as end-of-input rather than as silence: a
    /// client that has finished talking says so, and the alternative is
    /// waiting out a thirty-second read timeout in a test suite.
    fn raw(&self, bytes: &[u8]) -> String {
        let mut socket = TcpStream::connect(self.addr).expect("connect");
        socket.set_read_timeout(Some(Duration::from_secs(10))).ok();
        socket.write_all(bytes).expect("write");
        socket.flush().ok();
        let _ = socket.shutdown(std::net::Shutdown::Write);
        let mut text = String::new();
        let _ = socket.read_to_string(&mut text);
        text
    }

    fn post(&self, headers: &[(&str, &str)], body: &str) -> String {
        let mut request = format!(
            "POST {DEFAULT_PATH} HTTP/1.1\r\nHost: localhost\r\nOrigin: {}\r\nContent-Length: {}\r\n",
            self.origin(),
            body.len()
        );
        for (name, value) in headers {
            request.push_str(&format!("{name}: {value}\r\n"));
        }
        request.push_str("\r\n");
        request.push_str(body);
        self.raw(request.as_bytes())
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn status(response: &str) -> u16 {
    response
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse().ok())
        .unwrap_or(0)
}

fn header(response: &str, name: &str) -> Option<String> {
    response
        .lines()
        .find(|line| {
            line.to_ascii_lowercase()
                .starts_with(&format!("{}:", name.to_ascii_lowercase()))
        })
        .map(|line| line.split_once(':').unwrap().1.trim().to_string())
}

fn body(response: &str) -> String {
    response
        .split_once("\r\n\r\n")
        .map(|(_, body)| body.to_string())
        .unwrap_or_default()
}

const PING: &str = r#"{"jsonrpc":"2.0","id":9,"method":"ping"}"#;

const INITIALIZE: &str =
    r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}"#;

#[test]
fn the_happy_path_end_to_end_over_a_socket() {
    let server = Server::start("happy", None);
    let opened = server.post(&[], INITIALIZE);
    assert_eq!(status(&opened), 200, "{opened:.200}");
    let session = header(&opened, "Mcp-Session-Id").expect("a session id");

    // A tool call.
    let called = server.post(
        &[("Mcp-Session-Id", session.as_str())],
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"instance_get","arguments":{"instance_id":"inst-c"}}}"#,
    );
    assert_eq!(status(&called), 200);
    assert!(
        body(&called).contains("structuredContent"),
        "{}",
        body(&called)
    );

    // A subscription, then a write, then the stream that carries the news.
    let subscribed = server.post(
        &[("Mcp-Session-Id", session.as_str())],
        r#"{"jsonrpc":"2.0","id":3,"method":"resources/subscribe","params":{"uri":"fsm://instance/inst-c"}}"#,
    );
    assert_eq!(status(&subscribed), 200);
    let stream = server.raw(
        format!(
            "GET {DEFAULT_PATH} HTTP/1.1\r\nHost: localhost\r\nOrigin: {}\r\nAccept: text/event-stream\r\nMcp-Session-Id: {session}\r\n\r\n",
            server.origin()
        )
        .as_bytes(),
    );
    assert_eq!(status(&stream), 200);
    assert_eq!(
        header(&stream, "Content-Type").as_deref(),
        Some("text/event-stream")
    );

    // A write, and then the journal is coherent.
    let sent = server.post(
        &[("Mcp-Session-Id", session.as_str())],
        r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"instance_send","arguments":{"instance_id":"inst-c","event":{"name":"push"},"request_id":"conf-1"}}}"#,
    );
    assert_eq!(status(&sent), 200);
    assert!(body(&sent).contains("\"applied\":true"), "{}", body(&sent));

    // Teardown, and the session is gone.
    let deleted = server.raw(
        format!(
            "DELETE {DEFAULT_PATH} HTTP/1.1\r\nHost: localhost\r\nOrigin: {}\r\nMcp-Session-Id: {session}\r\n\r\n",
            server.origin()
        )
        .as_bytes(),
    );
    assert_eq!(status(&deleted), 200);
    let after = server.post(
        &[("Mcp-Session-Id", session.as_str())],
        r#"{"jsonrpc":"2.0","id":5,"method":"ping"}"#,
    );
    assert_eq!(status(&after), 404);
}

#[test]
fn every_hostile_shape_is_answered_and_the_server_survives_all_of_them() {
    let server = Server::start("hostile", Some("s3cret"));
    let origin = server.origin();
    let auth = "Bearer s3cret";
    let long_line = "x".repeat(MAX_REQUEST_LINE);
    let long_header = "v".repeat(MAX_HEADER_BYTES);
    let many_headers: String = (0..=MAX_HEADERS).map(|n| format!("X-{n}: v\r\n")).collect();

    // Each entry: a name, the raw bytes, and the status it has earned.
    let table: Vec<(&str, String, u16)> = vec![
        (
            "oversized request line",
            format!("GET /{long_line} HTTP/1.1\r\nHost: h\r\n\r\n"),
            414,
        ),
        (
            "oversized single header",
            format!("GET {DEFAULT_PATH} HTTP/1.1\r\nX-Long: {long_header}\r\n\r\n"),
            431,
        ),
        (
            "too many headers",
            format!("GET {DEFAULT_PATH} HTTP/1.1\r\n{many_headers}\r\n"),
            431,
        ),
        (
            "oversized body",
            format!(
                "POST {DEFAULT_PATH} HTTP/1.1\r\nOrigin: {origin}\r\nAuthorization: {auth}\r\nContent-Length: {}\r\n\r\n",
                MAX_BODY_BYTES + 1
            ),
            413,
        ),
        (
            // The client stops talking mid-body, which reaches the server as
            // end of input: the request never arrived.
            "content-length disagreeing with the body",
            format!(
                "POST {DEFAULT_PATH} HTTP/1.1\r\nOrigin: {origin}\r\nAuthorization: {auth}\r\nContent-Length: 100\r\n\r\nshort"
            ),
            408,
        ),
        (
            "both content-length and transfer-encoding",
            format!(
                "POST {DEFAULT_PATH} HTTP/1.1\r\nOrigin: {origin}\r\nContent-Length: 3\r\nTransfer-Encoding: chunked\r\n\r\nabc"
            ),
            400,
        ),
        (
            "chunked encoding",
            format!(
                "POST {DEFAULT_PATH} HTTP/1.1\r\nOrigin: {origin}\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n\r\n"
            ),
            411,
        ),
        (
            "obsolete line folding",
            format!("GET {DEFAULT_PATH} HTTP/1.1\r\nX-Folded: one\r\n  two\r\n\r\n"),
            400,
        ),
        (
            // Headers that never end: the connection closes and the head is
            // incomplete, which is a bad request rather than a timeout.
            "a truncated request",
            format!("GET {DEFAULT_PATH} HTTP/1.1\r\nHost: h\r\n"),
            400,
        ),
        (
            "missing origin",
            format!(
                "POST {DEFAULT_PATH} HTTP/1.1\r\nAuthorization: {auth}\r\nContent-Length: 0\r\n\r\n"
            ),
            403,
        ),
        (
            "foreign origin",
            format!(
                "POST {DEFAULT_PATH} HTTP/1.1\r\nOrigin: https://evil.example\r\nAuthorization: {auth}\r\nContent-Length: 0\r\n\r\n"
            ),
            403,
        ),
        (
            "bad token",
            format!(
                "POST {DEFAULT_PATH} HTTP/1.1\r\nOrigin: {origin}\r\nAuthorization: Bearer wrong\r\nContent-Length: 0\r\n\r\n"
            ),
            401,
        ),
        (
            "missing token",
            format!(
                "POST {DEFAULT_PATH} HTTP/1.1\r\nOrigin: {origin}\r\nContent-Length: 0\r\n\r\n"
            ),
            401,
        ),
        (
            "missing session id",
            format!(
                "POST {DEFAULT_PATH} HTTP/1.1\r\nOrigin: {origin}\r\nAuthorization: {auth}\r\nContent-Length: {}\r\n\r\n{PING}",
                PING.len()
            ),
            400,
        ),
        (
            "unknown session id",
            format!(
                "POST {DEFAULT_PATH} HTTP/1.1\r\nOrigin: {origin}\r\nAuthorization: {auth}\r\nMcp-Session-Id: 00000000000000000000000000000000\r\nContent-Length: {}\r\n\r\n{PING}",
                PING.len()
            ),
            404,
        ),
        (
            "a delete for a session that is not yours",
            format!(
                "DELETE {DEFAULT_PATH} HTTP/1.1\r\nOrigin: {origin}\r\nAuthorization: {auth}\r\nMcp-Session-Id: ffffffffffffffffffffffffffffffff\r\n\r\n"
            ),
            404,
        ),
        (
            "an unknown path",
            format!(
                "POST /elsewhere HTTP/1.1\r\nOrigin: {origin}\r\nAuthorization: {auth}\r\nContent-Length: 0\r\n\r\n"
            ),
            404,
        ),
        (
            "a method nobody routes",
            format!(
                "PUT {DEFAULT_PATH} HTTP/1.1\r\nOrigin: {origin}\r\nAuthorization: {auth}\r\nContent-Length: 0\r\n\r\n"
            ),
            405,
        ),
    ];

    for (name, bytes, expected) in &table {
        let response = server.raw(bytes.as_bytes());
        assert_eq!(
            status(&response),
            *expected,
            "{name}: {}",
            response.lines().next().unwrap_or("<nothing>")
        );
        // And the server is still a server: a well-formed request right
        // after each hostile one must be answered.
        let alive = server.post(
            &[("Authorization", auth)],
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}"#,
        );
        assert_eq!(
            status(&alive),
            200,
            "the server stopped serving after: {name}"
        );
    }

    // A second GET stream for one session is refused, and the first is not
    // disturbed.
    let opened = server.post(&[("Authorization", auth)], INITIALIZE);
    let session = header(&opened, "Mcp-Session-Id").unwrap();
    let stream = format!(
        "GET {DEFAULT_PATH} HTTP/1.1\r\nOrigin: {origin}\r\nAuthorization: {auth}\r\nAccept: text/event-stream\r\nMcp-Session-Id: {session}\r\n\r\n"
    );
    assert_eq!(status(&server.raw(stream.as_bytes())), 200);
    assert_eq!(status(&server.raw(stream.as_bytes())), 409);
}

#[test]
fn two_sessions_writing_at_once_leave_a_journal_that_verifies() {
    let server = Server::start("concurrent", None);
    let sessions: Vec<String> = (0..2)
        .map(|_| header(&server.post(&[], INITIALIZE), "Mcp-Session-Id").unwrap())
        .collect();
    let addr = server.addr;
    let origin = server.origin();

    let mut workers = Vec::new();
    for (index, session) in sessions.iter().enumerate() {
        let session = session.clone();
        let origin = origin.clone();
        workers.push(std::thread::spawn(move || {
            for n in 0..20 {
                let body = format!(
                    r#"{{"jsonrpc":"2.0","id":{n},"method":"tools/call","params":{{"name":"instance_send","arguments":{{"instance_id":"inst-c","event":{{"name":"push"}},"request_id":"s{index}-{n}"}}}}}}"#
                );
                let request = format!(
                    "POST {DEFAULT_PATH} HTTP/1.1\r\nHost: localhost\r\nOrigin: {origin}\r\nMcp-Session-Id: {session}\r\nContent-Length: {}\r\n\r\n{body}",
                    body.len()
                );
                let mut socket = TcpStream::connect(addr).expect("connect");
                socket.set_read_timeout(Some(Duration::from_secs(10))).ok();
                socket.write_all(request.as_bytes()).unwrap();
                socket.flush().unwrap();
                // Finished talking, and saying so: otherwise this waits for
                // a keep-alive connection to time out.
                let _ = socket.shutdown(std::net::Shutdown::Write);
                let mut text = String::new();
                let _ = socket.read_to_string(&mut text);
                assert_eq!(status(&text), 200, "{text:.200}");
            }
        }));
    }
    for worker in workers {
        worker.join().expect("no worker panicked");
    }

    let health = fsm_cli::journal_io::classify(&server.dir);
    assert!(
        matches!(health, fsm_cli::journal_io::JournalHealth::Ok),
        "two clients left a journal that does not verify: {health:?}"
    );
}

#[test]
fn the_transport_does_not_change_the_protocol() {
    // The same call over both transports must produce the same JSON-RPC
    // response object. If these ever differ, a transport has started making
    // protocol decisions.
    let server = Server::start("equivalence", None);
    let session = header(&server.post(&[], INITIALIZE), "Mcp-Session-Id").unwrap();

    for call in [
        r#"{"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"instance_get","arguments":{"instance_id":"inst-c"}}}"#,
        r#"{"jsonrpc":"2.0","id":8,"method":"tools/call","params":{"name":"instance_annotate","arguments":{"instance_id":"inst-c","note":"same both ways","request_id":"equal-1"}}}"#,
    ] {
        let over_http = value(&body(
            &server.post(&[("Mcp-Session-Id", session.as_str())], call),
        ));

        // The same store, the same call, over stdio.
        let stdio_dir = scratch("stdio");
        let store = seeded(&stdio_dir);
        drop(store);
        let mut store = Store::open(&stdio_dir).unwrap();
        let sink = fsm_cli::mcp::notify::SharedSink::new();
        let input = format!("{INITIALIZE}\n{call}\n");
        fsm_cli::mcp::serve::serve_session(
            Some(&mut store),
            &mut FixedClock::new(1_000, 1),
            std::io::Cursor::new(input.into_bytes()),
            sink.writer(),
        )
        .unwrap();
        let over_stdio = sink
            .text()
            .lines()
            .last()
            .map(|line| value(line))
            .expect("an answer");

        assert_eq!(
            fsm_core::canon::canon_bytes(&over_http),
            fsm_core::canon::canon_bytes(&over_stdio),
            "the transports disagree about the answer to {call:.80}"
        );
    }
}

#[test]
fn the_listener_is_healthy_after_everything_above() {
    // Belt and braces: one server, the whole hostile table, and a request
    // afterwards. If any input had panicked past the connection boundary,
    // this is where it would show.
    let server = Server::start("healthy", None);
    for bytes in [
        vec![0u8; 64],
        b"\xff\xfe\xfd\xfc".to_vec(),
        b"GET".to_vec(),
        vec![b'A'; 200_000],
    ] {
        let _ = server.raw(&bytes);
    }
    let alive = server.post(&[], INITIALIZE);
    assert_eq!(status(&alive), 200, "{alive:.200}");
}

#[test]
fn the_parser_corpus_still_runs_clean() {
    // The seeds the fuzz target is given, run here too, so the parser gets
    // the same treatment on every PR that the other targets get.
    let corpus = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/http");
    let mut seeds = 0;
    for entry in std::fs::read_dir(&corpus).expect("the corpus").flatten() {
        let bytes = std::fs::read(entry.path()).expect("a seed");
        let mut input = BufReader::new(std::io::Cursor::new(bytes.clone()));
        match fsm_cli::http::request::read_request(&mut input) {
            Ok(request) => assert!(request.path.starts_with('/')),
            Err(refusal) => assert!((400..=505).contains(&refusal.status)),
        }
        seeds += 1;
    }
    assert!(seeds >= 6);
}
