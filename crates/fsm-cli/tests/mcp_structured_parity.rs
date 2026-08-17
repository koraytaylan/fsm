//! CLI `--json` fixtures byte-match MCP `structuredContent`.

use fsm_cli::clock::{self, FixedClock};
use fsm_cli::mcp::serve::tool_error;
use fsm_cli::mcp::tools::dispatch;
use fsm_cli::store::Store;
use fsm_core::canon::canon_bytes;
use fsm_core::json::{JsonLimits, Value, parse};
use std::collections::BTreeMap;
use std::path::PathBuf;

fn spec() -> Value {
    parse(
        include_bytes!("../../fsm-core/tests/fixtures/machines/case_review.json"),
        &JsonLimits::DEFAULT,
    )
    .unwrap()
}

fn obj(pairs: &[(&str, Value)]) -> Value {
    Value::Obj(
        pairs
            .iter()
            .map(|(k, v)| ((*k).into(), v.clone()))
            .collect(),
    )
}

fn fixture(name: &str) -> Vec<u8> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/structured")
        .join(name);
    let mut b = std::fs::read(&p).unwrap_or_else(|_| panic!("missing {name}"));
    if b.last() == Some(&b'\n') {
        b.pop();
    }
    b
}

fn assert_bytes(name: &str, got: &Value) {
    let want = fixture(name);
    let have = canon_bytes(got);
    assert_eq!(
        have,
        want,
        "{name}\n got {}\nwant {}",
        String::from_utf8_lossy(&have),
        String::from_utf8_lossy(&want)
    );
}

#[test]
fn every_structured_fixture_matches_tool() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/structured");
    let files: Vec<String> = std::fs::read_dir(&dir)
        .expect("structured dir")
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".json"))
        .collect();
    assert!(!files.is_empty(), "structured dir empty");

    clock::reset_injected();
    clock::force_ms(5_000);
    clock::set_step(1);
    let tmp = std::env::temp_dir().join(format!("fsm-par-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let mut store = Store::open(&tmp).unwrap();
    let mut clock = FixedClock::new(5_000, 1);

    let mut seen = BTreeMap::new();

    let v = dispatch(
        &mut store,
        &mut clock,
        "machine_create",
        &obj(&[("spec", spec()), ("dry_run", Value::Bool(true))]),
    )
    .unwrap();
    assert_bytes("validate.json", &v);
    seen.insert("validate.json", ());

    let v = dispatch(
        &mut store,
        &mut clock,
        "machine_create",
        &obj(&[("spec", spec())]),
    )
    .unwrap();
    assert_bytes("machine_add.json", &v);
    seen.insert("machine_add.json", ());

    let v = dispatch(
        &mut store,
        &mut clock,
        "instance_create",
        &obj(&[
            ("machine", Value::Str("case_review".into())),
            ("request_id", Value::Str("new-1".into())),
        ]),
    )
    .unwrap();
    assert_bytes("instance_new.json", &v);
    seen.insert("instance_new.json", ());

    let v = dispatch(
        &mut store,
        &mut clock,
        "instance_send",
        &obj(&[
            ("instance_id", Value::Str("inst-new-1".into())),
            ("event", obj(&[("name", Value::Str("docs_ok".into()))])),
            ("request_id", Value::Str("send-1".into())),
        ]),
    )
    .unwrap();
    assert_bytes("send_docs_ok.json", &v);
    seen.insert("send_docs_ok.json", ());

    let err = dispatch(
        &mut store,
        &mut clock,
        "instance_send",
        &obj(&[
            ("instance_id", Value::Str("inst-new-1".into())),
            ("event", obj(&[("name", Value::Str("resume".into()))])),
            ("request_id", Value::Str("send-bad".into())),
        ]),
    )
    .unwrap_err();
    let wrapped = tool_error(&err);
    let sc_err = wrapped.get("structuredContent").cloned().unwrap();
    assert_bytes("send_rejected.json", &sc_err);
    let text = wrapped
        .get("content")
        .and_then(Value::as_arr)
        .and_then(|a| a.first())
        .and_then(|i| i.get("text"))
        .and_then(Value::as_str)
        .unwrap();
    assert_eq!(text, fsm_cli::render::render_human(&sc_err));
    seen.insert("send_rejected.json", ());

    let v = dispatch(
        &mut store,
        &mut clock,
        "instance_send",
        &obj(&[
            ("instance_id", Value::Str("inst-new-1".into())),
            ("event", obj(&[("name", Value::Str("docs_ok".into()))])),
            ("request_id", Value::Str("send-2".into())),
        ]),
    )
    .unwrap();
    assert_bytes("send_docs_ok2.json", &v);
    seen.insert("send_docs_ok2.json", ());

    let v = dispatch(
        &mut store,
        &mut clock,
        "effect_ack",
        &obj(&[
            ("instance_id", Value::Str("inst-new-1".into())),
            ("effect_id", Value::Str("inst-new-1/3/0".into())),
            ("outcome", Value::Str("ok".into())),
            ("request_id", Value::Str("ack-1".into())),
        ]),
    )
    .unwrap();
    assert_bytes("ack.json", &v);
    seen.insert("ack.json", ());

    store.annotate("inst-new-1", "ann-1", "hello-note").unwrap();
    let v = dispatch(
        &mut store,
        &mut clock,
        "instance_history",
        &obj(&[("instance_id", Value::Str("inst-new-1".into()))]),
    )
    .unwrap();
    assert_bytes("history.json", &v);
    seen.insert("history.json", ());

    let on_disk: BTreeMap<_, _> = files.into_iter().map(|n| (n, ())).collect();
    let seen: BTreeMap<_, _> = seen;
    assert_eq!(
        seen.keys().collect::<Vec<_>>(),
        on_disk.keys().collect::<Vec<_>>(),
        "unmapped structured fixtures"
    );
}
