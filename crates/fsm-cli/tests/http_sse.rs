//! The stream a server speaks on, and what it costs to hold one.
//!
//! Plan 0015 task 7003.

use std::collections::BTreeMap;
use std::io::{BufReader, Cursor, Write};
use std::sync::Arc;

use fsm_cli::clock::FixedClock;
use fsm_cli::http::endpoint::{DEFAULT_PATH, Endpoint};
use fsm_cli::http::request::{Request, read_request};
use fsm_cli::http::sse::{KEEPALIVE_MS, REPLAY_EVENTS, SessionStream, Stream, notifier_for};
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
        "fsm-sse-{tag}-{}-{}",
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

const CASE: &str = r#"{"format":"fsm.machine/1","name":"sse_case","states":[{"name":"open"},{"name":"held"}],"initial":"open","context":[],"events":[{"name":"push","fields":[]}],"transitions":[{"from":"open","on":"push","to":"held"},{"from":"held","on":"push","to":"open"}]}"#;

fn seeded(dir: &Scratch) -> Store {
    let mut store = Store::open(dir).unwrap();
    let mut clock = FixedClock::new(1_000, 1);
    store
        .define_machine_on(&mut clock, value(CASE), false, false)
        .unwrap();
    store
        .create_instance_ctx_on(
            &mut clock,
            "sse_case",
            "inst-s",
            "create-1",
            None,
            &BTreeMap::new(),
            &[],
        )
        .unwrap();
    store
}

fn raw(method: &str, path: &str, headers: &[(&str, &str)], body: &str) -> Request {
    let mut text = format!("{method} {path} HTTP/1.1\r\nHost: localhost\r\n");
    for (name, value) in headers {
        text.push_str(&format!("{name}: {value}\r\n"));
    }
    if !body.is_empty() {
        text.push_str(&format!("Content-Length: {}\r\n", body.len()));
    }
    text.push_str("\r\n");
    text.push_str(body);
    let mut input = BufReader::new(Cursor::new(text.into_bytes()));
    read_request(&mut input).expect("a well-formed request")
}

fn serve(endpoint: &Endpoint, request: &Request) -> String {
    let mut out = Vec::new();
    endpoint
        .serve(request, &mut FixedClock::new(2_000, 1), &mut out)
        .expect("written");
    String::from_utf8_lossy(&out).to_string()
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

const INITIALIZE: &str =
    r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}"#;

fn open_session(endpoint: &Endpoint) -> String {
    header(
        &serve(endpoint, &raw("POST", DEFAULT_PATH, &[], INITIALIZE)),
        "Mcp-Session-Id",
    )
    .expect("a session")
}

#[test]
fn a_get_with_the_right_accept_opens_a_stream() {
    let dir = scratch("open");
    let endpoint = Endpoint::new(DEFAULT_PATH, Some(seeded(&dir)), "");
    let session = open_session(&endpoint);
    let response = serve(
        &endpoint,
        &raw(
            "GET",
            DEFAULT_PATH,
            &[
                ("Mcp-Session-Id", session.as_str()),
                ("Accept", "text/event-stream"),
            ],
            "",
        ),
    );
    assert_eq!(status(&response), 200);
    assert_eq!(
        header(&response, "Content-Type").as_deref(),
        Some("text/event-stream")
    );
    assert!(
        !response.contains("Content-Length"),
        "a stream has no length to state"
    );
}

#[test]
fn a_get_without_the_accept_is_not_handed_a_stream() {
    let dir = scratch("accept");
    let endpoint = Endpoint::new(DEFAULT_PATH, Some(seeded(&dir)), "");
    let session = open_session(&endpoint);
    let response = serve(
        &endpoint,
        &raw(
            "GET",
            DEFAULT_PATH,
            &[("Mcp-Session-Id", session.as_str())],
            "",
        ),
    );
    assert_eq!(status(&response), 406);
}

