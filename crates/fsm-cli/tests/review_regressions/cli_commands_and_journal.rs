use std::collections::BTreeMap;
use std::process::Command;

use fsm_cli::store::Store;
use fsm_core::json::{JsonLimits, Value, parse};

use crate::harness::{case, fsm_bin, gate, tmp};

#[test]
fn simulate_zero_event_override_and_create_fail() {
    let _g = gate();
    let dir = tmp("simz");
    let mut store = Store::open(&dir).unwrap();
    store.define_machine(case(), false, false).unwrap();
    let mut clock = fsm_cli::clock::FixedClock::new(1000, 1);
    let schema = (fsm_cli::mcp::tools::registry()
        .iter()
        .find(|t| t.name == "simulate")
        .unwrap()
        .output_schema)();
    let z = fsm_cli::mcp::tools::dispatch(
        &mut store,
        &mut clock,
        "simulate",
        &Value::Obj(BTreeMap::from([
            ("machine".into(), Value::Str("case_review".into())),
            ("events".into(), Value::Arr(vec![])),
            (
                "context".into(),
                Value::Obj(BTreeMap::from([("visits".into(), Value::Str("3".into()))])),
            ),
        ])),
    )
    .unwrap();
    fsm_cli::mcp::tools::validate_args(&schema, &z).unwrap();
    assert!(z.get("stopped_at").is_none(), "{z:?}");
    assert_eq!(
        z.get("initial")
            .and_then(|i| i.get("context"))
            .and_then(|c| c.get("visits"))
            .and_then(Value::as_str),
        Some("3")
    );
    assert_eq!(
        z.get("final")
            .and_then(|f| f.get("context"))
            .and_then(|c| c.get("visits"))
            .and_then(Value::as_str),
        Some("3")
    );
    let bad = parse(
        br#"{"format":"fsm.machine/1","name":"cf","regions":[{"name":"left","states":[{"name":"left_ready"}],"initial":"left_ready"},{"name":"right","states":[{"name":"right_ready"}],"initial":"right_ready"}],"context":[{"name":"n","ty":"int","init":"0"}],"events":[],"transitions":[],"invariants":[{"name":"x","expr":"1 == 0","mode":"enforce"}]}"#,
        &JsonLimits::DEFAULT,
    )
    .unwrap();
    let err = fsm_cli::mcp::tools::dispatch(
        &mut store,
        &mut clock,
        "simulate",
        &Value::Obj(BTreeMap::from([
            ("spec".into(), bad),
            ("events".into(), Value::Arr(vec![])),
        ])),
    )
    .unwrap_err();
    assert_eq!(err.code, "run/create_failed");
}

#[test]
fn history_honors_include_trace() {
    let _g = gate();
    let dir = tmp("htr");
    let mut store = Store::open(&dir).unwrap();
    store.define_machine(case(), false, false).unwrap();
    store
        .create_instance("case_review", "i1", "c1", None)
        .unwrap();
    store
        .send_event("i1", "docs_ok", Value::Obj(BTreeMap::new()), "s1", None)
        .unwrap();
    let mut clock = fsm_cli::clock::FixedClock::new(1000, 1);
    let off = fsm_cli::mcp::tools::dispatch(
        &mut store,
        &mut clock,
        "instance_history",
        &Value::Obj(BTreeMap::from([
            ("instance_id".into(), Value::Str("i1".into())),
            ("include_trace".into(), Value::Bool(false)),
        ])),
    )
    .unwrap();
    let on = fsm_cli::mcp::tools::dispatch(
        &mut store,
        &mut clock,
        "instance_history",
        &Value::Obj(BTreeMap::from([
            ("instance_id".into(), Value::Str("i1".into())),
            ("include_trace".into(), Value::Bool(true)),
        ])),
    )
    .unwrap();
    let off_e = off.get("entries").and_then(Value::as_arr).unwrap();
    let on_e = on.get("entries").and_then(Value::as_arr).unwrap();
    let applied_off = off_e
        .iter()
        .find(|e| e.get("kind").and_then(Value::as_str) == Some("EventApplied"))
        .unwrap();
    let applied_on = on_e
        .iter()
        .find(|e| e.get("kind").and_then(Value::as_str) == Some("EventApplied"))
        .unwrap();
    assert!(applied_off.get("from_leaf").is_some());
    assert!(applied_off.get("trace").is_none(), "{applied_off:?}");
    assert!(applied_on.get("trace").is_some(), "{applied_on:?}");
}

#[test]
fn journal_verify_report_prints_hashes() {
    let _g = gate();
    let dir = tmp("vrep");
    let mut store = Store::open(&dir).unwrap();
    store.define_machine(case(), false, false).unwrap();
    store
        .create_instance("case_review", "i1", "c1", None)
        .unwrap();
    drop(store);
    let bin = fsm_bin();
    let out = Command::new(&bin)
        .args([
            "--data-dir",
            dir.to_str().unwrap(),
            "--json",
            "journal",
            "verify",
            "--report",
        ])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(0), "{stdout}");
    assert!(stdout.contains("instance_hashes"), "{stdout}");
    assert!(
        stdout.contains("state_hash") || stdout.contains("sha256:"),
        "{stdout}"
    );
    assert!(stdout.contains("segments"), "{stdout}");
}

