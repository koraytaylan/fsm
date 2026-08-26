//! Isolated driver of the real `fuzz/fuzz_targets` subjects.

use fsm_cli::clock::{self, FixedClock};
use fsm_cli::mcp::serve::serve_session;
use fsm_cli::store::Store;
use fsm_core::canon::canon_bytes;
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::record::{RecordKind, verify_line, zeros};
use fsm_core::sha256::{sha256, to_hex};
use std::collections::BTreeMap;
use std::io::Cursor;
use std::path::Path;

fn independent_record_hash(
    seq: u64,
    ts: i64,
    kind: RecordKind,
    body: &Value,
    prev: &str,
) -> String {
    let mut m = BTreeMap::new();
    m.insert("seq".into(), Value::Num(seq.to_string()));
    m.insert("ts".into(), Value::Num(ts.to_string()));
    m.insert("kind".into(), Value::Str(kind.as_str().into()));
    m.insert("body".into(), body.clone());
    m.insert("prev".into(), Value::Str(prev.into()));
    let mut buf = b"fsm:record:1".to_vec();
    buf.push(0x0A);
    buf.extend_from_slice(&canon_bytes(&Value::Obj(m)));
    to_hex(&sha256(&buf))
}

fn read_target(name: &str) -> String {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../../fuzz/fuzz_targets/{name}"));
    std::fs::read_to_string(p).unwrap()
}

#[test]
fn isolated_fuzz_targets_driver() {
    let record_src = read_target("record_line.rs");
    assert!(
        !record_src.contains("record::seal(") && !record_src.contains("fsm_core::record::seal("),
        "record_line must not reseal with production seal"
    );
    assert!(
        record_src.contains("sha256")
            && record_src.contains("fsm:record:1")
            && record_src.contains("canon_bytes"),
        "record_line must recompute hash from domain tag, canonical bytes, and SHA"
    );
    assert!(
        !record_src.contains("domain_hash"),
        "record_line must not reuse production domain_hash"
    );
    for name in [
        "jsonrpc_loop.rs",
        "expr_parse.rs",
        "record_line.rs",
        "json_parse.rs",
        "decimal_parse.rs",
        "canon_roundtrip.rs",
    ] {
        assert!(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join(format!("../../fuzz/fuzz_targets/{name}"))
                .is_file(),
            "missing shipped target {name}"
        );
    }

    let rec = fsm_core::record::seal(
        0,
        1,
        RecordKind::Genesis,
        Value::Obj(BTreeMap::from([
            ("format".into(), Value::Str("fsm.journal/1".into())),
            ("created_ts".into(), Value::Num("1".into())),
            ("limits".into(), fsm_core::record::limits_value()),
        ])),
        &zeros(),
    );
    let line = rec.to_line();
    let parsed = verify_line(&line, 0, &zeros()).unwrap();
    let independent = independent_record_hash(
        parsed.seq,
        parsed.ts,
        parsed.kind,
        &parsed.body,
        &parsed.prev,
    );
    assert_eq!(independent, parsed.hash);
    let mut extra = parsed.body.clone();
    if let Value::Obj(o) = &mut extra {
        o.insert("x".into(), Value::Str("y".into()));
    }
    let other = independent_record_hash(parsed.seq, parsed.ts, parsed.kind, &extra, &parsed.prev);
    assert_ne!(other, parsed.hash, "extra key must change independent hash");

    for src in ["ctx.n + 1", "1 < 2 < 3", "@@@", ""] {
        match fsm_core::expr::parser::parse(src) {
            Ok(e) => {
                assert!(fsm_core::expr::ast::node_count(&e) <= 512);
                assert!(fsm_core::expr::ast::depth(&e) <= 32);
            }
            Err(e) => {
                assert!(e.span.start <= e.span.end);
                assert!(e.span.end as usize <= src.len());
                assert!(!e.code.is_empty());
            }
        }
    }

    clock::reset_injected();
    let mut store = Store::open_memory().unwrap();
    let mut clk = FixedClock::new(1, 0);
    let req = br#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"t","version":"0"}}}"#;
    let sink = fsm_cli::mcp::notify::SharedSink::new();
    let _ = serve_session(
        Some(&mut store),
        &mut clk,
        Cursor::new(&req[..]),
        sink.writer(),
    );
    drop(store);
    let out = sink.bytes();
    for line in out.split(|&b| b == b'\n') {
        if line.is_empty() {
            continue;
        }
        assert!(
            parse(line, &JsonLimits::DEFAULT).is_ok(),
            "invalid JSON {}",
            String::from_utf8_lossy(line)
        );
    }
}