#[test]
fn one_stream_per_session_and_another_session_is_free_to_open_its_own() {
    let dir = scratch("one");
    let endpoint = Endpoint::new(DEFAULT_PATH, Some(seeded(&dir)), "");
    let first = open_session(&endpoint);
    let second = open_session(&endpoint);
    let get = |session: &str| {
        raw(
            "GET",
            DEFAULT_PATH,
            &[("Mcp-Session-Id", session), ("Accept", "text/event-stream")],
            "",
        )
    };
    assert_eq!(status(&serve(&endpoint, &get(&first))), 200);
    assert_eq!(
        status(&serve(&endpoint, &get(&first))),
        409,
        "two streams would split ordering with nothing to reassemble it"
    );
    assert_eq!(
        status(&serve(&endpoint, &get(&second))),
        200,
        "and another session is another stream"
    );
}

#[test]
fn a_disconnect_frees_the_slot_and_leaves_the_session_alone() {
    let dir = scratch("reconnect");
    let endpoint = Endpoint::new(DEFAULT_PATH, Some(seeded(&dir)), "");
    let session = open_session(&endpoint);
    let get = raw(
        "GET",
        DEFAULT_PATH,
        &[
            ("Mcp-Session-Id", session.as_str()),
            ("Accept", "text/event-stream"),
        ],
        "",
    );
    assert_eq!(status(&serve(&endpoint, &get)), 200);

    // The client goes away: the stream slot is released, the session is not.
    endpoint.stream_state(&session).release();
    assert_eq!(
        status(&serve(&endpoint, &get)),
        200,
        "a client that comes back gets its stream back"
    );
    assert!(
        endpoint
            .sessions()
            .touch(Some(&session), None, 3_000)
            .is_ok(),
        "and the session it belongs to was never in question"
    );
}

#[test]
fn deleting_a_session_closes_its_stream_without_a_farewell() {
    let dir = scratch("delete");
    let endpoint = Endpoint::new(DEFAULT_PATH, Some(seeded(&dir)), "");
    let session = open_session(&endpoint);
    let headers = [
        ("Mcp-Session-Id", session.as_str()),
        ("Accept", "text/event-stream"),
    ];
    assert_eq!(
        status(&serve(&endpoint, &raw("GET", DEFAULT_PATH, &headers, ""))),
        200
    );
    let goodbye = serve(&endpoint, &raw("DELETE", DEFAULT_PATH, &headers, ""));
    assert_eq!(status(&goodbye), 200);
    assert!(
        !goodbye.contains("data:"),
        "there is nothing to say and the client may already be gone: {goodbye}"
    );
    assert_eq!(
        status(&serve(&endpoint, &raw("GET", DEFAULT_PATH, &headers, ""))),
        404,
        "the session is gone, so its stream is not there to reopen"
    );
}

#[test]
fn a_notifier_over_a_session_stream_needs_no_change_above_the_transport() {
    // The claim plan 0012 was designed to make good on: its notifier, its
    // framing, its bytes — into a socket.
    let stream = Arc::new(Stream::default());
    let sink = fsm_cli::mcp::notify::SharedSink::new();
    let notifier = notifier_for(Arc::clone(&stream), sink.writer());
    notifier
        .notify(
            "notifications/resources/updated",
            fsm_cli::mcp::watch::updated_params("fsm://instance/inst-s"),
        )
        .unwrap();
    let text = sink.text();
    assert!(text.starts_with("id: 1\ndata: "), "{text}");
    assert!(text.ends_with("\n\n"), "{text}");

    // Byte-identical to what the stdio transport writes for the same event.
    let stdio = fsm_cli::mcp::notify::SharedSink::new();
    let over_stdio = fsm_cli::mcp::notify::Notifier::new(Box::new(stdio.writer()));
    over_stdio
        .notify(
            "notifications/resources/updated",
            fsm_cli::mcp::watch::updated_params("fsm://instance/inst-s"),
        )
        .unwrap();
    let expected = stdio.text();
    let on_the_wire = text
        .lines()
        .find_map(|line| line.strip_prefix("data: "))
        .expect("a data line");
    assert_eq!(
        on_the_wire,
        expected.trim_end(),
        "the payload differs between transports"
    );
}

