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
fn nineteen_tools_in_order() {
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
            "explain_step",
            "journal_verify",
            "instance_elicit",
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

/// A model that creates a workflow gets a handle to it, and the handle is
/// readable in the same session.
///
/// Plan 0012 task 5802.
mod resource_links {
    use super::*;
    use fsm_core::json::{JsonLimits, Value, parse};

    /// The three lines every session starts with.
    fn hello() -> String {
        [
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}"#,
            r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
        ]
        .join("\n")
    }

    /// One `tools/call` line.
    fn call(name: &str, arguments: &str) -> String {
        format!(
            r#"{{"jsonrpc":"2.0","id":{},"method":"tools/call","params":{{"name":"{name}","arguments":{arguments}}}}}"#,
            NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        )
    }

    static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(100);

    /// A one-line machine to create instances of.
    fn machine_line() -> String {
        r#"{"format":"fsm.machine/1","name":"flow","states":[{"name":"idle"},{"name":"done","terminal":true}],"initial":"idle","context":[],"events":[{"name":"go","fields":[]}],"transitions":[{"from":"idle","on":"go","to":"done"}]}"#.to_string()
    }

    /// Every `tools/call` result in a transcript, paired with the tool name
    /// that produced it.
    fn results(transcript: &str) -> Vec<Value> {
        transcript
            .lines()
            .filter_map(|line| parse(line.as_bytes(), &JsonLimits::DEFAULT).ok())
            .filter_map(|message| message.get("result").cloned())
            .filter(|result| result.get("content").is_some())
            .collect()
    }

    fn link_of(result: &Value) -> Option<Value> {
        result
            .get("content")
            .and_then(Value::as_arr)?
            .iter()
            .find(|entry| entry.get("type").and_then(Value::as_str) == Some("resource_link"))
            .cloned()
    }

    #[test]
    fn an_instance_tool_links_to_the_instance_it_acted_on() {
        let transcript = drive(&format!(
            "{}\n{}\n{}\n{}\n",
            hello(),
            call(
                "machine_create",
                &format!(r#"{{"spec":{}}}"#, machine_line())
            ),
            call(
                "instance_create",
                r#"{"machine":"flow","request_id":"link-1"}"#
            ),
            call("instance_get", r#"{"instance_id":"inst-link-1"}"#),
        ));
        let calls = results(&transcript);
        // machine_create names no instance; the other two do.
        let creation = &calls[1];
        let link = link_of(creation).expect("instance_create links");
        assert_eq!(
            link.get("uri").and_then(Value::as_str),
            Some("fsm://instance/inst-link-1")
        );
        assert_eq!(
            link.get("mimeType").and_then(Value::as_str),
            Some("application/json")
        );
        // The linked id is the one in the structured result, not the one in
        // the arguments — `instance_create` was given a request id, not an
        // instance id.
        assert_eq!(
            link.get("name").and_then(Value::as_str),
            creation
                .get("structuredContent")
                .and_then(|structured| structured.get("instance_id"))
                .and_then(Value::as_str)
        );
        assert!(link_of(&calls[2]).is_some(), "instance_get links too");
        assert!(
            link_of(&calls[0]).is_none(),
            "machine_create names no instance"
        );
    }

    #[test]
    fn a_link_is_readable_in_the_same_session() {
        let transcript = drive(&format!(
            "{}\n{}\n{}\n{}\n",
            hello(),
            call(
                "machine_create",
                &format!(r#"{{"spec":{}}}"#, machine_line())
            ),
            call(
                "instance_create",
                r#"{"machine":"flow","request_id":"link-2"}"#
            ),
            r#"{"jsonrpc":"2.0","id":9,"method":"resources/read","params":{"uri":"fsm://instance/inst-link-2"}}"#,
        ));
        let read = transcript
            .lines()
            .filter_map(|line| parse(line.as_bytes(), &JsonLimits::DEFAULT).ok())
            .find(|message| message.get("id").and_then(Value::as_num) == Some("9"))
            .expect("the read answered");
        assert!(
            read.get("error").is_none(),
            "a link the server hands out has to resolve: {read:?}"
        );
    }

    #[test]
    fn a_listing_and_a_failure_carry_no_link() {
        let transcript = drive(&format!(
            "{}\n{}\n{}\n{}\n",
            hello(),
            call(
                "machine_create",
                &format!(r#"{{"spec":{}}}"#, machine_line())
            ),
            call("instance_list", "{}"),
            call(
                "instance_send",
                r#"{"instance_id":"inst-nosuch","event":{"name":"go"},"request_id":"link-3"}"#
            ),
        ));
        let calls = results(&transcript);
        assert!(
            link_of(&calls[1]).is_none(),
            "a list result would carry N links and bury the text"
        );
        let failure = &calls[2];
        assert_eq!(failure.get("isError").and_then(Value::as_bool), Some(true));
        assert!(
            link_of(failure).is_none(),
            "a link on a failure invites a read that returns not-found"
        );
    }
}
