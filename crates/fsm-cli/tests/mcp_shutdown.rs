//! A background thread that outlives its session writes to a closed pipe
//! from a process that has moved on, so every exit path joins it — and the
//! cheapest lifecycle is not spawning at all.
//!
//! Plan 0012 task 5703.

use std::collections::BTreeMap;
use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use fsm_cli::clock::SystemClock;
use fsm_cli::mcp::notify::{FeedHandle, Notifier, SharedSink, feeds_spawned, sleep_unless_stopped};
use fsm_cli::mcp::serve::serve_session;
use fsm_cli::store::Store;
use fsm_core::json::{JsonLimits, Value, parse};

static NEXT: AtomicU64 = AtomicU64::new(0);

/// The spawn counter is process-global, so **every** test that spawns a feed
/// takes turns — a test reading a delta while another test's thread starts
/// would be measuring the wrong thing.
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
        let path = std::env::temp_dir().join(format!("fsm-shutdown-{}-{n}", std::process::id()));
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

const HELLO: &str =
    r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}"#;

/// Drive one session to EOF and return its transcript.
fn session(directory: &TestDirectory, lines: &str) -> String {
    let mut store = Store::open(directory.path()).unwrap();
    let mut clock = SystemClock;
    let sink = SharedSink::new();
    serve_session(
        Some(&mut store),
        &mut clock,
        Cursor::new(lines.as_bytes().to_vec()),
        sink.writer(),
    )
    .unwrap();
    sink.text()
}

#[test]
fn a_session_nobody_subscribes_to_spawns_nothing() {
    let _counting = counting();
    let directory = TestDirectory::create();
    let before = feeds_spawned();
    let transcript = session(
        &directory,
        &format!(
            "{HELLO}\n{}\n",
            r#"{"jsonrpc":"2.0","id":2,"method":"ping"}"#
        ),
    );
    assert_eq!(
        feeds_spawned(),
        before,
        "a server nobody watches does no work between requests"
    );
    assert_eq!(transcript.lines().count(), 2);
}

#[test]
fn a_subscribing_session_spawns_one_feed_and_joins_it_at_eof() {
    let _counting = counting();
    let directory = TestDirectory::create();
    let before = feeds_spawned();
    let started = Instant::now();
    let transcript = session(
        &directory,
        &format!(
            "{HELLO}\n{}\n{}\n",
            r#"{"jsonrpc":"2.0","id":2,"method":"resources/subscribe","params":{"uri":"fsm://instance/inst-1"}}"#,
            r#"{"jsonrpc":"2.0","id":3,"method":"resources/subscribe","params":{"uri":"fsm://instance/inst-2"}}"#
        ),
    );
    // The session returned, which means the join completed.
    assert_eq!(
        feeds_spawned(),
        before + 1,
        "one feed per session, however many subscriptions"
    );
    assert!(
        started.elapsed() < std::time::Duration::from_millis(2_000),
        "shutdown waited for a full poll interval: {:?}",
        started.elapsed()
    );
    assert_eq!(transcript.lines().count(), 3);
}

#[test]
fn a_dropped_handle_still_joins_its_thread() {
    let _counting = counting();
    let flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let observed = std::sync::Arc::clone(&flag);
    {
        let _handle = FeedHandle::spawn(move |stop| {
            while !stop.load(Ordering::Relaxed) {
                sleep_unless_stopped(stop, 250);
            }
            observed.store(true, Ordering::Relaxed);
        });
        // Dropped here without an explicit stop.
    }
    assert!(
        flag.load(Ordering::Relaxed),
        "Drop is what makes an early return safe"
    );
}

#[test]
fn a_stop_is_honoured_well_inside_one_interval() {
    let _counting = counting();
    let started = Instant::now();
    let mut handle = FeedHandle::spawn(|stop| {
        while !stop.load(Ordering::Relaxed) {
            // A full interval, slept in slices.
            sleep_unless_stopped(stop, 5_000);
        }
    });
    std::thread::sleep(std::time::Duration::from_millis(10));
    handle.stop_and_join();
    assert!(
        started.elapsed() < std::time::Duration::from_millis(1_000),
        "a sleep that ignores the flag turns a disconnect into a stall: {:?}",
        started.elapsed()
    );
}

/// A writer that fails, so a feed meets a closed pipe.
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
fn a_broken_stream_ends_the_feed_without_a_panic() {
    let _counting = counting();
    let notifier = Notifier::new(Box::new(Broken));
    assert!(notifier.send(&Value::Obj(BTreeMap::new())).is_err());
    let writer = notifier.clone_handle();
    let finished = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let observed = std::sync::Arc::clone(&finished);
    let mut handle = FeedHandle::spawn(move |stop| {
        while !stop.load(Ordering::Relaxed) {
            if writer.is_broken() {
                observed.store(true, Ordering::Relaxed);
                return;
            }
            sleep_unless_stopped(stop, 250);
        }
    });
    std::thread::sleep(std::time::Duration::from_millis(20));
    handle.stop_and_join();
    assert!(
        finished.load(Ordering::Relaxed),
        "the feed noticed the closed pipe and stopped itself"
    );
}

#[test]
fn two_sequential_sessions_each_spawn_and_join_their_own() {
    let _counting = counting();
    let directory = TestDirectory::create();
    let before = feeds_spawned();
    for _ in 0..2 {
        session(
            &directory,
            &format!(
                "{HELLO}\n{}\n",
                r#"{"jsonrpc":"2.0","id":2,"method":"resources/subscribe","params":{"uri":"fsm://instance/inst-1"}}"#
            ),
        );
    }
    assert_eq!(
        feeds_spawned(),
        before + 2,
        "one each, with no interference"
    );
}

#[test]
fn a_non_subscribing_transcript_is_what_it_always_was() {
    let _counting = counting();
    let directory = TestDirectory::create();
    let transcript = session(
        &directory,
        &format!(
            "{HELLO}\n{}\n",
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#
        ),
    );
    for line in transcript.lines() {
        let parsed = parse(line.as_bytes(), &JsonLimits::DEFAULT).expect("one message per line");
        assert!(
            parsed.get("method").is_none(),
            "a session nobody subscribed to says nothing on its own: {line}"
        );
    }
}
