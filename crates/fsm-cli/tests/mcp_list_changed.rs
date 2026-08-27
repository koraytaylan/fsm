//! A listing whose membership changed says so, once — and a listing whose
//! members merely moved says nothing.
//!
//! Plan 0012 task 5903.

use std::collections::BTreeMap;
use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use fsm_cli::clock::FixedClock;
use fsm_cli::mcp::notify::{Notifier, SharedSink};
use fsm_cli::mcp::serve::serve_session;
use fsm_cli::mcp::subscribe::Subscriptions;
use fsm_cli::mcp::watch::Feed;
use fsm_cli::store::Store;
use fsm_core::hashes::{digest_of, machine_id};
use fsm_core::json::{JsonLimits, Value, parse};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn create() -> Self {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("fsm-listch-{}-{n}", std::process::id()));
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

fn value(source: &str) -> Value {
    parse(source.as_bytes(), &JsonLimits::DEFAULT).unwrap()
}

fn digest(source: &str) -> String {
    digest_of(&machine_id(&value(source))).unwrap().to_string()
}

const CASE: &str = r#"{"format":"fsm.machine/1","name":"lc_case","states":[{"name":"intake"},{"name":"done","terminal":true}],"initial":"intake","context":[],"events":[{"name":"go","fields":[]}],"effects":[{"name":"notify","fields":[]}],"transitions":[{"from":"intake","on":"go","to":"done","emit":[{"effect":"notify"}]}]}"#;

const CHILD: &str = r#"{"format":"fsm.machine/1","name":"lc_child","states":[{"name":"working"},{"name":"done","terminal":true}],"initial":"working","context":[],"events":[{"name":"finish","fields":[]}],"transitions":[{"from":"working","on":"finish","to":"done"}]}"#;

fn parent_source() -> String {
    format!(
        r#"{{"format":"fsm.machine/1","name":"lc_parent","states":[{{"name":"idle"}},{{"name":"busy","invoke":[{{"id":"down","machine":"{}"}}]}},{{"name":"out"}}],"initial":"idle","context":[],"events":[{{"name":"open","fields":[]}},{{"name":"give_up","fields":[]}}],"transitions":[{{"from":"idle","on":"open","to":"busy"}},{{"from":"busy","on":"$done.invoke.down","to":"out"}},{{"from":"busy","on":"give_up","to":"out"}}]}}"#,
        digest(CHILD)
    )
}

fn watching(directory: &TestDirectory, uris: &[&str]) -> (Feed, SharedSink) {
    let from_seq = Store::open_read_only(directory.path())
        .map(|store| store.journal.last_seq)
        .unwrap_or(0);
    let mut watched = Subscriptions::default();
    for uri in uris {
        watched.subscribe(uri);
    }
    let sink = SharedSink::new();
    let notifier = Notifier::new(Box::new(sink.writer()));
    (
        Feed::new(directory.path(), watched, notifier, from_seq),
        sink,
    )
}

