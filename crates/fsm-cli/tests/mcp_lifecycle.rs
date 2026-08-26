use std::io::Cursor;
use std::process::Command;

use fsm_cli::clock::SystemClock;
use fsm_cli::mcp::serve::{rpc_error, serve_session, tool_error};
use fsm_cli::store::{ErrorObj, Store};
use fsm_core::json::Value;

static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

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

fn tmp() -> Scratch {
    let n = N.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let p = std::env::temp_dir().join(format!("fsm-life-{}-{}", std::process::id(), n));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    Scratch(p)
}

fn run(input: &str) -> String {
    let _g = LOCK.lock().unwrap();
    let dir = tmp();
    let mut store = Store::open(&dir).unwrap();
    let mut clock = SystemClock;
    let mut out = Vec::new();
    serve_session(
        Some(&mut store),
        &mut clock,
        Cursor::new(input.as_bytes()),
        &mut out,
    )
    .unwrap();
    String::from_utf8(out).unwrap()
}

#[test]
fn negotiate_table() {
    for (offer, want) in [
        ("2025-03-26", "2025-03-26"),
        ("2024-11-05", "2024-11-05"),
        ("2025-11-25", "2025-06-18"),
        ("9999-01-01", "2025-06-18"),
    ] {
        let input = format!(
            r#"{{"jsonrpc":"2.0","id":1,"method":"initialize","params":{{"protocolVersion":"{offer}"}}}}"#
        );
        let got = run(&input);
        assert!(
            got.contains(&format!("\"protocolVersion\":\"{want}\"")),
            "{got}"
        );
    }
}

#[test]
fn gate_batch_cancel_dup_id() {
    let before = concat!(r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#, "\n",);
    let got = run(before);
    assert!(got.contains("-32002"), "{got}");

    let batch = concat!(
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05"}}"#,
        "\n",
        r#"[{"jsonrpc":"2.0","id":2,"method":"ping"}]"#,
        "\n",
    );
    let got = run(batch);
    assert!(got.contains("-32600"), "{got}");

    let cancel = concat!(
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}"#,
        "\n",
        r#"{"jsonrpc":"2.0","method":"notifications/cancelled"}"#,
        "\n",
        r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#,
        "\n",
    );
    let got = run(cancel);
    let lines: Vec<_> = got.lines().collect();
    assert_eq!(lines.len(), 2, "{got}");
}

#[test]
fn eof_after_initialize() {
    let input = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}"#;
    let out = run(input);
    assert!(out.ends_with('\n'));
}

#[test]
fn error_helpers() {
    let rpc = rpc_error(
        Value::Num("1".into()),
        -32600,
        "batch requests are not supported",
    );
    assert!(rpc.get("error").is_some());
    let err = ErrorObj::new("run/not_enabled", "no");
    let te = tool_error(&err);
    assert_eq!(te.get("isError").and_then(Value::as_bool), Some(true));
    let sc = te.get("structuredContent").unwrap();
    let err = sc.get("error").expect("structuredContent.error envelope");
    for k in [
        "code",
        "message",
        "path",
        "hint",
        "retryable",
        "duplicate",
        "details",
        "docs",
    ] {
        assert!(err.get(k).is_some(), "missing {k}");
    }
    assert_eq!(err.get("duplicate").and_then(Value::as_bool), Some(false));
    let text = te
        .get("content")
        .and_then(Value::as_arr)
        .and_then(|a| a.first())
        .and_then(|i| i.get("text"))
        .and_then(Value::as_str)
        .unwrap();
    assert_eq!(text, fsm_cli::render::render_human(sc));
}

#[test]
fn panic_reexec() {
    if std::env::var("FSM_MCP_PANIC").ok().as_deref() == Some("1") {
        return;
    }
    let exe = std::env::current_exe().unwrap();
    let out = Command::new(exe)
        .env("FSM_MCP_PANIC", "1")
        .env("RUST_BACKTRACE", "0")
        .args(["--exact", "eof_after_initialize"])
        .output()
        .unwrap();
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("fsm panic"), "{err}");
    assert!(
        err.contains("serve_session") || err.contains("backtrace") || err.contains("fsm-cli"),
        "{err}"
    );
    assert!(!out.status.success());
}

#[test]
fn request_after_initialize_before_initialized_warns() {
    if std::env::var("FSM_MCP_EARLY").ok().as_deref() == Some("1") {
        let input = concat!(
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}"#,
            "\n",
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
            "\n",
        );
        let got = run(input);
        assert!(got.contains("machine_create"), "{got}");
        return;
    }
    let exe = std::env::current_exe().unwrap();
    let out = Command::new(exe)
        .env("FSM_MCP_EARLY", "1")
        .args([
            "--exact",
            "request_after_initialize_before_initialized_warns",
        ])
        .output()
        .unwrap();
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("before notifications/initialized"), "{err}");
    assert!(out.status.success(), "{err}");
}

#[test]
fn initialized_notification_then_tools() {
    let input = concat!(
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}"#,
        "\n",
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
        "\n",
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
        "\n",
    );
    let got = run(input);
    assert!(got.contains("machine_create"), "{got}");
}

#[test]
fn capabilities_shape() {
    let got = run(
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}"#,
    );
    assert!(got.contains("\"listChanged\":false"));
    assert!(got.contains("\"subscribe\":false"));
    assert!(got.contains("instructions"));
}
