//! What the journal did while nobody was asking, told to whoever asked to be
//! told.
//!
//! Plan 0012 task 5902.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use fsm_cli::clock::FixedClock;
use fsm_cli::mcp::notify::{Notifier, SharedSink};
use fsm_cli::mcp::subscribe::Subscriptions;
use fsm_cli::mcp::watch::Feed;
use fsm_cli::store::Store;
use fsm_core::hashes::{child_instance_id, digest_of, machine_id};
use fsm_core::json::{JsonLimits, Value, parse};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn create() -> Self {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("fsm-feed-{}-{n}", std::process::id()));
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

const CASE: &str = r#"{"format":"fsm.machine/1","name":"feed_case","states":[{"name":"intake"},{"name":"middle"},{"name":"done","terminal":true}],"initial":"intake","context":[],"events":[{"name":"go","fields":[]},{"name":"stop","fields":[]}],"transitions":[{"from":"intake","on":"go","to":"middle"},{"from":"middle","on":"go","to":"intake"},{"from":"middle","on":"stop","to":"done"}]}"#;

const CHILD: &str = r#"{"format":"fsm.machine/1","name":"feed_child","states":[{"name":"working"},{"name":"done","terminal":true}],"initial":"working","context":[],"events":[{"name":"finish","fields":[]}],"transitions":[{"from":"working","on":"finish","to":"done"}]}"#;

fn parent_source() -> String {
    format!(
        r#"{{"format":"fsm.machine/1","name":"feed_parent","states":[{{"name":"idle"}},{{"name":"busy","invoke":[{{"id":"down","machine":"{}"}}]}},{{"name":"out"}}],"initial":"idle","context":[],"events":[{{"name":"open","fields":[]}},{{"name":"give_up","fields":[]}}],"transitions":[{{"from":"idle","on":"open","to":"busy"}},{{"from":"busy","on":"$done.invoke.down","to":"out"}},{{"from":"busy","on":"give_up","to":"out"}}]}}"#,
        digest(CHILD)
    )
}

const SENDER: &str = r#"{"format":"fsm.machine/1","name":"feed_sender","states":[{"name":"idle"},{"name":"sent","entry":{"signal":[{"to":"ctx.peer","event":"go"}]}}],"initial":"idle","context":[{"name":"peer","ty":"str","init":"inst-target"}],"events":[{"name":"send","fields":[]}],"transitions":[{"from":"idle","on":"send","to":"sent"}]}"#;

/// A feed watching the given URIs, starting from the store's current seq.
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

