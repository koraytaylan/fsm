use std::collections::BTreeMap;
use std::process::Command;

use fsm_cli::store::Store;
use fsm_core::json::{JsonLimits, Value, parse};

use crate::harness::{case, fsm_bin, gate, tmp};

#[test]
fn create_request_id_cli_mcp_parity() {
    let _g = gate();
    let dir = tmp("rid");
    let mut s = Store::open(&dir).unwrap();
    s.define_machine(case(), false, false).unwrap();
    let store_err = s
        .create_instance("missing", "ghost", "rid-1", None)
        .unwrap_err();
    assert_eq!(store_err.code, "req/machine_not_found");
    assert_eq!(
        store_err.details.get("request_id").and_then(Value::as_str),
        Some("rid-1")
    );
    drop(s);

    let mut clock = fsm_cli::clock::FixedClock::new(5_000, 1);
    let mut s = Store::open(&dir).unwrap();
    let mcp_err = fsm_cli::mcp::tools::dispatch(&mut s, &mut clock, "instance_create", &{
        let mut m = BTreeMap::new();
        m.insert("machine".into(), Value::Str("missing".into()));
        m.insert("request_id".into(), Value::Str("rid-2".into()));
        Value::Obj(m)
    })
    .unwrap_err();
    assert_eq!(mcp_err.code, "req/machine_not_found");
    assert_eq!(
        mcp_err.details.get("request_id").and_then(Value::as_str),
        Some("rid-2")
    );
    let mut ctx = BTreeMap::new();
    ctx.insert("visits".into(), Value::Num("2".into()));
    let mut args = BTreeMap::new();
    args.insert("machine".into(), Value::Str("case_review".into()));
    args.insert("request_id".into(), Value::Str("rid-3".into()));
    args.insert("context".into(), Value::Obj(ctx));
    let mcp_num =
        fsm_cli::mcp::tools::dispatch(&mut s, &mut clock, "instance_create", &Value::Obj(args))
            .unwrap_err();
    assert_eq!(mcp_num.code, "req/number_token");
    assert_eq!(
        mcp_num.details.get("request_id").and_then(Value::as_str),
        Some("rid-3")
    );
    drop(s);

    let bin = fsm_bin();
    let out = Command::new(&bin)
        .args([
            "--data-dir",
            dir.to_str().unwrap(),
            "--json",
            "instance",
            "new",
            "missing",
            "--request-id",
            "rid-4",
        ])
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stderr);
    assert!(text.contains("req/machine_not_found"), "{text}");
    assert!(text.contains("rid-4"), "{text}");
    let out = Command::new(&bin)
        .args([
            "--data-dir",
            dir.to_str().unwrap(),
            "--json",
            "instance",
            "new",
            "case_review",
            "--request-id",
            "rid-5",
            "--context-json",
            r#"{"visits":2}"#,
        ])
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stderr);
    assert!(
        text.contains("req/number_token") || text.contains("rid-5"),
        "{text}"
    );
    assert!(text.contains("rid-5"), "{text}");
}

#[test]
fn create_failure_exposes_discarded_trace() {
    let _g = gate();
    let v = parse(
        br#"{"format":"fsm.machine/1","name":"cf","context":[{"name":"x","ty":"int","init":"9223372036854775807"},{"name":"y","ty":"int","init":"0"}],"events":[],"states":[{"name":"c","initial":"leaf","entry":{"do":[{"target":"y","value":"1"}]},"states":[{"name":"leaf","entry":{"do":[{"target":"x","value":"ctx.x + 1"}]}}]}],"initial":"c","transitions":[]}"#,
        &JsonLimits::DEFAULT,
    )
    .unwrap();
    let dir = tmp("cf");
    let mut s = Store::open(&dir).unwrap();
    s.define_machine(v, false, false).unwrap();
    let err = s.create_instance("cf", "i1", "cf1", None).unwrap_err();
    assert_eq!(err.code, "run/create_failed");
    assert_eq!(
        err.details.get("block").and_then(Value::as_str),
        Some("entry(leaf)")
    );
    let trace = err
        .details
        .get("trace")
        .and_then(Value::as_obj)
        .expect("trace");
    let pipe = trace
        .get("pipeline")
        .and_then(Value::as_arr)
        .expect("pipeline");
    assert!(pipe.iter().any(|p| {
        p.get("block").and_then(Value::as_str) == Some("entry(c)")
            && p.get("discarded").and_then(Value::as_bool) == Some(true)
    }));
    assert!(pipe.iter().any(|p| {
        p.get("block").and_then(Value::as_str) == Some("entry(leaf)")
            && p.get("discarded").and_then(Value::as_bool) == Some(true)
    }));
}

