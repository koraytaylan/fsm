//! One live session, byte for byte — notifications, progress, and silences.
//!
//! A push surface is only trustworthy if its whole stream is compared, and
//! the only way to compare a timer-driven stream is to take the timer out of
//! it. The feed here is driven through the same `poll_once` its timer would
//! call, at points this suite picks, so the bytes are the bytes a real
//! session writes and no wall time is spent waiting for them.
//!
//! One companion test does spend wall time, on purpose and once: it starts
//! the real timer and asserts that a notification arrives *at all*. Never
//! when, never how many. The golden proves the bytes; that test proves the
//! wire is live.
//!
//! Plan 0012 task 6101.

use std::collections::BTreeMap;
use std::io::{BufRead, Read};

use fsm_cli::clock::{self, FixedClock};
use fsm_cli::mcp::notify::SharedSink;
use fsm_cli::mcp::serve::serve_session;
use fsm_cli::mcp::watch;
use fsm_cli::store::Store;
use fsm_core::json::{JsonLimits, Value, parse};

/// A scratch directory that removes itself. A suite that leaks one per run
/// exhausts a machine's inodes long before its bytes, and the failure looks
/// like a broken toolchain rather than a leaky test.
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

static TMP_N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// The injected clock is process-global, so the two byte-exact sessions take
/// turns. The timing test does not need it and does not take it.
static GOLDEN: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn scratch(tag: &str) -> Scratch {
    let dir = std::env::temp_dir().join(format!(
        "fsm-live-{tag}-{}-{}",
        std::process::id(),
        TMP_N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    Scratch(dir)
}

/// Lines handed to the server one at a time, with a hook that runs after each
/// one is answered.
///
/// The server blocks reading the next line, so a hook running here runs while
/// nothing else can write: whatever it emits lands between two responses, in
/// one place, every time.
struct Scripted<F: FnMut(usize)> {
    lines: Vec<Vec<u8>>,
    next: usize,
    buf: Vec<u8>,
    pos: usize,
    after: F,
}

impl<F: FnMut(usize)> Scripted<F> {
    fn new(lines: &[String], after: F) -> Self {
        Self {
            lines: lines
                .iter()
                .map(|line| format!("{line}\n").into_bytes())
                .collect(),
            next: 0,
            buf: Vec::new(),
            pos: 0,
            after,
        }
    }
}

impl<F: FnMut(usize)> BufRead for Scripted<F> {
    fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
        if self.pos == self.buf.len() {
            if self.next > 0 {
                (self.after)(self.next - 1);
            }
            if self.next == self.lines.len() {
                return Ok(&[]);
            }
            self.buf = self.lines[self.next].clone();
            self.pos = 0;
            self.next += 1;
        }
        Ok(&self.buf[self.pos..])
    }

    fn consume(&mut self, amount: usize) {
        self.pos = (self.pos + amount).min(self.buf.len());
    }
}

impl<F: FnMut(usize)> Read for Scripted<F> {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        let available = self.fill_buf()?;
        let n = available.len().min(out.len());
        out[..n].copy_from_slice(&available[..n]);
        self.consume(n);
        Ok(n)
    }
}

const CASE: &str = r#"{"format":"fsm.machine/1","name":"live_case","states":[{"name":"intake"},{"name":"review"},{"name":"closed","terminal":true}],"initial":"intake","context":[],"events":[{"name":"submit","fields":[]},{"name":"approve","fields":[]}],"transitions":[{"from":"intake","on":"submit","to":"review"},{"from":"review","on":"approve","to":"closed"}]}"#;

fn value(source: &str) -> Value {
    parse(source.as_bytes(), &JsonLimits::DEFAULT).unwrap()
}

/// A store holding the machine and one instance, at a pinned clock so two
/// runs of this suite journal the same bytes.
fn seeded(dir: &Scratch) -> Store {
    clock::reset_injected();
    clock::force_ms(1_000);
    clock::set_step(0);
    let mut store = Store::open(dir).unwrap();
    let mut clk = FixedClock::new(1_000, 0);
    store
        .define_machine_on(&mut clk, value(CASE), false, false)
        .unwrap();
    store
        .create_instance_ctx_on(
            &mut clk,
            "live_case",
            "inst-live",
            "seed",
            None,
            &BTreeMap::new(),
            &[],
        )
        .unwrap();
    clock::reset_injected();
    store
}