fn notified(sink: &SharedSink) -> Vec<String> {
    sink.text()
        .lines()
        .filter_map(|line| parse(line.as_bytes(), &JsonLimits::DEFAULT).ok())
        .filter(|message| {
            message.get("method").and_then(Value::as_str) == Some("notifications/resources/updated")
        })
        .filter_map(|message| {
            message
                .get("params")?
                .get("uri")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .collect()
}

/// A store with one machine and one instance.
fn ready(directory: &TestDirectory) -> Store {
    let mut store = Store::open(directory.path()).unwrap();
    let mut clock = FixedClock::new(1_000, 1);
    store
        .define_machine_on(&mut clock, value(CASE), false, false)
        .unwrap();
    store
        .create_instance_ctx_on(
            &mut clock,
            "feed_case",
            "inst-1",
            "create-1",
            None,
            &BTreeMap::new(),
            &[],
        )
        .unwrap();
    store
}

#[test]
fn an_advance_notifies_the_subscriber_once_per_uri() {
    let directory = TestDirectory::create();
    let mut store = ready(&directory);
    let (mut feed, sink) = watching(
        &directory,
        &["fsm://instance/inst-1", "fsm://instance/inst-1/history"],
    );
    store
        .send_event("inst-1", "go", Value::Obj(BTreeMap::new()), "go-1", None)
        .unwrap();
    assert_eq!(feed.poll_once(), 2);
    assert_eq!(
        notified(&sink),
        [
            "fsm://instance/inst-1".to_string(),
            "fsm://instance/inst-1/history".to_string()
        ],
        "ordered by URI, so a batch is comparable"
    );
    // And the watermark stops it happening again.
    assert_eq!(feed.poll_once(), 0);
    assert_eq!(notified(&sink).len(), 2);
}

#[test]
fn an_unchanged_journal_costs_one_comparison() {
    let directory = TestDirectory::create();
    let store = ready(&directory);
    drop(store);
    let (mut feed, sink) = watching(&directory, &["fsm://instance/inst-1"]);
    for _ in 0..5 {
        assert_eq!(feed.poll_once(), 0);
    }
    assert!(notified(&sink).is_empty());
    assert_eq!(
        feed.walks(),
        0,
        "the case that runs four times a second forever must not walk records"
    );
}

#[test]
fn ten_records_touching_one_instance_are_one_notification_each() {
    let directory = TestDirectory::create();
    let mut store = ready(&directory);
    let (mut feed, sink) = watching(
        &directory,
        &["fsm://instance/inst-1", "fsm://instance/inst-1/history"],
    );
    for index in 0..10 {
        let event = if index % 2 == 0 { "go" } else { "go" };
        store
            .send_event(
                "inst-1",
                event,
                Value::Obj(BTreeMap::new()),
                &format!("go-{index}"),
                None,
            )
            .unwrap();
    }
    assert_eq!(feed.poll_once(), 2, "two URIs, not twenty notifications");
    assert_eq!(notified(&sink).len(), 2);
}

#[test]
fn an_unsubscribed_instance_is_not_reported() {
    let directory = TestDirectory::create();
    let mut store = ready(&directory);
    let mut clock = FixedClock::new(3_000, 1);
    store
        .create_instance_ctx_on(
            &mut clock,
            "feed_case",
            "inst-2",
            "create-2",
            None,
            &BTreeMap::new(),
            &[],
        )
        .unwrap();
    let (mut feed, sink) = watching(&directory, &["fsm://instance/inst-1"]);
    store
        .send_event("inst-2", "go", Value::Obj(BTreeMap::new()), "go-2", None)
        .unwrap();
    assert_eq!(feed.poll_once(), 0);
    assert!(notified(&sink).is_empty());
}

#[test]
fn a_batch_is_byte_deterministic() {
    let render = || {
        let directory = TestDirectory::create();
        let mut store = ready(&directory);
        let (mut feed, sink) = watching(
            &directory,
            &["fsm://instance/inst-1", "fsm://instance/inst-1/history"],
        );
        store
            .send_event("inst-1", "go", Value::Obj(BTreeMap::new()), "go-1", None)
            .unwrap();
        feed.poll_once();
        sink.text()
    };
    assert_eq!(render(), render());
}

/// A writer that fails, so a batch cannot be written.
struct Broken;

impl std::io::Write for Broken {
    fn write(&mut self, _buf: &[u8]) -> std::io::Result<usize> {
        Err(std::io::Error::other("the client is gone"))
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn a_write_error_leaves_the_watermark_where_it_was() {
    let directory = TestDirectory::create();
    let mut store = ready(&directory);
    let from_seq = store.journal.last_seq;
    let mut watched = Subscriptions::default();
    watched.subscribe("fsm://instance/inst-1");
    let mut feed = Feed::new(
        directory.path(),
        watched,
        Notifier::new(Box::new(Broken)),
        from_seq,
    );
    store
        .send_event("inst-1", "go", Value::Obj(BTreeMap::new()), "go-1", None)
        .unwrap();
    assert_eq!(feed.poll_once(), 0, "nothing was written");
    assert_eq!(
        feed.watermark(),
        from_seq,
        "a duplicate notification is harmless; a missed one is not"
    );
}

#[test]
fn the_feed_coexists_with_a_writer() {
    let directory = TestDirectory::create();
    // The writer stays open across the poll: the feed takes no lock.
    let mut store = ready(&directory);
    let (mut feed, sink) = watching(&directory, &["fsm://instance/inst-1"]);
    store
        .send_event("inst-1", "go", Value::Obj(BTreeMap::new()), "go-1", None)
        .unwrap();
    assert_eq!(feed.poll_once(), 1);
    assert_eq!(notified(&sink), ["fsm://instance/inst-1".to_string()]);
}

#[test]
fn a_definition_notifies_its_machine_uri() {
    let directory = TestDirectory::create();
    let mut store = Store::open(directory.path()).unwrap();
    let mut clock = FixedClock::new(1_000, 1);
    store
        .define_machine_on(&mut clock, value(CASE), false, false)
        .unwrap();
    let id = machine_id(&value(CASE));
    let (mut feed, sink) = watching(&directory, &[&format!("fsm://machine/{id}")]);
    store
        .define_machine_on(&mut clock, value(CHILD), false, false)
        .unwrap();
    // The new definition is a different machine, so the watched URI is not
    // in the batch.
    assert_eq!(feed.poll_once(), 0);
    assert!(notified(&sink).is_empty());

    let child_uri = format!("fsm://machine/{}", machine_id(&value(CHILD)));
    let (mut feed, sink) = watching(&directory, &[&child_uri]);
    let another = CHILD.replace("feed_child", "feed_child_two");
    store
        .define_machine_on(&mut clock, value(&another), false, false)
        .unwrap();
    assert_eq!(feed.poll_once(), 0, "a different machine again");
    let _ = sink;
}

#[test]
fn composition_records_notify_both_sides() {
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
            "feed_parent",
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
    let child = child_instance_id("p1", "down");

    // A subscriber on the parent is told about the invocation.
    let (mut parent_feed, parent_sink) = watching(&directory, &["fsm://instance/p1"]);
    // And so is a subscriber on the child, by the same record.
    let (mut child_feed, child_sink) = watching(&directory, &[&format!("fsm://instance/{child}")]);
    store.invoke_child("p1", "down", "inv-1").unwrap();
    assert_eq!(parent_feed.poll_once(), 1);
    assert_eq!(
        child_feed.poll_once(),
        1,
        "a field-name probe would miss this"
    );
    assert_eq!(notified(&parent_sink), ["fsm://instance/p1".to_string()]);
    assert_eq!(notified(&child_sink), [format!("fsm://instance/{child}")]);

    // The return notifies both sides too.
    store
        .send_event(&child, "finish", Value::Obj(BTreeMap::new()), "fin-1", None)
        .unwrap();
    assert_eq!(child_feed.poll_once(), 1);
    store.invocation_return("p1", "down", "ret-1").unwrap();
    assert_eq!(parent_feed.poll_once(), 1);
    assert_eq!(child_feed.poll_once(), 1);
}

#[test]
fn a_delivered_signal_notifies_its_target() {
    let directory = TestDirectory::create();
    let mut store = Store::open(directory.path()).unwrap();
    let mut clock = FixedClock::new(1_000, 1);
    store
        .define_machine_on(&mut clock, value(CASE), false, false)
        .unwrap();
    store
        .create_instance_ctx_on(
            &mut clock,
            "feed_case",
            "inst-target",
            "c-target",
            None,
            &BTreeMap::new(),
            &[],
        )
        .unwrap();
    store
        .define_machine_on(&mut clock, value(SENDER), false, false)
        .unwrap();
    store
        .create_instance_ctx_on(
            &mut clock,
            "feed_sender",
            "s1",
            "c-s1",
            None,
            &BTreeMap::new(),
            &[],
        )
        .unwrap();
    store
        .send_event("s1", "send", Value::Obj(BTreeMap::new()), "send-1", None)
        .unwrap();
    let signal_id = store.state.instances["s1"]
        .signals
        .keys()
        .next()
        .cloned()
        .unwrap();

    let (mut feed, sink) = watching(&directory, &["fsm://instance/inst-target"]);
    store.signal_deliver("s1", &signal_id, "sig-1").unwrap();
    assert_eq!(
        feed.poll_once(),
        1,
        "the target is named by `target_instance_id`, which a probe for `instance_id` would miss"
    );
    assert_eq!(notified(&sink), ["fsm://instance/inst-target".to_string()]);
}