#[test]
fn mcp_rejected_retry_marks_duplicate() {
    let _g = gate();
    let dir = tmp("mcpdup");
    let mut s = Store::open(&dir).unwrap();
    s.define_machine(case(), false, false).unwrap();
    s.create_instance("case_review", "i1", "c1", None).unwrap();
    s.send_event("i1", "docs_ok", Value::Obj(BTreeMap::new()), "R", None)
        .unwrap();
    let mut clock = fsm_cli::clock::FixedClock::new(5_000, 1);
    let mut args = BTreeMap::new();
    args.insert("instance_id".into(), Value::Str("i1".into()));
    args.insert("request_id".into(), Value::Str("bad".into()));
    let mut ev = BTreeMap::new();
    ev.insert("name".into(), Value::Str("resume".into()));
    args.insert("event".into(), Value::Obj(ev));
    let first = fsm_cli::mcp::tools::dispatch(
        &mut s,
        &mut clock,
        "instance_send",
        &Value::Obj(args.clone()),
    )
    .unwrap_err();
    assert!(!first.duplicate);
    let again =
        fsm_cli::mcp::tools::dispatch(&mut s, &mut clock, "instance_send", &Value::Obj(args))
            .unwrap_err();
    assert!(again.duplicate);
    drop(s);
    let mut s2 = Store::open(&dir).unwrap();
    let mut clock = fsm_cli::clock::FixedClock::new(5_000, 1);
    let mut args = BTreeMap::new();
    args.insert("instance_id".into(), Value::Str("i1".into()));
    args.insert("request_id".into(), Value::Str("bad".into()));
    let mut ev = BTreeMap::new();
    ev.insert("name".into(), Value::Str("resume".into()));
    args.insert("event".into(), Value::Obj(ev));
    let reopened =
        fsm_cli::mcp::tools::dispatch(&mut s2, &mut clock, "instance_send", &Value::Obj(args))
            .unwrap_err();
    assert!(reopened.duplicate);
    assert_eq!(first.code, reopened.code);
}

fn cli_json_err(dir: &std::path::Path, extra: &[&str]) -> Value {
    let bin = fsm_bin();
    let mut args = vec![
        "--data-dir",
        dir.to_str().unwrap(),
        "--json",
        "instance",
        "new",
    ];
    args.extend_from_slice(extra);
    let out = Command::new(&bin).args(&args).output().unwrap();
    parse(&out.stderr, &JsonLimits::DEFAULT).unwrap_or_else(|_| {
        panic!(
            "cli stderr not json: {}",
            String::from_utf8_lossy(&out.stderr)
        )
    })
}

fn mcp_create_err(store: &mut Store, args: Value) -> Value {
    let mut clock = fsm_cli::clock::FixedClock::new(5_000, 1);
    fsm_cli::mcp::tools::dispatch(store, &mut clock, "instance_create", &args)
        .unwrap_err()
        .to_value()
}

fn obj_str(pairs: &[(&str, &str)]) -> Value {
    Value::Obj(
        pairs
            .iter()
            .map(|(k, v)| ((*k).into(), Value::Str((*v).into())))
            .collect(),
    )
}