fn methods(sink: &SharedSink) -> Vec<String> {
    sink.text()
        .lines()
        .filter_map(|line| parse(line.as_bytes(), &JsonLimits::DEFAULT).ok())
        .filter_map(|message| {
            message
                .get("method")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .collect()
}

fn list_changed(sink: &SharedSink) -> usize {
    methods(sink)
        .iter()
        .filter(|method| method.as_str() == "notifications/resources/list_changed")
        .count()
}

fn defined(directory: &TestDirectory) -> Store {
    let mut store = Store::open(directory.path()).unwrap();
    store
        .define_machine_on(&mut FixedClock::new(1_000, 1), value(CASE), false, false)
        .unwrap();
    store
}

#[test]
fn one_creation_is_one_notification() {
    let directory = TestDirectory::create();
    let mut store = defined(&directory);
    let (mut feed, sink) = watching(&directory, &[]);
    store
        .create_instance_ctx_on(
            &mut FixedClock::new(2_000, 1),
            "lc_case",
            "inst-1",
            "c1",
            None,
            &BTreeMap::new(),
            &[],
        )
        .unwrap();
    feed.poll_once();
    assert_eq!(list_changed(&sink), 1);
}

#[test]
fn a_batch_of_five_joiners_is_still_one_notification() {
    let directory = TestDirectory::create();
    let mut store = defined(&directory);
    let (mut feed, sink) = watching(&directory, &[]);
    let mut clock = FixedClock::new(2_000, 1);
    for index in 0..3 {
        store
            .create_instance_ctx_on(
                &mut clock,
                "lc_case",
                &format!("inst-{index}"),
                &format!("c{index}"),
                None,
                &BTreeMap::new(),
                &[],
            )
            .unwrap();
    }
    for name in ["one", "two"] {
        let source = CASE.replace("lc_case", &format!("lc_case_{name}"));
        store
            .define_machine_on(&mut clock, value(&source), false, false)
            .unwrap();
    }
    feed.poll_once();
    assert_eq!(
        list_changed(&sink),
        1,
        "however many appeared, the listing changed once"
    );
}

#[test]
fn an_invoked_child_joins_the_listing() {
    let directory = TestDirectory::create();
    let mut store = Store::open(directory.path()).unwrap();
    let mut clock = FixedClock::new(1_000, 1);
    for source in [CHILD.to_string(), parent_source()] {
        store
            .define_machine_on(&mut clock, value(&source), false, false)
            .unwrap();
    }
    store
        .create_instance_ctx_on(
            &mut clock,
            "lc_parent",
            "p1",
            "c1",
            None,
            &BTreeMap::new(),
            &[],
        )
        .unwrap();
    store
        .send_event("p1", "open", Value::Obj(BTreeMap::new()), "open-1", None)
        .unwrap();
    // From here on, only the invocation.
    let (mut feed, sink) = watching(&directory, &[]);
    store.invoke_child("p1", "down", "inv-1").unwrap();
    feed.poll_once();
    assert_eq!(
        list_changed(&sink),
        1,
        "a child that appears in the listing without this is a listing nobody re-reads"
    );
}

#[test]
fn movement_alone_changes_no_listing() {
    let directory = TestDirectory::create();
    let mut store = defined(&directory);
    store
        .create_instance_ctx_on(
            &mut FixedClock::new(2_000, 1),
            "lc_case",
            "inst-1",
            "c1",
            None,
            &BTreeMap::new(),
            &[],
        )
        .unwrap();
    let (mut feed, sink) = watching(&directory, &["fsm://instance/inst-1"]);
    store
        .send_event("inst-1", "go", Value::Obj(BTreeMap::new()), "go-1", None)
        .unwrap();
    let effect = store.state.instances["inst-1"].pending[0].clone();
    store
        .ack_effect_outcome("inst-1", &effect, "ack-1", "ok", None)
        .unwrap();
    feed.poll_once();
    assert_eq!(
        list_changed(&sink),
        0,
        "a client that re-listed on every transition would be worse off than one that polled"
    );
    assert!(
        methods(&sink)
            .iter()
            .any(|m| m == "notifications/resources/updated"),
        "the instance itself was still reported"
    );
}

#[test]
fn the_updates_come_first_so_a_re_listing_client_sees_a_consistent_listing() {
    let directory = TestDirectory::create();
    let mut store = defined(&directory);
    store
        .create_instance_ctx_on(
            &mut FixedClock::new(2_000, 1),
            "lc_case",
            "inst-1",
            "c1",
            None,
            &BTreeMap::new(),
            &[],
        )
        .unwrap();
    let (mut feed, sink) = watching(&directory, &["fsm://instance/inst-1"]);
    let mut clock = FixedClock::new(3_000, 1);
    store
        .send_event("inst-1", "go", Value::Obj(BTreeMap::new()), "go-1", None)
        .unwrap();
    store
        .create_instance_ctx_on(
            &mut clock,
            "lc_case",
            "inst-2",
            "c2",
            None,
            &BTreeMap::new(),
            &[],
        )
        .unwrap();
    feed.poll_once();
    let order = methods(&sink);
    let updated = order
        .iter()
        .position(|m| m == "notifications/resources/updated")
        .expect("the watched instance moved");
    let changed = order
        .iter()
        .position(|m| m == "notifications/resources/list_changed")
        .expect("a new instance joined");
    assert!(updated < changed, "{order:?}");
}

#[test]
fn nothing_the_server_did_not_advertise_is_ever_sent() {
    // A full session exercising tools, resources, and prompts: the two
    // static capabilities are `false`, and sending a notification the server
    // did not advertise is a protocol error rather than a courtesy.
    let directory = TestDirectory::create();
    let mut store = defined(&directory);
    let sink = SharedSink::new();
    let lines = [
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05"}}"#.to_string(),
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#.to_string(),
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#.to_string(),
        r#"{"jsonrpc":"2.0","id":3,"method":"prompts/list"}"#.to_string(),
        r#"{"jsonrpc":"2.0","id":4,"method":"resources/list"}"#.to_string(),
        r#"{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"instance_create","arguments":{"machine":"lc_case","request_id":"lc-1"}}}"#
            .to_string(),
        r#"{"jsonrpc":"2.0","id":6,"method":"resources/subscribe","params":{"uri":"fsm://docs/spec"}}"#.to_string(),
    ];
    serve_session(
        Some(&mut store),
        &mut FixedClock::new(4_000, 1),
        Cursor::new(lines.join("\n").into_bytes()),
        sink.writer(),
    )
    .unwrap();
    for method in methods(&sink) {
        assert!(
            method != "notifications/tools/list_changed"
                && method != "notifications/prompts/list_changed",
            "{method} was never advertised"
        );
    }
}

#[test]
fn the_notification_carries_nothing_it_does_not_need() {
    let directory = TestDirectory::create();
    let mut store = defined(&directory);
    let (mut feed, sink) = watching(&directory, &[]);
    store
        .create_instance_ctx_on(
            &mut FixedClock::new(2_000, 1),
            "lc_case",
            "inst-1",
            "c1",
            None,
            &BTreeMap::new(),
            &[],
        )
        .unwrap();
    feed.poll_once();
    let line = sink
        .text()
        .lines()
        .find(|line| line.contains("list_changed"))
        .expect("one was sent")
        .to_string();
    assert_eq!(
        line,
        r#"{"jsonrpc":"2.0","method":"notifications/resources/list_changed","params":{}}"#
    );
}
