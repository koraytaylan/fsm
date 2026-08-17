//! Byte-exact MCP skeleton transcripts and in-memory edge cases.

use std::io::Cursor;

use fsm_cli::clock::SystemClock;
use fsm_cli::mcp::serve::serve_session;
use fsm_cli::store::Store;

fn run(input: &str) -> String {
    let dir = std::env::temp_dir().join(format!(
        "fsm-skel-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
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

fn assert_hygiene(output: &str) {
    for line in output.split_inclusive('\n') {
        if line.is_empty() {
            continue;
        }
        assert!(line.ends_with('\n'), "response missing trailing newline");
        assert_eq!(line.as_bytes().iter().filter(|&&b| b == b'\n').count(), 1);
        assert!(!line[..line.len() - 1].contains('\n'));
    }
}

#[test]
fn skeleton_transcript() {
    let input = include_str!("fixtures/transcripts/skeleton.in.jsonl");
    let expected = include_str!("fixtures/transcripts/skeleton.out.jsonl");
    let got = run(input);
    assert_eq!(got, expected);
    assert_hygiene(&got);
}

#[test]
fn skeleton_echo_transcript() {
    let input = include_str!("fixtures/transcripts/skeleton_echo.in.jsonl");
    let expected = include_str!("fixtures/transcripts/skeleton_echo.out.jsonl");
    let got = run(input);
    assert_eq!(got, expected);
    assert_hygiene(&got);
}

#[test]
fn line_over_cap_names_cap() {
    let huge = "x".repeat(16 * 1024 * 1024 + 1);
    let got = run(&(huge + "\n"));
    assert!(got.contains("-32700"), "{got}");
    assert!(got.contains("16777216"), "{got}");
}

#[test]
fn eof_after_initialize_is_ok() {
    let input = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"t","version":"0"}}}"#;
    let out = run(input);
    assert!(!out.is_empty());
    assert!(out.ends_with('\n'));
}

#[test]
fn unknown_notification_is_silent_and_loop_continues() {
    let input = concat!(
        r#"{"jsonrpc":"2.0","method":"nope"}"#,
        "\n",
        r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#,
        "\n",
    );
    let got = run(input);
    assert_eq!(got, "{\"id\":1,\"jsonrpc\":\"2.0\",\"result\":{}}\n");
}

#[test]
fn echo_versions_in_fresh_sessions() {
    for ver in ["2025-03-26", "2024-11-05"] {
        let input = format!(
            r#"{{"jsonrpc":"2.0","id":1,"method":"initialize","params":{{"protocolVersion":"{ver}","capabilities":{{}},"clientInfo":{{"name":"t","version":"0"}}}}}}"#
        );
        let got = run(&input);
        assert!(
            got.contains(&format!("\"protocolVersion\":\"{ver}\"")),
            "{got}"
        );
    }
}