#[test]
fn clock_ticks_only_on_journal_append() {
    fsm_cli::clock::reset_injected();
    let dir = tmp("clk2");
    let mut store = Store::open(&dir).unwrap();
    let mut clock = fsm_cli::clock::FixedClock::new(9_000, 1_000);
    let before = store.journal.last_seq;
    fsm_cli::mcp::tools::dispatch(
        &mut store,
        &mut clock,
        "machine_list",
        &Value::Obj(BTreeMap::new()),
    )
    .unwrap();
    fsm_cli::mcp::tools::dispatch(
        &mut store,
        &mut clock,
        "machine_create",
        &Value::Obj(BTreeMap::from([
            ("spec".into(), case()),
            ("dry_run".into(), Value::Bool(true)),
        ])),
    )
    .unwrap();
    let err = fsm_cli::mcp::tools::dispatch(
        &mut store,
        &mut clock,
        "instance_send",
        &Value::Obj(BTreeMap::from([
            ("instance_id".into(), Value::Str("missing".into())),
            (
                "event".into(),
                Value::Obj(BTreeMap::from([(
                    "name".into(),
                    Value::Str("docs_ok".into()),
                )])),
            ),
            ("request_id".into(), Value::Str("x".into())),
        ])),
    )
    .unwrap_err();
    assert_eq!(err.code, "req/instance_not_found");
    assert_eq!(store.journal.last_seq, before);
    fsm_cli::mcp::tools::dispatch(
        &mut store,
        &mut clock,
        "machine_create",
        &Value::Obj(BTreeMap::from([("spec".into(), case())])),
    )
    .unwrap();
    let t0 = store.records.last().map(|r| r.ts).unwrap();
    fsm_cli::mcp::tools::dispatch(
        &mut store,
        &mut clock,
        "instance_create",
        &Value::Obj(BTreeMap::from([
            ("machine".into(), Value::Str("case_review".into())),
            ("request_id".into(), Value::Str("c1".into())),
        ])),
    )
    .unwrap();
    let t1 = store.records.last().map(|r| r.ts).unwrap();
    assert_eq!(t1 - t0, 1000, "only the append advances 1000ms");
    fsm_cli::mcp::tools::dispatch(
        &mut store,
        &mut clock,
        "instance_create",
        &Value::Obj(BTreeMap::from([
            ("machine".into(), Value::Str("case_review".into())),
            ("request_id".into(), Value::Str("c1".into())),
        ])),
    )
    .unwrap();
    let t2 = store.records.last().map(|r| r.ts).unwrap();
    assert_eq!(t2, t1, "duplicate must not tick");
    assert_eq!(clock.now, 11_000, "two appends consume two 1000ms steps");
}

#[test]
fn ignored_send_reopen_preserves_schema_and_fields() {
    let _g = gate();
    let spec = parse(
        br#"{"format":"fsm.machine/1","name":"ig","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"go","fields":[]},{"name":"skip","fields":[]}],"transitions":[{"from":"a","on":"go"}],"on_unhandled":"ignore"}"#,
        &JsonLimits::DEFAULT,
    )
    .unwrap();
    let dir = tmp("igre");
    let mut store = Store::open(&dir).unwrap();
    store.define_machine(spec, false, false).unwrap();
    store.create_instance("ig", "i1", "c1", None).unwrap();
    let first = store
        .send_event("i1", "skip", Value::Obj(BTreeMap::new()), "s1", None)
        .unwrap();
    let schema = (fsm_cli::mcp::tools::registry()
        .iter()
        .find(|t| t.name == "instance_send")
        .unwrap()
        .output_schema)();
    fsm_cli::mcp::tools::validate_args(&schema, &first).unwrap();
    drop(store);
    let mut store = Store::open(&dir).unwrap();
    let retry = store
        .send_event("i1", "skip", Value::Obj(BTreeMap::new()), "s1", None)
        .unwrap();
    fsm_cli::mcp::tools::validate_args(&schema, &retry).unwrap();
    let fo = first.as_obj().unwrap();
    let ro = retry.as_obj().unwrap();
    for (k, v) in fo {
        if k == "duplicate" {
            continue;
        }
        assert_eq!(ro.get(k), Some(v), "field {k}");
    }
    assert_eq!(retry.get("duplicate").and_then(Value::as_bool), Some(true));
    assert_eq!(retry.get("ignored").and_then(Value::as_bool), Some(true));
}

