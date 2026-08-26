//! Byte-exact full-session MCP transcripts per negotiated revision.

use std::io::Cursor;

use fsm_cli::clock::{self, FixedClock};
use fsm_cli::mcp::serve::serve_session;
use fsm_cli::mcp::tools::names;
use fsm_cli::store::Store;

/// A scratch directory that removes itself.
///
/// Every temp directory a test makes has to be given back: a suite that
/// leaks one per run exhausts a long-lived machine's tmpfs inodes long
/// before it exhausts its bytes, and the failure looks like a broken
/// toolchain rather than a leaky test.
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

impl AsRef<std::ffi::OsStr> for Scratch {
    fn as_ref(&self) -> &std::ffi::OsStr {
        self.0.as_os_str()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Per-process counter. Tests in one binary run concurrently, and a timestamp
/// alone can collide between two threads building a path together.
static TMP_N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn drive(input: &str) -> String {
    let _g = LOCK.lock().unwrap();
    let dir = std::env::temp_dir().join(format!(
        "fsm-full-{}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        TMP_N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let dir = Scratch(dir);
    clock::reset_injected();
    // Genesis/lock timestamps are not MCP-dispatched; pin only the open path so
    // two sessions stay byte-identical. Tool appends use `clk` below.
    clock::force_ms(1_000);
    clock::set_step(1);
    let mut store = Store::open(&dir).unwrap();
    clock::reset_injected();
    let mut clk = FixedClock::new(1_000, 1);
    let sink = fsm_cli::mcp::notify::SharedSink::new();
    serve_session(
        Some(&mut store),
        &mut clk,
        Cursor::new(input.as_bytes()),
        sink.writer(),
    )
    .unwrap();
    sink.text()
}

fn assert_transcript(ver: &str) {
    let input = match ver {
        "2025-06-18" => include_str!("fixtures/transcripts/full_2025-06-18.in.jsonl"),
        "2025-03-26" => include_str!("fixtures/transcripts/full_2025-03-26.in.jsonl"),
        "2024-11-05" => include_str!("fixtures/transcripts/full_2024-11-05.in.jsonl"),
        _ => unreachable!(),
    };
    let expected = match ver {
        "2025-06-18" => include_str!("fixtures/transcripts/full_2025-06-18.out.jsonl"),
        "2025-03-26" => include_str!("fixtures/transcripts/full_2025-03-26.out.jsonl"),
        "2024-11-05" => include_str!("fixtures/transcripts/full_2024-11-05.out.jsonl"),
        _ => unreachable!(),
    };
    let got = drive(input);
    if std::env::var("REGEN_MCP_FULL").ok().as_deref() == Some("1") {
        let p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
            "tests/fixtures/transcripts/full_{}.out.jsonl",
            ver.replace('-', "-")
        ));
        std::fs::write(p, &got).unwrap();
        return;
    }
    assert_eq!(got, expected, "transcript {ver}");
}

#[test]
fn eighteen_tools_in_order() {
    assert_eq!(
        names(),
        [
            "machine_create",
            "machine_list",
            "machine_get",
            "machine_analyze",
            "machine_diagram",
            "instance_create",
            "instance_send",
            "deadline_poll",
            "effect_ack",
            "instance_cancel",
            "instance_migrate",
            "invocation_start",
            "invocation_return",
            "signal_deliver",
            "instance_get",
            "instance_list",
            "instance_history",
            "simulate",
        ]
    );
}

#[test]
fn full_2025_06_18() {
    assert_transcript("2025-06-18");
}

#[test]
fn full_2025_03_26() {
    assert_transcript("2025-03-26");
}

#[test]
fn full_2024_11_05() {
    assert_transcript("2024-11-05");
}

#[test]
fn cross_revision_invariance() {
    let a = include_str!("fixtures/transcripts/full_2025-06-18.out.jsonl");
    let b = include_str!("fixtures/transcripts/full_2025-03-26.out.jsonl")
        .replace("2025-03-26", "2025-06-18");
    let c = include_str!("fixtures/transcripts/full_2024-11-05.out.jsonl")
        .replace("2024-11-05", "2025-06-18");
    assert_eq!(a, b);
    assert_eq!(a, c);
}

#[test]
fn determinism_twice() {
    let input = include_str!("fixtures/transcripts/full_2025-06-18.in.jsonl");
    assert_eq!(drive(input), drive(input));
}