#[test]
fn create_cli_mcp_errors_are_byte_equal() {
    let _g = gate();
    let dir = tmp("eq");
    let mut s = Store::open(&dir).unwrap();
    s.define_machine(case(), false, false).unwrap();
    drop(s);

    let miss_cli = cli_json_err(&dir, &["missing", "--request-id", "e1"]);
    let mut s = Store::open(&dir).unwrap();
    let miss_mcp = mcp_create_err(
        &mut s,
        obj_str(&[("machine", "missing"), ("request_id", "e1")]),
    );
    drop(s);
    assert_eq!(miss_cli, miss_mcp, "missing machine");

    let wrong_cli = cli_json_err(
        &dir,
        &["case_review", "--request-id", "e2", "--context-json", "[]"],
    );
    let mut s = Store::open(&dir).unwrap();
    let mut args = BTreeMap::new();
    args.insert("machine".into(), Value::Str("case_review".into()));
    args.insert("request_id".into(), Value::Str("e2".into()));
    args.insert("context".into(), Value::Arr(vec![]));
    let wrong_mcp = mcp_create_err(&mut s, Value::Obj(args));
    drop(s);
    assert_eq!(wrong_cli, wrong_mcp, "wrong container");

    let num_cli = cli_json_err(
        &dir,
        &[
            "case_review",
            "--request-id",
            "e3",
            "--context-json",
            r#"{"visits":2}"#,
        ],
    );
    let mut s = Store::open(&dir).unwrap();
    let mut ctx = BTreeMap::new();
    ctx.insert("visits".into(), Value::Num("2".into()));
    let mut args = BTreeMap::new();
    args.insert("machine".into(), Value::Str("case_review".into()));
    args.insert("request_id".into(), Value::Str("e3".into()));
    args.insert("context".into(), Value::Obj(ctx));
    let num_mcp = mcp_create_err(&mut s, Value::Obj(args));
    drop(s);
    assert_eq!(num_cli, num_mcp, "raw number");

    let unk_cli = cli_json_err(
        &dir,
        &[
            "case_review",
            "--request-id",
            "e4",
            "--context-json",
            r#"{"nope":"1"}"#,
        ],
    );
    let mut s = Store::open(&dir).unwrap();
    let mut ctx = BTreeMap::new();
    ctx.insert("nope".into(), Value::Str("1".into()));
    let mut args = BTreeMap::new();
    args.insert("machine".into(), Value::Str("case_review".into()));
    args.insert("request_id".into(), Value::Str("e4".into()));
    args.insert("context".into(), Value::Obj(ctx));
    let unk_mcp = mcp_create_err(&mut s, Value::Obj(args));
    drop(s);
    assert_eq!(unk_cli, unk_mcp, "unknown field");

    let co_cli = cli_json_err(
        &dir,
        &[
            "case_review",
            "--request-id",
            "e5",
            "--context-json",
            r#"{"visits":"nope"}"#,
        ],
    );
    let mut s = Store::open(&dir).unwrap();
    let mut ctx = BTreeMap::new();
    ctx.insert("visits".into(), Value::Str("nope".into()));
    let mut args = BTreeMap::new();
    args.insert("machine".into(), Value::Str("case_review".into()));
    args.insert("request_id".into(), Value::Str("e5".into()));
    args.insert("context".into(), Value::Obj(ctx));
    let co_mcp = mcp_create_err(&mut s, Value::Obj(args));
    drop(s);
    assert_eq!(co_cli, co_mcp, "coercion");
}

fn cli_send_json_err(dir: &std::path::Path, extra: &[&str]) -> Value {
    let bin = fsm_bin();
    let mut args = vec![
        "--data-dir",
        dir.to_str().unwrap(),
        "--json",
        "instance",
        "send",
    ];
    args.extend_from_slice(extra);
    let out = Command::new(&bin).args(&args).output().unwrap();
    parse(&out.stderr, &JsonLimits::DEFAULT).unwrap_or_else(|_| {
        panic!(
            "cli send stderr not json: {}",
            String::from_utf8_lossy(&out.stderr)
        )
    })
}

