//! Coming back to a stream, and the two ways that can fail.
//!
//! Plan 0015 task 7004.

use std::io::{BufReader, Cursor, Write};
use std::sync::Arc;

use fsm_cli::clock::FixedClock;
use fsm_cli::http::endpoint::{DEFAULT_PATH, Endpoint};
use fsm_cli::http::request::{Request, read_request};
use fsm_cli::http::sse::{REPLAY_BYTES, REPLAY_EVENTS, ResumeError, SessionStream, Stream};
use fsm_cli::mcp::notify::SharedSink;
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
        "fsm-resume-{tag}-{}-{}",
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

const CASE: &str = r#"{"format":"fsm.machine/1","name":"resume_case","states":[{"name":"open"},{"name":"held"}],"initial":"open","context":[],"events":[{"name":"push","fields":[]}],"transitions":[{"from":"open","on":"push","to":"held"},{"from":"held","on":"push","to":"open"}]}"#;

fn seeded(dir: &Scratch) -> Store {
    let mut store = Store::open(dir).unwrap();
    let mut clock = FixedClock::new(1_000, 1);
    store
        .define_machine_on(&mut clock, value(CASE), false, false)
        .unwrap();
    store
}

fn raw(method: &str, headers: &[(&str, &str)], body: &str) -> Request {
    let mut text = format!("{method} {DEFAULT_PATH} HTTP/1.1\r\nHost: localhost\r\n");
    for (name, value) in headers {
        text.push_str(&format!("{name}: {value}\r\n"));
    }
    if !body.is_empty() {
        text.push_str(&format!("Content-Length: {}\r\n", body.len()));
    }
    text.push_str("\r\n");
    text.push_str(body);
    let mut input = BufReader::new(Cursor::new(text.into_bytes()));
    read_request(&mut input).expect("well formed")
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

/// A session, its stream state, and a recording of everything sent on it.
fn streaming(endpoint: &Endpoint) -> (String, Arc<Stream>, SharedSink) {
    let session = header(
        &serve(endpoint, &raw("POST", &[], INITIALIZE)),
        "Mcp-Session-Id",
    )
    .expect("a session");
    let stream = endpoint.stream_state(&session);
    let sink = SharedSink::new();
    (session, stream, sink)
}

/// Emit `count` events on a stream, recording the exact bytes sent.
fn emit(stream: &Arc<Stream>, sink: &SharedSink, count: usize, size: usize) {
    let mut writer = SessionStream::new(sink.writer(), Arc::clone(stream));
    for n in 0..count {
        let padding = "x".repeat(size);
        writeln!(writer, "{{\"n\":{n},\"pad\":\"{padding}\"}}").unwrap();
    }
}

#[test]
fn a_client_resumes_where_it_stopped_with_no_gap_and_no_duplicate() {
    let dir = scratch("resume");
    let endpoint = Endpoint::new(DEFAULT_PATH, Some(seeded(&dir)), "");
    let (session, stream, sink) = streaming(&endpoint);
    emit(&stream, &sink, 5, 4);
    let first_delivery = sink.text();

    // Events that happen *while* the client is away are buffered.
    emit(&stream, &sink, 3, 4);

    let resumed = serve(
        &endpoint,
        &raw(
            "GET",
            &[
                ("Mcp-Session-Id", session.as_str()),
                ("Accept", "text/event-stream"),
                ("Last-Event-ID", "5"),
            ],
            "",
        ),
    );
    assert_eq!(status(&resumed), 200);
    let ids: Vec<u64> = resumed
        .lines()
        .filter_map(|line| line.strip_prefix("id: "))
        .filter_map(|id| id.parse().ok())
        .collect();
    assert_eq!(ids, [6, 7, 8], "no duplicate and no gap");

    // The replayed bytes are the bytes that were sent, not bytes made now.
    let all_sent = sink.text();
    let sent: Vec<&str> = all_sent.lines().collect();
    for id in &ids {
        let original = sent
            .iter()
            .position(|line| *line == format!("id: {id}"))
            .map(|at| sent[at + 1])
            .expect("the original event");
        assert!(
            resumed.contains(original),
            "a replayed event differs from the one that was sent: {original}"
        );
    }
    assert!(
        !first_delivery.is_empty(),
        "the first delivery was recorded to compare against"
    );
}

#[test]
fn resuming_from_the_newest_event_replays_nothing() {
    let dir = scratch("caught-up");
    let endpoint = Endpoint::new(DEFAULT_PATH, Some(seeded(&dir)), "");
    let (session, stream, sink) = streaming(&endpoint);
    emit(&stream, &sink, 4, 4);
    let resumed = serve(
        &endpoint,
        &raw(
            "GET",
            &[
                ("Mcp-Session-Id", session.as_str()),
                ("Accept", "text/event-stream"),
                ("Last-Event-ID", "4"),
            ],
            "",
        ),
    );
    assert_eq!(status(&resumed), 200);
    assert!(
        !resumed.contains("data: "),
        "a client that missed nothing is sent nothing: {resumed}"
    );
    // And the next live event continues the sequence rather than restarting.
    assert_eq!(stream.next_id(), 4);
    emit(&stream, &sink, 1, 4);
    assert_eq!(stream.next_id(), 5);
}

#[test]
fn the_two_ways_a_resume_fails_are_two_different_answers() {
    let dir = scratch("refusals");
    let endpoint = Endpoint::new(DEFAULT_PATH, Some(seeded(&dir)), "");
    let (session, stream, sink) = streaming(&endpoint);
    // Past the event bound, so the earliest ids are gone.
    emit(&stream, &sink, REPLAY_EVENTS + 20, 4);

    let get = |last: &str| {
        raw(
            "GET",
            &[
                ("Mcp-Session-Id", session.as_str()),
                ("Accept", "text/event-stream"),
                ("Last-Event-ID", last),
            ],
            "",
        )
    };
    let evicted = serve(&endpoint, &get("2"));
    assert_eq!(
        status(&evicted),
        409,
        "resuming from the oldest retained event would hand the client a gap \
         it cannot detect, which is worse than refusing: {evicted:.200}"
    );
    assert!(evicted.contains("re-initialize"), "{evicted}");
    assert_eq!(ResumeError::Evicted.status(), 409);

    // An id this session never issued is the client's mistake, not an expiry.
    let unknown = serve(&endpoint, &get("999999"));
    assert_eq!(status(&unknown), 400);
    assert_eq!(ResumeError::Unknown.status(), 400);
}

#[test]
fn the_buffer_evicts_by_count() {
    let stream = Stream::default();
    for n in 0..300 {
        stream.record(format!("{{\"n\":{n}}}").as_bytes());
    }
    assert_eq!(stream.buffered_events(), REPLAY_EVENTS);
    let kept = stream.resume_after(300 - REPLAY_EVENTS as u64).unwrap();
    assert_eq!(kept.len(), REPLAY_EVENTS, "the newest 256 are still there");
    assert!(
        stream.resume_after(1).is_err(),
        "and the oldest 44 are gone, which is said rather than hidden"
    );
}

#[test]
fn the_buffer_evicts_by_size_before_it_reaches_the_count() {
    // Twenty events of 64 KiB is well under 256 events and well over 1 MiB:
    // the cost this bounds is memory, and memory is bytes.
    let stream = Stream::default();
    let big = "y".repeat(64 * 1024);
    for _ in 0..20 {
        stream.record(big.as_bytes());
    }
    assert!(
        stream.buffered_events() < 20,
        "size evicted nothing: {} events, {} bytes",
        stream.buffered_events(),
        stream.buffered_bytes()
    );
    assert!(
        stream.buffered_bytes() <= REPLAY_BYTES,
        "{} bytes buffered against a {REPLAY_BYTES} bound",
        stream.buffered_bytes()
    );
}

#[test]
fn the_buffer_does_not_outlive_the_session() {
    let dir = scratch("delete");
    let endpoint = Endpoint::new(DEFAULT_PATH, Some(seeded(&dir)), "");
    let (session, stream, sink) = streaming(&endpoint);
    emit(&stream, &sink, 10, 100);
    assert!(stream.buffered_bytes() > 0);

    let deleted = serve(
        &endpoint,
        &raw("DELETE", &[("Mcp-Session-Id", session.as_str())], ""),
    );
    assert_eq!(status(&deleted), 200);
    assert_eq!(
        stream.buffered_bytes(),
        0,
        "a disconnected client's buffer outlived its session"
    );
    assert_eq!(stream.buffered_events(), 0);
}