#[test]
fn ids_are_monotonic_and_the_buffer_knows_the_same_ones() {
    let stream = Arc::new(Stream::default());
    let sink = fsm_cli::mcp::notify::SharedSink::new();
    let mut writer = SessionStream::new(sink.writer(), Arc::clone(&stream));
    for n in 0..5 {
        writeln!(writer, "{{\"n\":{n}}}").unwrap();
    }
    let text = sink.text();
    let ids: Vec<u64> = text
        .lines()
        .filter_map(|line| line.strip_prefix("id: "))
        .filter_map(|id| id.parse().ok())
        .collect();
    assert_eq!(ids, [1, 2, 3, 4, 5]);
    let (kept, gap) = stream.replay_after(0);
    assert!(!gap);
    assert_eq!(
        kept.iter().map(|event| event.id).collect::<Vec<_>>(),
        ids,
        "the wire and the buffer agree by construction, not by staying in step"
    );
}

#[test]
fn the_replay_buffer_is_bounded_and_says_when_it_dropped_something() {
    let stream = Stream::default();
    for n in 0..(REPLAY_EVENTS + 10) {
        stream.record(format!("{{\"n\":{n}}}").as_bytes());
    }
    let (kept, gap) = stream.replay_after(0);
    assert_eq!(
        kept.len(),
        REPLAY_EVENTS,
        "a stream nobody reads must not grow"
    );
    assert!(
        gap,
        "a client that thinks it caught up and has not is worse off than one that knows"
    );
    // A client resuming from near the end missed nothing the buffer had to
    // drop, and is told so: the gap is about *this* caller's position, not
    // about the buffer's history.
    let (recent, gap) = stream.replay_after(kept.last().unwrap().id - 1);
    assert_eq!(recent.len(), 1);
    assert!(
        !gap,
        "a caller who missed nothing must not be told they did"
    );
}

#[test]
fn a_keepalive_is_a_comment_and_costs_no_id() {
    assert_eq!(KEEPALIVE_MS, 15_000, "the documented interval");
    let stream = Arc::new(Stream::default());
    let sink = fsm_cli::mcp::notify::SharedSink::new();
    let mut writer = SessionStream::new(sink.writer(), Arc::clone(&stream));
    writeln!(writer, "{{\"first\":true}}").unwrap();
    writer.keepalive().unwrap();
    writeln!(writer, "{{\"second\":true}}").unwrap();
    let text = sink.text();
    assert!(text.contains(": keepalive\n\n"), "{text}");
    assert!(
        text.contains("id: 2\ndata: {\"second\":true}"),
        "a comment is not an event, so it must not consume an id: {text}"
    );
}

#[test]
fn one_session_reading_slowly_does_not_stop_another() {
    // Each session writes into its own socket, so a client that stopped
    // reading costs its own connection and nothing else.
    let dir = scratch("slow");
    let endpoint = Endpoint::new(DEFAULT_PATH, Some(seeded(&dir)), "");
    let slow = open_session(&endpoint);
    let brisk = open_session(&endpoint);

    let slow_stream = endpoint.stream_state(&slow);
    let brisk_stream = endpoint.stream_state(&brisk);

    // The slow client's socket refuses everything.
    struct Broken;
    impl Write for Broken {
        fn write(&mut self, _buf: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "stopped reading",
            ))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let to_slow = notifier_for(Arc::clone(&slow_stream), Broken);
    let sink = fsm_cli::mcp::notify::SharedSink::new();
    let to_brisk = notifier_for(Arc::clone(&brisk_stream), sink.writer());

    let _ = to_slow.notify("notifications/message", Value::Obj(BTreeMap::new()));
    to_brisk
        .notify("notifications/message", Value::Obj(BTreeMap::new()))
        .expect("the other session is unaffected");
    assert!(
        to_slow.is_broken(),
        "the slow client's own stream is broken"
    );
    assert!(!to_brisk.is_broken());
    assert!(sink.text().contains("notifications/message"));
}