#[test]
fn stamp_preserves_explicit_zero() {
    let _g = gate();
    let spec = parse(
        br#"{"format":"fsm.machine/1","name":"st","states":[{"name":"a"},{"name":"b","terminal":true}],"initial":"a","context":[],"events":[{"name":"go","fields":[{"name":"when","ty":"timestamp"},{"name":"also","ty":"timestamp"}]}],"transitions":[{"from":"a","on":"go","to":"b"}]}"#,
        &JsonLimits::DEFAULT,
    )
    .unwrap();
    let dir = tmp("st0");
    let mut store = Store::open(&dir).unwrap();
    store.define_machine(spec, false, false).unwrap();
    store.create_instance("st", "i1", "c1", None).unwrap();
    let mut payload = Value::Obj(BTreeMap::from([("when".into(), Value::Str("0".into()))]));
    let mut clock = fsm_cli::clock::FixedClock::new(42_000, 1);
    let resp = store
        .send_event_stamp_on(
            &mut clock,
            "i1",
            "go",
            &mut payload,
            "s1",
            None,
            &["when", "also"],
        )
        .unwrap();
    let p = payload.as_obj().unwrap();
    assert_eq!(p.get("when").and_then(Value::as_str), Some("0"));
    assert_eq!(p.get("also").and_then(Value::as_str), Some("42000"));
    let rec = store
        .records
        .iter()
        .rev()
        .find(|r| r.body.get("request_id").and_then(Value::as_str) == Some("s1"))
        .unwrap();
    let jp = rec.body.get("payload").and_then(Value::as_obj).unwrap();
    assert_eq!(jp.get("when").and_then(Value::as_str), Some("0"));
    assert_eq!(jp.get("also").and_then(Value::as_str), Some("42000"));
    assert_eq!(rec.ts, 42_000);
    let _ = resp;
}

#[test]
fn integer_schema_is_standard_and_rejects_u64_max() {
    let _g = gate();
    let dir = tmp("u64m");
    let mut store = Store::open(&dir).unwrap();
    store.define_machine(case(), false, false).unwrap();
    let mut clock = fsm_cli::clock::FixedClock::new(1000, 1);
    let schema = (fsm_cli::mcp::tools::registry()
        .iter()
        .find(|t| t.name == "machine_list")
        .unwrap()
        .input_schema)();
    let lim = schema
        .get("properties")
        .and_then(|p| p.get("limit"))
        .and_then(Value::as_obj)
        .unwrap();
    assert_eq!(lim.get("type").and_then(Value::as_str), Some("integer"));
    assert!(lim.get("integer").is_none());
    let err = fsm_cli::mcp::tools::dispatch(
        &mut store,
        &mut clock,
        "machine_list",
        &Value::Obj(BTreeMap::from([(
            "limit".into(),
            Value::Num("18446744073709551615".into()),
        )])),
    )
    .unwrap_err();
    assert_eq!(err.code, "req/args_invalid");
    let err = fsm_cli::mcp::tools::dispatch(
        &mut store,
        &mut clock,
        "machine_list",
        &Value::Obj(BTreeMap::from([(
            "limit".into(),
            Value::Num("9223372036854775808".into()),
        )])),
    )
    .unwrap_err();
    assert_eq!(err.code, "req/args_invalid");
}

#[test]
fn action_error_is_public_block_code() {
    let _g = gate();
    let spec = parse(
        br#"{"format":"fsm.machine/1","name":"ov","context":[{"name":"x","ty":"int","init":"9223372036854775807"}],"events":[{"name":"go","fields":[]}],"states":[{"name":"a"}],"initial":"a","transitions":[{"from":"a","on":"go","do":[{"target":"x","value":"ctx.x + 1"}]}]}"#,
        &JsonLimits::DEFAULT,
    )
    .unwrap();
    let dir = tmp("acterr");
    let mut store = Store::open(&dir).unwrap();
    let mut clock = fsm_cli::clock::FixedClock::new(1000, 1);
    fsm_cli::mcp::tools::dispatch(
        &mut store,
        &mut clock,
        "machine_create",
        &Value::Obj(BTreeMap::from([("spec".into(), spec)])),
    )
    .unwrap();
    fsm_cli::mcp::tools::dispatch(
        &mut store,
        &mut clock,
        "instance_create",
        &Value::Obj(BTreeMap::from([
            ("machine".into(), Value::Str("ov".into())),
            ("request_id".into(), Value::Str("c1".into())),
        ])),
    )
    .unwrap();
    let err = fsm_cli::mcp::tools::dispatch(
        &mut store,
        &mut clock,
        "instance_send",
        &Value::Obj(BTreeMap::from([
            ("instance_id".into(), Value::Str("inst-c1".into())),
            (
                "event".into(),
                Value::Obj(BTreeMap::from([("name".into(), Value::Str("go".into()))])),
            ),
            ("request_id".into(), Value::Str("s1".into())),
        ])),
    )
    .unwrap_err();
    assert_eq!(err.code, "run/action_error");
    assert_eq!(
        err.details.get("cause").and_then(Value::as_str),
        Some("run/overflow")
    );
    assert_eq!(
        err.details.get("block").and_then(Value::as_str),
        Some("transition")
    );
    assert!(err.span.is_some());
    assert!(err.details.get("trace").is_some());
}
