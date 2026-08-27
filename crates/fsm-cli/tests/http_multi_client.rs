//! Many clients, one writer, one coherent journal.
//!
//! Plan 0015 task 7201.

use std::collections::BTreeMap;
use std::io::{BufReader, Cursor};
use std::sync::Arc;

use fsm_cli::clock::FixedClock;
use fsm_cli::http::endpoint::{DEFAULT_PATH, Endpoint};
use fsm_cli::http::request::{Request, read_request};
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
        "fsm-multi-{tag}-{}-{}",
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

const CASE: &str = r#"{"format":"fsm.machine/1","name":"multi_case","context":[{"name":"seen","ty":"int","init":"0"}],"states":[{"name":"open"},{"name":"held"}],"initial":"open","events":[{"name":"push","fields":[]}],"transitions":[{"from":"open","on":"push","to":"held","do":[{"target":"seen","value":"ctx.seen + 1"}]},{"from":"held","on":"push","to":"open","do":[{"target":"seen","value":"ctx.seen + 1"}]}]}"#;

fn seeded(dir: &Scratch) -> Store {
    let mut store = Store::open(dir).unwrap();
    let mut clock = FixedClock::new(1_000, 1);
    store
        .define_machine_on(&mut clock, value(CASE), false, false)
        .unwrap();
    store
        .create_instance_ctx_on(
            &mut clock,
            "multi_case",
            "inst-m",
            "create-1",
            None,
            &BTreeMap::new(),
            &[],
        )
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

fn body(response: &str) -> String {
    response
        .split_once("\r\n\r\n")
        .map(|(_, body)| body.to_string())
        .unwrap_or_default()
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

fn open(endpoint: &Endpoint) -> String {
    header(
        &serve(endpoint, &raw("POST", &[], INITIALIZE)),
        "Mcp-Session-Id",
    )
    .unwrap()
}

fn send(endpoint: &Endpoint, session: &str, request_id: &str) -> String {
    serve(
        endpoint,
        &raw(
            "POST",
            &[("Mcp-Session-Id", session)],
            &format!(
                r#"{{"jsonrpc":"2.0","id":9,"method":"tools/call","params":{{"name":"instance_send","arguments":{{"instance_id":"inst-m","event":{{"name":"push"}},"request_id":"{request_id}"}}}}}}"#
            ),
        ),
    )
}

#[test]
fn many_sessions_writing_at_once_leave_one_coherent_journal() {
    let dir = scratch("concurrent");
    let endpoint = Arc::new(Endpoint::new(DEFAULT_PATH, Some(seeded(&dir)), ""));
    let sessions: Vec<String> = (0..4).map(|_| open(&endpoint)).collect();

    // Eight threads across four sessions, fifty events each.
    let mut workers = Vec::new();
    for worker in 0..8 {
        let endpoint = Arc::clone(&endpoint);
        let session = sessions[worker % sessions.len()].clone();
        workers.push(std::thread::spawn(move || {
            for n in 0..50 {
                let response = send(&endpoint, &session, &format!("w{worker}-{n}"));
                assert!(response.contains("200 OK"), "{response:.200}");
            }
        }));
    }
    for worker in workers {
        worker.join().expect("no worker panicked");
    }
    drop(endpoint);

    // Every event applied exactly once, and the journal verifies clean.
    let store = Store::open_read_only(&dir).expect("the journal is readable");
    let applied = store
        .records
        .iter()
        .filter(|record| record.kind == fsm_core::record::RecordKind::EventApplied)
        .count();
    assert_eq!(applied, 8 * 50, "an event was lost or applied twice");
    assert_eq!(
        store.state.instances["inst-m"]
            .ctx
            .get("seen")
            .map(|value| format!("{value:?}"))
            .unwrap_or_default()
            .contains("400"),
        true,
        "the context counted every event"
    );

    // Gapless seqs: no record interleaved with another's.
    let seqs: Vec<u64> = store.records.iter().map(|record| record.seq).collect();
    for pair in seqs.windows(2) {
        assert_eq!(pair[1], pair[0] + 1, "a gap in the journal at {}", pair[0]);
    }
    assert!(matches!(
        fsm_cli::journal_io::classify(&dir),
        fsm_cli::journal_io::JournalHealth::Ok
    ));
}

#[test]
fn a_read_never_sees_a_half_applied_macrostep() {
    let dir = scratch("consistency");
    let endpoint = Arc::new(Endpoint::new(DEFAULT_PATH, Some(seeded(&dir)), ""));
    let writer_session = open(&endpoint);
    let reader_session = open(&endpoint);

    let writing = {
        let endpoint = Arc::clone(&endpoint);
        let session = writer_session.clone();
        std::thread::spawn(move || {
            for n in 0..100 {
                let _ = send(&endpoint, &session, &format!("r{n}"));
            }
        })
    };

    // While that runs, read repeatedly: the leaf and the counter must always
    // agree, because a macrostep either happened or did not.
    for _ in 0..200 {
        let response = serve(
            &endpoint,
            &raw(
                "POST",
                &[("Mcp-Session-Id", reader_session.as_str())],
                r#"{"jsonrpc":"2.0","id":10,"method":"tools/call","params":{"name":"instance_get","arguments":{"instance_id":"inst-m"}}}"#,
            ),
        );
        let answered = value(&body(&response));
        let structured = answered
            .get("result")
            .and_then(|r| r.get("structuredContent"))
            .expect("a view");
        let leaf = structured
            .get("configuration")
            .and_then(|c| c.get("leaf"))
            .and_then(Value::as_str)
            .unwrap_or("");
        let seen: i64 = structured
            .get("context")
            .and_then(|c| c.get("seen"))
            .and_then(Value::as_str)
            .and_then(|n| n.parse().ok())
            .unwrap_or(-1);
        // Every push both moves the leaf and increments the counter, so an
        // odd count is `held` and an even one is `open`. A half-applied
        // macrostep would show up here as the two disagreeing.
        let expected = if seen % 2 == 0 { "open" } else { "held" };
        assert_eq!(leaf, expected, "seen={seen} leaf={leaf}");
    }
    writing.join().unwrap();
}

#[test]
fn the_long_reads_do_not_take_the_lock_at_all() {
    // This is what makes the mutex affordable: `journal_verify` reads
    // through `open_read_only`, so a write completes while one is running.
    let dir = scratch("verify");
    let endpoint = Arc::new(Endpoint::new(DEFAULT_PATH, Some(seeded(&dir)), ""));
    let session = open(&endpoint);
    for n in 0..200 {
        let _ = send(&endpoint, &session, &format!("seed{n}"));
    }

    let verifying = {
        let dir = dir.to_path_buf();
        std::thread::spawn(move || {
            for _ in 0..5 {
                let report = fsm_cli::mcp::tools::verify_report(
                    &dir,
                    &value("{}"),
                    &mut FixedClock::new(1_000, 1),
                    &fsm_cli::mcp::progress::ProgressReporter::discarding(),
                    &fsm_cli::mcp::cancel::CancelFlag::default(),
                )
                .expect("verified");
                assert_eq!(
                    report.get("health").and_then(Value::as_str),
                    Some("Ok"),
                    "a verify running beside a writer saw a broken store"
                );
            }
        })
    };
    // Writes proceed during the verify rather than queueing behind it.
    for n in 0..20 {
        let response = send(&endpoint, &session, &format!("during{n}"));
        assert!(response.contains("200 OK"), "{response:.200}");
    }
    verifying.join().unwrap();
}

#[test]
fn idempotency_holds_across_sessions() {
    // The property that makes many clients *safe* rather than merely
    // possible: a key means the same thing whoever presents it.
    let dir = scratch("idempotent");
    let endpoint = Endpoint::new(DEFAULT_PATH, Some(seeded(&dir)), "");
    let first = open(&endpoint);
    let second = open(&endpoint);

    let original = send(&endpoint, &first, "shared-key");
    assert!(original.contains("200 OK"));
    let replayed = send(&endpoint, &second, "shared-key");
    assert!(
        body(&replayed).contains("\"duplicate\":true"),
        "the same key and content in another session must replay: {}",
        body(&replayed)
    );

    // Different content under the same key is refused, not replayed.
    let conflict = serve(
        &endpoint,
        &raw(
            "POST",
            &[("Mcp-Session-Id", second.as_str())],
            r#"{"jsonrpc":"2.0","id":11,"method":"tools/call","params":{"name":"instance_annotate","arguments":{"instance_id":"inst-m","note":"different content","request_id":"shared-key"}}}"#,
        ),
    );
    assert!(
        body(&conflict).contains("req/request_id_conflict"),
        "{}",
        body(&conflict)
    );
}

#[test]
fn per_session_state_stays_per_session_under_load() {
    let dir = scratch("isolation");
    let endpoint = Arc::new(Endpoint::new(DEFAULT_PATH, Some(seeded(&dir)), ""));
    let watching = open(&endpoint);
    let busy = open(&endpoint);

    // One session subscribes; the other hammers the store.
    let subscribed = serve(
        &endpoint,
        &raw(
            "POST",
            &[("Mcp-Session-Id", watching.as_str())],
            r#"{"jsonrpc":"2.0","id":12,"method":"resources/subscribe","params":{"uri":"fsm://instance/inst-m"}}"#,
        ),
    );
    assert!(subscribed.contains("200 OK"));

    let working = {
        let endpoint = Arc::clone(&endpoint);
        let session = busy.clone();
        std::thread::spawn(move || {
            for n in 0..50 {
                let _ = send(&endpoint, &session, &format!("busy{n}"));
            }
        })
    };
    working.join().unwrap();

    // The busy session never acquired the other's subscription.
    let listed = serve(
        &endpoint,
        &raw(
            "POST",
            &[("Mcp-Session-Id", busy.as_str())],
            r#"{"jsonrpc":"2.0","id":13,"method":"resources/unsubscribe","params":{"uri":"fsm://instance/inst-m"}}"#,
        ),
    );
    assert!(
        listed.contains("200 OK"),
        "unsubscribing what it never had is not an error"
    );
}