/// The session step 2 of the task describes, in order.
fn live_session_lines() -> Vec<String> {
    vec![
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"golden","version":"1"}}}"#.into(),
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#.into(),
        r#"{"jsonrpc":"2.0","id":2,"method":"resources/subscribe","params":{"uri":"fsm://instance/inst-live"}}"#.into(),
        r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"instance_send","arguments":{"instance_id":"inst-live","event":{"name":"submit"},"request_id":"golden-1"}}}"#.into(),
        r#"{"jsonrpc":"2.0","id":4,"method":"resources/read","params":{"uri":"fsm://instance/inst-live"}}"#.into(),
        r#"{"jsonrpc":"2.0","id":5,"method":"logging/setLevel","params":{"level":"debug"}}"#.into(),
        r#"{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"simulate","arguments":{"machine":"live_case","events":[{"name":"submit"},{"name":"approve"}]},"_meta":{"progressToken":"golden-progress"}}}"#.into(),
        r#"{"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":7,"reason":"the client changed its mind"}}"#.into(),
        r#"{"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"instance_get","arguments":{"instance_id":"inst-live"}}}"#.into(),
        r#"{"jsonrpc":"2.0","id":8,"method":"resources/unsubscribe","params":{"uri":"fsm://instance/inst-live"}}"#.into(),
    ]
}

/// A session that watches nothing: the same server, none of the new stream.
fn quiet_session_lines() -> Vec<String> {
    vec![
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"golden","version":"1"}}}"#.into(),
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#.into(),
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"instance_send","arguments":{"instance_id":"inst-live","event":{"name":"submit"},"request_id":"quiet-1"}}}"#.into(),
        r#"{"jsonrpc":"2.0","id":3,"method":"resources/read","params":{"uri":"fsm://instance/inst-live"}}"#.into(),
    ]
}

/// Drive one hand-fed session and return everything it wrote.
///
/// `poll_after` names the line index after which the feed's pass runs — the
/// write, in the live session. Every other line is followed by a pass too, so
/// the golden also records that the feed has nothing more to say; a feed that
/// spoke twice about one change would show up here as an extra line.
fn drive(store: &mut Store, lines: &[String]) -> String {
    let sink = SharedSink::new();
    let by_hand = watch::ByHand::arm();
    let mut clk = FixedClock::new(1_000, 0);
    let input = Scripted::new(lines, |_| {
        by_hand.poll();
    });
    serve_session(Some(store), &mut clk, input, sink.writer()).unwrap();
    sink.text()
}

fn golden(name: &str) -> String {
    std::fs::read_to_string(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/mcp_live")
            .join(name),
    )
    .unwrap_or_default()
}

fn regen(name: &str, text: &str) {
    std::fs::write(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/mcp_live")
            .join(name),
        text,
    )
    .unwrap();
}

fn regenerating() -> bool {
    std::env::var("REGEN_MCP_LIVE").ok().as_deref() == Some("1")
}

/// What each written line is, in one word: a reply to id N, or a notification
/// by method. This is the shape the specification and the architecture fix,
/// derived by hand below and compared against whatever the fixture holds —
/// so a fixture that drifts toward a self-consistent implementation still
/// fails.
fn shape(stream: &str) -> Vec<String> {
    stream
        .lines()
        .map(|line| {
            let message = parse(line.as_bytes(), &JsonLimits::DEFAULT).expect("a JSON line");
            match message.get("method").and_then(Value::as_str) {
                Some(method) => method.to_string(),
                None => format!(
                    "reply:{}",
                    message
                        .get("id")
                        .and_then(|id| id.as_num().or_else(|| id.as_str()))
                        .unwrap_or("?")
                ),
            }
        })
        .collect()
}

#[test]
fn a_live_session_writes_exactly_these_bytes() {
    let _guard = GOLDEN.lock().unwrap_or_else(|e| e.into_inner());
    let dir = scratch("golden");
    let mut store = seeded(&dir);
    let stream = drive(&mut store, &live_session_lines());

    // Hand-derived: what the specification says this session produces, in
    // order. Two requests are answered by silence — the cancelled id 7, and
    // both notifications, which are notifications precisely because nothing
    // answers them.
    assert_eq!(
        shape(&stream),
        vec![
            // initialize
            "reply:1",
            // resources/subscribe
            "reply:2",
            // the write, then the feed's pass over it
            "reply:3",
            "notifications/resources/updated",
            // resources/read
            "reply:4",
            // logging/setLevel
            "reply:5",
            // simulate, reporting progress per rendered step, then its result
            "notifications/progress",
            "notifications/progress",
            "reply:6",
            // the cancellation arrives: logged at the level just set, and
            // then the request it names is skipped without a reply
            "notifications/message",
            "notifications/message",
            // resources/unsubscribe
            "reply:8",
        ],
        "the stream was:\n{stream}"
    );

    if regenerating() {
        regen("session.expected", &stream);
        return;
    }
    assert_eq!(stream, golden("session.expected"));
}

#[test]
fn the_same_session_twice_writes_the_same_bytes() {
    let _guard = GOLDEN.lock().unwrap_or_else(|e| e.into_inner());
    let first = {
        let dir = scratch("twice-a");
        let mut store = seeded(&dir);
        drive(&mut store, &live_session_lines())
    };
    let second = {
        let dir = scratch("twice-b");
        let mut store = seeded(&dir);
        drive(&mut store, &live_session_lines())
    };
    assert_eq!(first, second);
}