#[test]
fn send_stale_expect_seq_cli_mcp_equal() {
    let _g = gate();
    let dir = tmp("seq");
    let mut s = Store::open(&dir).unwrap();
    s.define_machine(case(), false, false).unwrap();
    s.create_instance("case_review", "inst-e0", "e0", None)
        .unwrap();
    drop(s);
    let seq_cli = cli_send_json_err(
        &dir,
        &[
            "inst-e0",
            "docs_ok",
            "--request-id",
            "s1",
            "--expect-seq",
            "0",
        ],
    );
    let mut s = Store::open(&dir).unwrap();
    let mut ev = BTreeMap::new();
    ev.insert("name".into(), Value::Str("docs_ok".into()));
    let mut args = BTreeMap::new();
    args.insert("instance_id".into(), Value::Str("inst-e0".into()));
    args.insert("event".into(), Value::Obj(ev));
    args.insert("request_id".into(), Value::Str("s1".into()));
    args.insert("expect_seq".into(), Value::Num("0".into()));
    let mut clock = fsm_cli::clock::FixedClock::new(5_000, 1);
    let seq_mcp =
        fsm_cli::mcp::tools::dispatch(&mut s, &mut clock, "instance_send", &Value::Obj(args))
            .unwrap_err()
            .to_value();
    assert_eq!(seq_cli, seq_mcp, "send stale expect_seq");
    assert_eq!(
        seq_cli.get("code").and_then(Value::as_str),
        Some("req/seq_mismatch")
    );
    let create = fsm_cli::mcp::tools::registry()
        .into_iter()
        .find(|t| t.name == "instance_create")
        .unwrap();
    let props = (create.input_schema)()
        .get("properties")
        .and_then(Value::as_obj)
        .cloned()
        .unwrap();
    assert!(
        !props.contains_key("expect_seq"),
        "instance_create must not advertise expect_seq"
    );
}

#[test]
fn history_and_explain_reconstruct_trace() {
    let _g = gate();
    let dir = tmp("hist");
    let mut store = Store::open(&dir).unwrap();
    store.define_machine(case(), false, false).unwrap();
    store
        .create_instance("case_review", "i1", "c1", None)
        .unwrap();
    store
        .send_event("i1", "docs_ok", Value::Obj(BTreeMap::new()), "s1", None)
        .unwrap();
    let hist = store.history_page("i1", 0, 50, true, true).unwrap();
    let entries = hist.get("entries").and_then(Value::as_arr).unwrap();
    let applied = entries
        .iter()
        .find(|e| e.get("kind").and_then(Value::as_str) == Some("EventApplied"))
        .unwrap();
    assert!(applied.get("trace").is_some(), "{applied:?}");
    assert!(applied.get("before_leaf").is_some(), "{applied:?}");
    assert!(applied.get("after_leaf").is_some(), "{applied:?}");
    assert!(applied.get("ts").is_some(), "{applied:?}");
    assert_eq!(
        hist.get("chain_verified").and_then(Value::as_bool),
        Some(true)
    );
    let seq = applied
        .get("seq")
        .and_then(Value::as_num)
        .unwrap()
        .parse::<u64>()
        .unwrap();
    let explained = store.explain_seq("i1", seq).unwrap();
    assert!(explained.get("trace").is_some(), "{explained:?}");
    let hidden = store.history_page("i1", 0, 50, false, false).unwrap();
    let kinds: Vec<_> = hidden
        .get("entries")
        .and_then(Value::as_arr)
        .unwrap()
        .iter()
        .filter_map(|e| e.get("kind").and_then(Value::as_str))
        .collect();
    assert!(!kinds.iter().any(|k| *k == "EventRejected"));
}

#[test]
fn serve_uses_ctx_data_dir() {
    let _g = gate();
    let dir = tmp("serve");
    let input = std::io::Cursor::new(Vec::<u8>::new());
    let mut out = Vec::new();
    let _ = fsm_cli::mcp::serve::serve_dir(&dir, input, &mut out);
    assert!(
        dir.join("VERSION").exists(),
        "serve must open the given data dir"
    );
}

