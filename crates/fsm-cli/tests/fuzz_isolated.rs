//! Isolated fuzz of JSON-RPC, expressions, and record hashes.

use std::io::Cursor;

use fsm_cli::clock::{self, SystemClock};
use fsm_cli::mcp::serve::serve_session;
use fsm_cli::store::Store;
use fsm_core::expr::parser::parse as parse_expr;
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::record::{Record, seal};

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
    let p = std::env::temp_dir().join(format!(
        "fsm-fuzz-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&p).unwrap();
    Scratch(p)
}

fn case() -> Value {
    parse(
        include_bytes!("../../fsm-core/tests/fixtures/machines/case_review.json"),
        &JsonLimits::DEFAULT,
    )
    .unwrap()
}

#[test]
fn isolated_jsonrpc_expr_record_fuzz() {
    clock::reset_injected();
    clock::force_ms(9_000);
    clock::set_step(1);
    let dir = tmp();
    assert!(!dir.starts_with(fsm_cli::args::default_data_dir()));
    let mut store = Store::open(&dir).unwrap();
    store.define_machine(case(), false, false).unwrap();

    let lines = [
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}"#,
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
        r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"machine_list","arguments":[]}}"#,
        r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"machine_list","arguments":{}}}"#,
        r#"{not json"#,
        r#"{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"instance_send","arguments":{"instance_id":"nope","event":{"name":"x"},"request_id":"f1"}}}"#,
    ];
    let input = lines.join("\n") + "\n";
    let mut out = Vec::new();
    let mut clock = SystemClock;
    serve_session(
        Some(&mut store),
        &mut clock,
        Cursor::new(input.as_bytes()),
        &mut out,
    )
    .unwrap();
    let text = String::from_utf8(out).unwrap();
    for line in text.lines() {
        if line.is_empty() {
            continue;
        }
        parse(line.as_bytes(), &JsonLimits::DEFAULT)
            .unwrap_or_else(|e| panic!("invalid emitted JSON {line}: {e:?}"));
    }

    for src in ["1 +", "if true then 1", "ctx.missing > 0", "((((1"] {
        match parse_expr(src) {
            Ok(_) => {}
            Err(e) => {
                assert!(e.span.end >= e.span.start, "{src} {e:?}");
                assert!(!e.code.is_empty());
            }
        }
    }

    let recs = fsm_cli::journal_io::load_records(&dir).unwrap();
    assert!(!recs.is_empty());
    for rec in &recs {
        let sealed = seal(rec.seq, rec.ts, rec.kind, rec.body.clone(), &rec.prev);
        assert_eq!(sealed.hash, rec.hash, "seq {}", rec.seq);
        let _ = rec as &Record;
    }
    if let Some(rec) = recs.first() {
        let mut body = rec.body.clone();
        if let Value::Obj(o) = &mut body {
            o.insert("fuzz_extra".into(), Value::Bool(true));
        }
        let resealed = seal(rec.seq, rec.ts, rec.kind, body, &rec.prev);
        assert_ne!(resealed.hash, rec.hash);
    }
}
