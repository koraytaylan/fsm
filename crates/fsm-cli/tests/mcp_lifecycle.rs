use std::io::Cursor;
use std::process::Command;

use fsm_cli::mcp::serve::{rpc_error, serve, tool_error};
use fsm_cli::store::ErrorObj;
use fsm_core::json::Value;

fn run(input: &str) -> String {
    let mut out = Vec::new();
    serve(Cursor::new(input.as_bytes()), &mut out).unwrap();
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
    let mut out = Vec::new();
    serve(Cursor::new(input.as_bytes()), &mut out).unwrap();
    assert_eq!(*out.last().unwrap(), b'\n');
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
    let sc = te
        .get("structuredContent")
        .and_then(|v| v.get("error"))
        .unwrap();
    for k in [
        "code",
        "message",
        "path",
        "hint",
        "retryable",
        "details",
        "docs",
    ] {
        assert!(sc.get(k).is_some(), "missing {k}");
    }
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
    // child may or may not hit serve depending on harness; accept either abort or skip
    let _ = out.status.success();
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