#[test]
fn tagged_create_is_listed_by_tag() {
    let _g = gate();
    let dir = tmp("tags");
    let mut store = Store::open(&dir).unwrap();
    store.define_machine(case(), false, false).unwrap();
    let mut clock = fsm_cli::clock::FixedClock::new(1000, 1);
    fsm_cli::mcp::tools::dispatch(&mut store, &mut clock, "instance_create", &{
        let mut o = BTreeMap::new();
        o.insert("machine".into(), Value::Str("case_review".into()));
        o.insert("request_id".into(), Value::Str("tagged".into()));
        o.insert("tags".into(), Value::Arr(vec![Value::Str("vip".into())]));
        Value::Obj(o)
    })
    .unwrap();
    fsm_cli::mcp::tools::dispatch(&mut store, &mut clock, "instance_create", &{
        let mut o = BTreeMap::new();
        o.insert("machine".into(), Value::Str("case_review".into()));
        o.insert("request_id".into(), Value::Str("plain".into()));
        Value::Obj(o)
    })
    .unwrap();
    let listed = fsm_cli::mcp::tools::dispatch(&mut store, &mut clock, "instance_list", &{
        let mut o = BTreeMap::new();
        o.insert("tag".into(), Value::Str("vip".into()));
        Value::Obj(o)
    })
    .unwrap();
    let ids: Vec<_> = listed
        .get("instances")
        .and_then(Value::as_arr)
        .unwrap()
        .iter()
        .filter_map(|i| i.get("instance_id").and_then(Value::as_str))
        .collect();
    assert_eq!(ids, vec!["inst-tagged"]);
    drop(store);
    let mut store = Store::open(&dir).unwrap();
    let listed = fsm_cli::mcp::tools::dispatch(&mut store, &mut clock, "instance_list", &{
        let mut o = BTreeMap::new();
        o.insert("tag".into(), Value::Str("vip".into()));
        Value::Obj(o)
    })
    .unwrap();
    let ids: Vec<_> = listed
        .get("instances")
        .and_then(Value::as_arr)
        .unwrap()
        .iter()
        .filter_map(|i| i.get("instance_id").and_then(Value::as_str))
        .collect();
    assert_eq!(ids, vec!["inst-tagged"], "tags must survive journal reopen");
}

#[test]
fn machine_list_limit_and_cursor() {
    let _g = gate();
    let dir = tmp("ml");
    let mut store = Store::open(&dir).unwrap();
    let mut clock = fsm_cli::clock::FixedClock::new(1000, 1);
    for (name, desc) in [("aa", "1"), ("bb", "2")] {
        let mut spec = case().as_obj().unwrap().clone();
        spec.insert("name".into(), Value::Str(name.into()));
        spec.insert("description".into(), Value::Str(desc.into()));
        fsm_cli::mcp::tools::dispatch(
            &mut store,
            &mut clock,
            "machine_create",
            &Value::Obj(BTreeMap::from([("spec".into(), Value::Obj(spec))])),
        )
        .unwrap();
    }
    let first = fsm_cli::mcp::tools::dispatch(
        &mut store,
        &mut clock,
        "machine_list",
        &Value::Obj(BTreeMap::from([("limit".into(), Value::Num("1".into()))])),
    )
    .unwrap();
    let rows = first.get("machines").and_then(Value::as_arr).unwrap();
    assert_eq!(rows.len(), 1, "{first:?}");
    let cur = rows[0]
        .get("machine_id")
        .and_then(Value::as_str)
        .unwrap()
        .to_string();
    let rest = fsm_cli::mcp::tools::dispatch(
        &mut store,
        &mut clock,
        "machine_list",
        &Value::Obj(BTreeMap::from([
            ("limit".into(), Value::Num("10".into())),
            ("cursor".into(), Value::Str(cur.clone())),
        ])),
    )
    .unwrap();
    let rest_ids: Vec<_> = rest
        .get("machines")
        .and_then(Value::as_arr)
        .unwrap()
        .iter()
        .filter_map(|m| m.get("machine_id").and_then(Value::as_str))
        .collect();
    assert!(!rest_ids.contains(&cur.as_str()), "{rest:?}");
    assert!(!rest_ids.is_empty(), "{rest:?}");
}

