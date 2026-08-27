//! The one ordering claim this server makes, under pressure.
//!
//! The protocol does not order notifications against responses and neither
//! does this server. What it claims is narrower and absolute: **no message's
//! bytes ever appear inside another message's line**. One writer, one lock
//! held across bytes, newline and flush, and every reader of the stream sees
//! whole messages whatever raced to produce them.
//!
//! The suite carries one wall-clock assertion — the bounded shutdown check —
//! and nothing else that a loaded machine can turn red.
//!
//! Plan 0012 task 6102.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use fsm_cli::clock::FixedClock;
use fsm_cli::mcp::notify::{Notifier, SharedSink};
use fsm_cli::mcp::subscribe::Subscriptions;
use fsm_cli::mcp::watch::Feed;
use fsm_cli::store::Store;
use fsm_core::json::{JsonLimits, Value, parse};

/// A scratch directory that removes itself.
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

static TMP_N: AtomicU64 = AtomicU64::new(0);

/// Every test that spawns a feed takes this, because the spawn counter is
/// per-process: two such tests running side by side would each count the
/// other's threads.
static SPAWNS: Mutex<()> = Mutex::new(());

fn scratch(tag: &str) -> Scratch {
    let dir = std::env::temp_dir().join(format!(
        "fsm-order-{tag}-{}-{}",
        std::process::id(),
        TMP_N.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    Scratch(dir)
}

/// One event that always applies, so a test can write as many records as it
/// likes without book-keeping which state each instance is in.
const CASE: &str = r#"{"format":"fsm.machine/1","name":"order_case","states":[{"name":"open"},{"name":"held"}],"initial":"open","context":[],"events":[{"name":"push","fields":[]}],"transitions":[{"from":"open","on":"push","to":"held"},{"from":"held","on":"push","to":"open"}]}"#;

fn value(source: &str) -> Value {
    parse(source.as_bytes(), &JsonLimits::DEFAULT).unwrap()
}

/// A store with the machine defined and two instances.
fn seeded(dir: &Scratch) -> Store {
    let mut store = Store::open(dir).unwrap();
    let mut clock = FixedClock::new(1_000, 1);
    store
        .define_machine_on(&mut clock, value(CASE), false, false)
        .unwrap();
    for id in ["inst-a", "inst-b"] {
        store
            .create_instance_ctx_on(
                &mut clock,
                "order_case",
                id,
                &format!("seed-{id}"),
                None,
                &BTreeMap::new(),
                &[],
            )
            .unwrap();
    }
    store
}

/// One event into one instance.
fn advance(store: &mut Store, id: &str, nth: usize) {
    store
        .send_event(
            id,
            "push",
            Value::Obj(BTreeMap::new()),
            &format!("{id}-{nth}"),
            None,
        )
        .unwrap();
}

fn lines(sink: &SharedSink) -> Vec<String> {
    sink.text().lines().map(str::to_string).collect()
}

/// Every line is one whole JSON-RPC message, or this returns the line that
/// was not.
fn parsed(stream: &[String]) -> Vec<Value> {
    stream
        .iter()
        .map(|line| {
            parse(line.as_bytes(), &JsonLimits::DEFAULT)
                .unwrap_or_else(|error| panic!("a torn line: {error:?} in {line:.120}"))
        })
        .collect()
}

#[test]
fn four_writers_and_two_thousand_messages_never_tear_a_line() {
    // Pressure, not the appearance of it: four notifier threads and a
    // response producer sharing one `Notifier`, message sizes spanning a
    // small notification and a tool result big enough to be split by any
    // buffer between here and the reader.
    const THREADS: usize = 4;
    const EACH: usize = 500;
    let sink = SharedSink::new();
    let notifier = Notifier::new(Box::new(sink.writer()));
    let mut workers = Vec::new();
    for thread in 0..THREADS {
        let out = notifier.clone_handle();
        workers.push(std::thread::spawn(move || {
            for index in 0..EACH {
                // Sizes chosen to straddle every buffer boundary in the way:
                // a bare notification, and a payload measured in tens of
                // kilobytes.
                let pad = match index % 4 {
                    0 => 0,
                    1 => 64,
                    2 => 4_096,
                    _ => 48_000,
                };
                let params = Value::Obj(BTreeMap::from([
                    ("thread".to_string(), Value::Num(thread.to_string())),
                    ("index".to_string(), Value::Num(index.to_string())),
                    ("pad".to_string(), Value::Str("x".repeat(pad))),
                ]));
                out.notify("notifications/message", params).unwrap();
            }
        }));
    }
    // The response-producing loop, through the same writer the real one uses.
    let responder = {
        let out = notifier.clone_handle();
        std::thread::spawn(move || {
            for id in 0..EACH {
                let result = Value::Obj(BTreeMap::from([(
                    "text".to_string(),
                    Value::Str("r".repeat(if id % 2 == 0 { 32 } else { 32_000 })),
                )]));
                let message = Value::Obj(BTreeMap::from([
                    ("jsonrpc".to_string(), Value::Str("2.0".into())),
                    ("id".to_string(), Value::Num(id.to_string())),
                    ("result".to_string(), result),
                ]));
                out.send(&message).unwrap();
            }
        })
    };
    for worker in workers {
        worker.join().unwrap();
    }
    responder.join().unwrap();

    let stream = lines(&sink);
    assert_eq!(
        stream.len(),
        THREADS * EACH + EACH,
        "every message is exactly one line"
    );
    let messages = parsed(&stream);

    // The multiset of what came out is exactly the multiset of what went in.
    let mut produced = BTreeSet::new();
    for thread in 0..THREADS {
        for index in 0..EACH {
            produced.insert(format!("n{thread}:{index}"));
        }
    }
    for id in 0..EACH {
        produced.insert(format!("r{id}"));
    }
    let mut observed = BTreeSet::new();
    for message in &messages {
        let key = match message.get("method").and_then(Value::as_str) {
            Some(_) => {
                let params = message.get("params").expect("params");
                format!(
                    "n{}:{}",
                    params.get("thread").and_then(Value::as_num).unwrap(),
                    params.get("index").and_then(Value::as_num).unwrap()
                )
            }
            None => format!("r{}", message.get("id").and_then(Value::as_num).unwrap()),
        };
        assert!(observed.insert(key.clone()), "duplicated message {key}");
    }
    assert_eq!(observed, produced, "nothing lost, nothing invented");
}

#[test]
fn one_notification_per_uri_per_batch_however_the_records_arrive() {
    let dir = scratch("batch");
    let mut store = seeded(&dir);
    let sink = SharedSink::new();
    let notifier = Notifier::new(Box::new(sink.writer()));
    let mut watched = Subscriptions::default();
    watched.subscribe("fsm://instance/inst-a");
    watched.subscribe("fsm://instance/inst-b");
    let mut feed = Feed::new(
        &dir,
        watched.clone_handle(),
        notifier.clone_handle(),
        store.journal.last_seq,
    );

    // Ten records touching two instances, then one pass over all of them.
    for nth in 0..10 {
        advance(
            &mut store,
            if nth % 2 == 0 { "inst-a" } else { "inst-b" },
            nth,
        );
    }
    let sent = feed.poll_once();
    assert_eq!(sent, 2, "one notification per affected URI, not per record");
    let uris: Vec<String> = parsed(&lines(&sink))
        .iter()
        .map(|message| {
            message
                .get("params")
                .and_then(|p| p.get("uri"))
                .and_then(Value::as_str)
                .unwrap()
                .to_string()
        })
        .collect();
    assert_eq!(uris, ["fsm://instance/inst-a", "fsm://instance/inst-b"]);

    // Now with writes between every pass: no single pass can speak twice
    // about one URI, whatever arrives between them.
    for round in 0..40 {
        let before = lines(&sink).len();
        advance(&mut store, "inst-a", round + 10);
        advance(&mut store, "inst-a", round + 100);
        feed.poll_once();
        let batch = &lines(&sink)[before..];
        let mut seen = BTreeSet::new();
        for line in batch {
            let message = parse(line.as_bytes(), &JsonLimits::DEFAULT).unwrap();
            if let Some(uri) = message.get("params").and_then(|p| p.get("uri")) {
                assert!(
                    seen.insert(uri.as_str().unwrap().to_string()),
                    "one pass named the same URI twice"
                );
            }
        }
    }
}

#[test]
fn the_watermark_reports_every_seq_once_and_skips_none() {
    let dir = scratch("watermark");
    let mut store = seeded(&dir);
    let sink = SharedSink::new();
    let notifier = Notifier::new(Box::new(sink.writer()));
    let mut watched = Subscriptions::default();
    watched.subscribe("fsm://instance/inst-a");
    let start = store.journal.last_seq;
    let mut feed = Feed::new(&dir, watched.clone_handle(), notifier.clone_handle(), start);

    // A hundred rounds of write-then-poll, and the reported ranges are
    // consecutive: `(previous, current]` with no gap and no overlap, which is
    // exactly "none skipped and none reported twice".
    let mut previous = start;
    for round in 0..100 {
        // Some rounds write nothing, some write several: a feed that only
        // works when every poll has exactly one record to report is a feed
        // that works on nobody's timeline.
        let records = round % 3;
        for extra in 0..records {
            advance(&mut store, "inst-a", round * 3 + extra);
        }
        let before = lines(&sink).len();
        feed.poll_once();
        let reported = lines(&sink).len() - before;
        assert_eq!(
            reported,
            usize::from(records > 0),
            "a pass reports its records once, and a pass with none says nothing"
        );
        let current = feed.watermark();
        assert!(
            current >= previous,
            "the watermark went backwards: {previous} then {current}"
        );
        assert!(
            current <= store.journal.last_seq,
            "the feed reported a seq the journal has not written"
        );
        previous = current;
    }
    assert_eq!(
        feed.watermark(),
        store.journal.last_seq,
        "after the last pass, everything written has been reported"
    );
}

#[test]
fn a_feed_stops_within_one_interval_of_every_session_ending() {
    // The suite's one wall-clock assertion. Twenty sessions in a row, each
    // spawning a real feed and ending at EOF; if a feed outlived its session
    // the join would stall and this would take twenty stalls to finish.
    let _guard = SPAWNS.lock().unwrap_or_else(|e| e.into_inner());
    let dir = scratch("lifecycle");
    let mut store = seeded(&dir);
    let before = fsm_cli::mcp::notify::feeds_spawned();
    let started = std::time::Instant::now();
    for round in 0..20 {
        let sink = SharedSink::new();
        let input = format!(
            "{}\n{}\n",
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}"#,
            format_args!(
                r#"{{"jsonrpc":"2.0","id":2,"method":"resources/subscribe","params":{{"uri":"fsm://instance/inst-a"}}}}"#
            )
        );
        let _ = round;
        fsm_cli::mcp::serve::serve_session(
            Some(&mut store),
            &mut FixedClock::new(1_000, 1),
            std::io::Cursor::new(input.into_bytes()),
            sink.writer(),
        )
        .unwrap();
    }
    let elapsed = started.elapsed();
    assert_eq!(
        fsm_cli::mcp::notify::feeds_spawned() - before,
        20,
        "one feed per subscribing session"
    );
    // Twenty sessions, each joining its feed. One poll interval is 250ms; a
    // session that waited for one would take 5s for the twenty, and a feed
    // that never stopped would never get here at all.
    assert!(
        elapsed < std::time::Duration::from_secs(5),
        "twenty sessions took {elapsed:?}; a feed is outliving its session"
    );
}

/// A stream that accepts a fixed number of bytes and then refuses, the way a
/// client that walked away does.
#[derive(Clone)]
struct Closing {
    written: Arc<AtomicUsize>,
    allow: usize,
}

impl Write for Closing {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let total = self.written.fetch_add(buf.len(), Ordering::Relaxed) + buf.len();
        if total > self.allow {
            return Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "the client went away",
            ));
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn a_stream_closed_mid_batch_ends_the_feed_quietly() {
    let dir = scratch("closing");
    let mut store = seeded(&dir);
    let written = Arc::new(AtomicUsize::new(0));
    let notifier = Notifier::new(Box::new(Closing {
        written: Arc::clone(&written),
        // Enough for the first notification and not the second.
        allow: 120,
    }));
    let mut watched = Subscriptions::default();
    watched.subscribe("fsm://instance/inst-a");
    watched.subscribe("fsm://instance/inst-b");
    let mut feed = Feed::new(
        &dir,
        watched.clone_handle(),
        notifier.clone_handle(),
        store.journal.last_seq,
    );
    advance(&mut store, "inst-a", 0);
    advance(&mut store, "inst-b", 1);

    let sent = feed.poll_once();
    assert!(sent <= 1, "the pass stopped at the closed stream");
    assert!(
        notifier.is_broken(),
        "and the stream is remembered as broken"
    );
    let after_break = written.load(Ordering::Relaxed);

    // The watermark did not advance, so the batch is not lost — and the loop
    // that would retry it exits instead, because the stream is broken.
    //
    // The watchdog is a safety net rather than an assertion: a feed that
    // ignored the broken stream would spin here forever, and a test that
    // hangs says less than one that fails. Stopped, such a feed would have
    // written to the closed stream on its way past, which the count below
    // catches.
    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let finished = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let flag = Arc::clone(&stop);
    let done = Arc::clone(&finished);
    std::thread::spawn(move || {
        for _ in 0..80 {
            if done.load(Ordering::Relaxed) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        flag.store(true, Ordering::Relaxed);
    });
    feed.run(&stop, 25);
    finished.store(true, Ordering::Relaxed);
    assert_eq!(
        written.load(Ordering::Relaxed),
        after_break,
        "nothing more was written to a stream that is gone"
    );
    assert!(
        feed.watermark() < store.journal.last_seq,
        "an unreported batch stays unreported rather than being skipped"
    );
}

#[test]
fn a_uri_is_notified_only_while_it_is_subscribed() {
    let dir = scratch("boundary");
    let mut store = seeded(&dir);
    let sink = SharedSink::new();
    let notifier = Notifier::new(Box::new(sink.writer()));
    let mut watched = Subscriptions::default();
    watched.subscribe("fsm://instance/inst-a");
    let mut feed = Feed::new(
        &dir,
        watched.clone_handle(),
        notifier.clone_handle(),
        store.journal.last_seq,
    );

    // Subscribed before the write: notified.
    advance(&mut store, "inst-a", 0);
    advance(&mut store, "inst-b", 1);
    assert_eq!(feed.poll_once(), 1);
    let text = sink.text();
    assert!(text.contains("inst-a"), "the subscribed URI was named");
    assert!(
        !text.contains("inst-b"),
        "and a URI nobody subscribed to was not: {text}"
    );

    // Subscribed after the write: not notified for that write. The pass that
    // reported it has moved the watermark past it, which is what makes a late
    // subscriber's first notification be about the future.
    let mut late = watched.clone_handle();
    late.subscribe("fsm://instance/inst-b");
    let before = lines(&sink).len();
    assert_eq!(feed.poll_once(), 0, "there is nothing new to report");
    assert_eq!(lines(&sink).len(), before);

    // And unsubscribed before the write: silent.
    let mut gone = watched.clone_handle();
    gone.unsubscribe("fsm://instance/inst-a");
    advance(&mut store, "inst-a", 2);
    advance(&mut store, "inst-b", 3);
    let before = lines(&sink).len();
    assert_eq!(feed.poll_once(), 1, "only the still-subscribed URI speaks");
    let batch = &lines(&sink)[before..];
    assert_eq!(batch.len(), 1);
    assert!(batch[0].contains("inst-b"), "{}", batch[0]);
}

#[test]
fn a_uri_nobody_subscribed_to_is_never_named_by_a_running_feed() {
    // The concurrent half of the boundary: a real feed thread polling on its
    // own timer while writes and subscription changes land beside it. The
    // claim is the one concurrency cannot make unsound — a URI that was never
    // subscribed is never named. When a notification for a *removed* URI
    // stops is a question about a pass already in flight, which the
    // deterministic test above answers per pass.
    let _guard = SPAWNS.lock().unwrap_or_else(|e| e.into_inner());
    let dir = scratch("racing");
    let mut store = seeded(&dir);
    let sink = SharedSink::new();
    let notifier = Notifier::new(Box::new(sink.writer()));
    let mut watched = Subscriptions::default();
    watched.subscribe("fsm://instance/inst-a");
    let from_seq = store.journal.last_seq;
    let handle = {
        let data_dir = dir.to_path_buf();
        let watched = watched.clone_handle();
        let out = notifier.clone_handle();
        fsm_cli::mcp::notify::FeedHandle::spawn(move |stop| {
            let mut feed = Feed::new(&data_dir, watched, out, from_seq);
            feed.run(stop, 25);
        })
    };
    for round in 0..40 {
        advance(&mut store, "inst-a", round);
        advance(&mut store, "inst-b", round + 1_000);
        let mut toggle = watched.clone_handle();
        if round % 2 == 0 {
            toggle.unsubscribe("fsm://instance/inst-a");
        } else {
            toggle.subscribe("fsm://instance/inst-a");
        }
    }
    let mut handle = handle;
    handle.stop_and_join();
    let text = sink.text();
    assert!(
        !text.contains("inst-b"),
        "a URI nobody ever subscribed to was named: {text}"
    );
    for line in lines(&sink) {
        parse(line.as_bytes(), &JsonLimits::DEFAULT).expect("a whole message");
    }
}

#[test]
fn a_session_answers_every_request_while_its_feed_writes_beside_it() {
    // The production arrangement rather than a model of it: one session
    // answering requests while its own feed pushes notifications through the
    // handle it cloned. No claim about when anything arrives — only that
    // every line is whole and every request is answered exactly once.
    let _guard = SPAWNS.lock().unwrap_or_else(|e| e.into_inner());
    let dir = scratch("session");
    let mut store = seeded(&dir);
    let sink = SharedSink::new();
    let mut input = String::from(
        "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{\"protocolVersion\":\"2025-06-18\"}}\n{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"resources/subscribe\",\"params\":{\"uri\":\"fsm://instance/inst-a\"}}\n",
    );
    for id in 3..200 {
        input.push_str(&format!(
            "{{\"jsonrpc\":\"2.0\",\"id\":{id},\"method\":\"tools/call\",\"params\":{{\"name\":\"instance_send\",\"arguments\":{{\"instance_id\":\"inst-a\",\"event\":{{\"name\":\"push\"}},\"request_id\":\"busy-{id}\"}}}}}}\n"
        ));
    }
    fsm_cli::mcp::serve::serve_session(
        Some(&mut store),
        &mut FixedClock::new(1_000, 1),
        std::io::Cursor::new(input.into_bytes()),
        sink.writer(),
    )
    .unwrap();

    let messages = parsed(&lines(&sink));
    let mut answered = BTreeSet::new();
    for message in &messages {
        if let Some(id) = message.get("id").and_then(Value::as_num) {
            assert!(answered.insert(id.to_string()), "id {id} answered twice");
        }
    }
    let expected: BTreeSet<String> = (1..200).map(|id| id.to_string()).collect();
    assert_eq!(answered, expected, "every request answered exactly once");
}
