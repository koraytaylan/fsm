//! A session watches what it asks to watch, within a cap, and the first
//! subscription is what brings the change feed to life.
//!
//! Plan 0012 task 5901.

use std::collections::BTreeMap;
use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use fsm_cli::clock::FixedClock;
use fsm_cli::mcp::notify::{SharedSink, feeds_spawned};
use fsm_cli::mcp::serve::serve_session;
use fsm_cli::mcp::subscribe::MAX_SUBSCRIPTIONS;
use fsm_cli::store::Store;
use fsm_core::json::{JsonLimits, Value, parse};

static NEXT: AtomicU64 = AtomicU64::new(0);
/// The spawn counter is process-global and a subscription spawns a feed, so
/// **every** test here takes turns — a delta read while another test's feed
/// starts is measuring the wrong thing.
static COUNTER: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn counting() -> std::sync::MutexGuard<'static, ()> {
    COUNTER
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn create() -> Self {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("fsm-sub-{}-{n}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

const CASE: &str = r#"{"format":"fsm.machine/1","name":"sub_case","states":[{"name":"intake"},{"name":"done","terminal":true}],"initial":"intake","context":[],"events":[{"name":"go","fields":[]}],"transitions":[{"from":"intake","on":"go","to":"done"}]}"#;

const HELLO: &str =
    r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}"#;

fn value(source: &str) -> Value {
    parse(source.as_bytes(), &JsonLimits::DEFAULT).unwrap()
}

/// A store with `count` instances, all on one machine.
fn ready(directory: &TestDirectory, count: usize) -> Store {
    let mut store = Store::open(directory.path()).unwrap();
    let mut clock = FixedClock::new(1_000, 1);
    store
        .define_machine_on(&mut clock, value(CASE), false, false)
        .unwrap();
    for index in 0..count {
        store
            .create_instance_ctx_on(
                &mut clock,
                "sub_case",
                &format!("inst-{index:02}"),
                &format!("create-{index}"),
                None,
                &BTreeMap::new(),
                &[],
            )
            .unwrap();
    }
    store
}

fn subscribe(id: u64, uri: &str) -> String {
    format!(
        r#"{{"jsonrpc":"2.0","id":{id},"method":"resources/subscribe","params":{{"uri":"{uri}"}}}}"#
    )
}

fn unsubscribe(id: u64, uri: &str) -> String {
    format!(
        r#"{{"jsonrpc":"2.0","id":{id},"method":"resources/unsubscribe","params":{{"uri":"{uri}"}}}}"#
    )
}

/// Drive one session over the given lines and return its replies by id.
fn session(store: &mut Store, lines: &[String]) -> BTreeMap<String, Value> {
    let mut clock = FixedClock::new(2_000, 1);
    let sink = SharedSink::new();
    let input = format!("{HELLO}\n{}\n", lines.join("\n"));
    serve_session(
        Some(store),
        &mut clock,
        Cursor::new(input.into_bytes()),
        sink.writer(),
    )
    .unwrap();
    sink.text()
        .lines()
        .filter_map(|line| parse(line.as_bytes(), &JsonLimits::DEFAULT).ok())
        .filter_map(|message| {
            let id = message.get("id").and_then(Value::as_num)?.to_string();
            Some((id, message))
        })
        .collect()
}

fn is_ok(reply: &Value) -> bool {
    reply.get("error").is_none() && reply.get("result").is_some()
}

fn error_code(reply: &Value) -> Option<&str> {
    reply.get("error")?.get("code")?.as_num()
}

#[test]
fn subscribing_to_a_servable_uri_succeeds() {
    let _counting = counting();
    let directory = TestDirectory::create();
    let mut store = ready(&directory, 1);
    let replies = session(
        &mut store,
        &[
            subscribe(2, "fsm://instance/inst-00"),
            subscribe(3, "fsm://docs/spec"),
        ],
    );
    assert!(is_ok(&replies["2"]), "{:?}", replies["2"]);
    assert!(is_ok(&replies["3"]), "a documentation URI is servable too");
    assert_eq!(
        replies["2"].get("result"),
        Some(&Value::Obj(BTreeMap::new())),
        "an empty result object"
    );
}

#[test]
fn a_uri_the_server_cannot_serve_is_refused_the_way_a_read_would_be() {
    let _counting = counting();
    let directory = TestDirectory::create();
    let mut store = ready(&directory, 1);
    let replies = session(
        &mut store,
        &[
            subscribe(2, "fsm://instance/nosuch"),
            subscribe(3, "fsm://nonsense"),
            subscribe(4, "fsm://instance/inst-00/ledger"),
        ],
    );
    for id in ["2", "3", "4"] {
        assert_eq!(
            error_code(&replies[id]),
            Some("-32002"),
            "a subscription must never name something unreadable: {:?}",
            replies[id]
        );
    }
}

#[test]
fn both_operations_are_idempotent() {
    let _counting = counting();
    let directory = TestDirectory::create();
    let mut store = ready(&directory, 1);
    let replies = session(
        &mut store,
        &[
            subscribe(2, "fsm://instance/inst-00"),
            subscribe(3, "fsm://instance/inst-00"),
            unsubscribe(4, "fsm://instance/inst-00"),
            unsubscribe(5, "fsm://instance/inst-00"),
        ],
    );
    for id in ["2", "3", "4", "5"] {
        assert!(
            is_ok(&replies[id]),
            "the client's intent is satisfied either way: {:?}",
            replies[id]
        );
    }
}

#[test]
fn the_cap_is_the_only_backpressure_and_it_names_itself() {
    let _counting = counting();
    let directory = TestDirectory::create();
    let mut store = ready(&directory, MAX_SUBSCRIPTIONS + 2);
    let mut lines = Vec::new();
    for index in 0..=MAX_SUBSCRIPTIONS {
        lines.push(subscribe(
            (index + 2) as u64,
            &format!("fsm://instance/inst-{index:02}"),
        ));
    }
    let replies = session(&mut store, &lines);
    let last_ok = (MAX_SUBSCRIPTIONS + 1).to_string();
    assert!(
        is_ok(&replies[&last_ok]),
        "the {}th succeeds: {:?}",
        MAX_SUBSCRIPTIONS,
        replies[&last_ok]
    );
    let over = (MAX_SUBSCRIPTIONS + 2).to_string();
    assert_eq!(error_code(&replies[&over]), Some("-32602"));
    let message = replies[&over]
        .get("error")
        .and_then(|error| error.get("message"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    assert!(
        message.contains(&MAX_SUBSCRIPTIONS.to_string()),
        "the refusal names the cap: {message}"
    );
}

#[test]
fn the_feed_starts_on_the_first_subscription_and_stays_for_the_session() {
    let _counting = counting();
    let directory = TestDirectory::create();
    let mut store = ready(&directory, 2);

    let before = feeds_spawned();
    session(
        &mut store,
        &[r#"{"jsonrpc":"2.0","id":2,"method":"ping"}"#.to_string()],
    );
    assert_eq!(feeds_spawned(), before, "nothing watched, nothing spawned");

    session(
        &mut store,
        &[
            subscribe(2, "fsm://instance/inst-00"),
            subscribe(3, "fsm://instance/inst-01"),
            // Unsubscribing the last URI does not stop the feed: a session
            // that resubscribes is common, and a parked feed costs one
            // integer comparison per interval.
            unsubscribe(4, "fsm://instance/inst-00"),
            unsubscribe(5, "fsm://instance/inst-01"),
            subscribe(6, "fsm://instance/inst-00"),
        ],
    );
    assert_eq!(
        feeds_spawned(),
        before + 1,
        "one feed for the session, however many subscriptions came and went"
    );
}

#[test]
fn two_sessions_watch_independently() {
    let _counting = counting();
    let directory = TestDirectory::create();
    let mut store = ready(&directory, 2);
    let first = session(&mut store, &[subscribe(2, "fsm://instance/inst-00")]);
    assert!(is_ok(&first["2"]));
    // A second session starts with an empty set: unsubscribing what the
    // first watched still succeeds, because both operations are idempotent
    // and nothing is shared between processes.
    let second = session(&mut store, &[unsubscribe(2, "fsm://instance/inst-00")]);
    assert!(is_ok(&second["2"]));
}

#[test]
fn a_subscription_before_initialized_is_accepted() {
    let _counting = counting();
    let directory = TestDirectory::create();
    let mut store = ready(&directory, 1);
    // The session below never sends `notifications/initialized`, matching
    // the leniency the loop already applies to every other method.
    let replies = session(&mut store, &[subscribe(2, "fsm://instance/inst-00")]);
    assert!(is_ok(&replies["2"]));
}