#[test]
fn diagram_instance_overlay_marks_current() {
    let _g = gate();
    let dir = tmp("ov");
    let mut store = Store::open(&dir).unwrap();
    store.define_machine(case(), false, false).unwrap();
    store
        .create_instance("case_review", "i1", "c1", None)
        .unwrap();
    let mut clock = fsm_cli::clock::FixedClock::new(1000, 1);
    let v = fsm_cli::mcp::tools::dispatch(
        &mut store,
        &mut clock,
        "machine_diagram",
        &Value::Obj(BTreeMap::from([
            ("machine".into(), Value::Str("case_review".into())),
            ("instance".into(), Value::Str("i1".into())),
            ("format".into(), Value::Str("mermaid".into())),
        ])),
    )
    .unwrap();
    let d = v.get("diagram").and_then(Value::as_str).unwrap();
    assert!(d.contains("class intake current"), "{d}");
}

#[test]
fn numeric_tokens_reject_non_integers() {
    let _g = gate();
    let dir = tmp("nums");
    let mut store = Store::open(&dir).unwrap();
    store.define_machine(case(), false, false).unwrap();
    store
        .create_instance("case_review", "i1", "c1", None)
        .unwrap();
    let seq = store.journal.last_seq;
    let mut clock = fsm_cli::clock::FixedClock::new(1000, 1);
    for bad in [
        "-1",
        "1.5",
        "1e2",
        "1.0",
        "999999999999999999999",
        "9223372036854775808",
        "18446744073709551615",
    ] {
        let err = fsm_cli::mcp::tools::dispatch(
            &mut store,
            &mut clock,
            "instance_send",
            &Value::Obj(BTreeMap::from([
                ("instance_id".into(), Value::Str("i1".into())),
                (
                    "event".into(),
                    Value::Obj(BTreeMap::from([(
                        "name".into(),
                        Value::Str("docs_ok".into()),
                    )])),
                ),
                ("request_id".into(), Value::Str(format!("e{bad}"))),
                ("expect_seq".into(), Value::Num(bad.into())),
            ])),
        )
        .unwrap_err();
        assert_eq!(err.code, "req/args_invalid", "{bad} {err:?}");
        assert_eq!(store.journal.last_seq, seq, "journal changed on {bad}");
        let err = fsm_cli::mcp::tools::dispatch(
            &mut store,
            &mut clock,
            "machine_list",
            &Value::Obj(BTreeMap::from([("limit".into(), Value::Num(bad.into()))])),
        )
        .unwrap_err();
        assert_eq!(err.code, "req/args_invalid", "limit {bad}");
        let err = fsm_cli::mcp::tools::dispatch(
            &mut store,
            &mut clock,
            "instance_history",
            &Value::Obj(BTreeMap::from([
                ("instance_id".into(), Value::Str("i1".into())),
                ("limit".into(), Value::Num(bad.into())),
            ])),
        )
        .unwrap_err();
        assert_eq!(err.code, "req/args_invalid", "hist {bad}");
        let err = fsm_cli::mcp::tools::dispatch(
            &mut store,
            &mut clock,
            "machine_list",
            &Value::Obj(BTreeMap::from([("cursor".into(), Value::Num(bad.into()))])),
        )
        .unwrap_err();
        assert_eq!(err.code, "req/args_invalid", "cursor {bad}");
    }
    let err = fsm_cli::mcp::tools::dispatch(
        &mut store,
        &mut clock,
        "machine_list",
        &Value::Obj(BTreeMap::from([("limit".into(), Value::Num("201".into()))])),
    )
    .unwrap_err();
    assert_eq!(err.code, "req/args_invalid", "limit+1 list");
    let err = fsm_cli::mcp::tools::dispatch(
        &mut store,
        &mut clock,
        "instance_history",
        &Value::Obj(BTreeMap::from([
            ("instance_id".into(), Value::Str("i1".into())),
            ("limit".into(), Value::Num("501".into())),
        ])),
    )
    .unwrap_err();
    assert_eq!(err.code, "req/args_invalid", "limit+1 hist");
    assert_eq!(store.journal.last_seq, seq);
}