#[test]
fn the_fixture_leaks_nothing_about_the_machine_that_made_it() {
    let text = golden("session.expected");
    assert!(!text.is_empty(), "fixture missing");
    assert!(!text.contains("/tmp"), "an absolute path leaked");
    assert!(!text.contains("fsm-live-"), "a temp directory leaked");
    assert!(!text.contains(&std::process::id().to_string()) || text.contains("\"1\""));
    assert!(!text.contains('\r'), "a line ending leaked");
    // No instant, wall-clock or pinned: an ISO-8601 timestamp is `…9T09:…`,
    // which the negotiated protocol version (`2025-06-18`) is not.
    let bytes = text.as_bytes();
    for (index, byte) in bytes.iter().enumerate() {
        if *byte != b'T' || index == 0 || index + 3 >= bytes.len() {
            continue;
        }
        let instant = bytes[index - 1].is_ascii_digit()
            && bytes[index + 1].is_ascii_digit()
            && bytes[index + 2].is_ascii_digit()
            && bytes[index + 3] == b':';
        assert!(
            !instant,
            "a timestamp leaked at byte {index}: {}",
            &text[index.saturating_sub(24)..(index + 12).min(text.len())]
        );
    }
    assert!(
        text.contains("\"protocolVersion\":\"2025-06-18\""),
        "the only date-shaped run in the fixture is the negotiated version"
    );
}

#[test]
fn a_session_that_watches_nothing_says_nothing() {
    let _guard = GOLDEN.lock().unwrap_or_else(|e| e.into_inner());
    let dir = scratch("quiet");
    let mut store = seeded(&dir);
    let stream = drive(&mut store, &quiet_session_lines());
    assert_eq!(
        shape(&stream),
        vec!["reply:1", "reply:2", "reply:3"],
        "a session that subscribes to nothing gets replies and nothing else:\n{stream}"
    );
    if regenerating() {
        regen("quiet.expected", &stream);
        return;
    }
    assert_eq!(stream, golden("quiet.expected"));
}

#[test]
fn the_two_fixtures_differ_only_where_the_live_surface_speaks() {
    // The quiet session's replies are the pre-plan transcript: the same
    // server, answering the same way. Only `initialize` differs, because
    // capabilities are what this plan added to it.
    let quiet = golden("quiet.expected");
    let live = golden("session.expected");
    assert!(
        quiet.contains("\"resources\""),
        "capabilities are advertised"
    );
    assert!(
        !quiet.contains("notifications/"),
        "and nothing is pushed to a session that asked for nothing"
    );
    assert!(live.contains("notifications/resources/updated"));
    let quiet_initialize = quiet.lines().next().unwrap();
    let live_initialize = live.lines().next().unwrap();
    assert_eq!(
        quiet_initialize, live_initialize,
        "one initialize result, whatever the session does next"
    );
}

#[test]
fn a_real_feed_delivers_a_real_change() {
    // The one timing-tolerant test in this suite: the timer is real, the
    // write is real, and the only claim is that something arrived. Not when,
    // not how many — those are the golden's business, and asserting them
    // against a scheduler is how a suite starts failing on Tuesdays.
    let dir = scratch("timer");
    let mut store = {
        let mut store = Store::open(&dir).unwrap();
        let mut clk = FixedClock::new(1_000, 1);
        store
            .define_machine_on(&mut clk, value(CASE), false, false)
            .unwrap();
        store
            .create_instance_ctx_on(
                &mut clk,
                "live_case",
                "inst-live",
                "seed",
                None,
                &BTreeMap::new(),
                &[],
            )
            .unwrap();
        store
    };
    let sink = SharedSink::new();
    let lines = vec![
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}"#.to_string(),
        r#"{"jsonrpc":"2.0","id":2,"method":"resources/subscribe","params":{"uri":"fsm://instance/inst-live"}}"#.to_string(),
        r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"instance_send","arguments":{"instance_id":"inst-live","event":{"name":"submit"},"request_id":"timed-1"}}}"#.to_string(),
        r#"{"jsonrpc":"2.0","id":4,"method":"ping"}"#.to_string(),
    ];
    let watch_sink = sink.clone();
    let input = Scripted::new(&lines, move |index| {
        // After the write, wait for the feed to say something — bounded, so a
        // wire that is dead fails the test instead of hanging the suite.
        if index != 2 {
            return;
        }
        for _ in 0..200 {
            if watch_sink
                .text()
                .contains("notifications/resources/updated")
            {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
    });
    let mut clk = FixedClock::new(1_000, 1);
    serve_session(Some(&mut store), &mut clk, input, sink.writer()).unwrap();
    assert!(
        sink.text().contains("notifications/resources/updated"),
        "the timer-driven feed delivered nothing in five seconds:\n{}",
        sink.text()
    );
}
