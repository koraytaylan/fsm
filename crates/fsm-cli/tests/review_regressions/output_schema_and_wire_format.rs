use std::collections::BTreeMap;
use std::process::Command;

use fsm_cli::store::Store;
use fsm_core::json::{JsonLimits, Value, parse};

use crate::harness::{case, fsm_bin, gate, tmp};

#[test]
fn stamp_applies_every_requested_field() {
    let _g = gate();
    let dir = tmp("stamp");
    let mut store = Store::open(&dir).unwrap();
    let spec = parse(
        br#"{"format":"fsm.machine/1","name":"ts","context":[],"events":[{"name":"tick","fields":[{"name":"a","ty":"timestamp"},{"name":"b","ty":"timestamp"}]}],"states":[{"name":"x"},{"name":"y"}],"initial":"x","transitions":[{"from":"x","on":"tick","to":"y"}]}"#,
        &JsonLimits::DEFAULT,
    )
    .unwrap();
    store.define_machine(spec, false, false).unwrap();
    store.create_instance("ts", "t1", "c", None).unwrap();
    fsm_cli::clock::reset_injected();
    fsm_cli::clock::force_ms(42_000);
    let mut clock = fsm_cli::clock::FixedClock::new(42_000, 1);
    let v = fsm_cli::mcp::tools::dispatch(
        &mut store,
        &mut clock,
        "instance_send",
        &Value::Obj(BTreeMap::from([
            ("instance_id".into(), Value::Str("t1".into())),
            (
                "event".into(),
                Value::Obj(BTreeMap::from([
                    ("name".into(), Value::Str("tick".into())),
                    ("payload".into(), Value::Obj(BTreeMap::new())),
                ])),
            ),
            ("request_id".into(), Value::Str("st".into())),
            (
                "stamp".into(),
                Value::Arr(vec![Value::Str("a".into()), Value::Str("b".into())]),
            ),
        ])),
    )
    .unwrap();
    assert_eq!(
        v.get("applied").and_then(Value::as_bool),
        Some(true),
        "{v:?}"
    );
    let hist = fsm_cli::mcp::tools::dispatch(
        &mut store,
        &mut clock,
        "instance_history",
        &Value::Obj(BTreeMap::from([(
            "instance_id".into(),
            Value::Str("t1".into()),
        )])),
    )
    .unwrap();
    let entries = hist.get("entries").and_then(Value::as_arr).unwrap();
    let payload = entries
        .iter()
        .rev()
        .find_map(|e| e.get("payload").and_then(Value::as_obj));
    let payload = payload.expect("stamped send payload in history");
    assert_eq!(payload.get("a").and_then(Value::as_str), Some("42000"));
    assert_eq!(payload.get("b").and_then(Value::as_str), Some("42000"));
}

#[test]
fn output_schemas_are_field_level() {
    let expect: &[(&str, &[&str])] = &[
        (
            "machine_create",
            &["machine_id", "name", "created", "dry_run", "warnings"],
        ),
        ("machine_list", &["machines"]),
        ("machine_get", &["machine_id", "name", "spec"]),
        ("machine_analyze", &["findings", "completeness"]),
        ("machine_diagram", &["format", "diagram"]),
        (
            "instance_create",
            &[
                "instance_id",
                "configuration",
                "leaf",
                "state",
                "status",
                "context",
                "deadlines_pending",
                "seq",
                "request_id",
            ],
        ),
        (
            "instance_send",
            &[
                "instance_id",
                "configuration",
                "leaf",
                "state",
                "status",
                "context",
                "deadlines_pending",
                "seq",
                "request_id",
            ],
        ),
        (
            "deadline_poll",
            &[
                "instance_id",
                "configuration",
                "deadline_applied",
                "deadline_not_due",
                "deadlines_pending",
                "request_id",
            ],
        ),
        (
            "effect_ack",
            &[
                "instance_id",
                "effect_id",
                "acked",
                "duplicate",
                "seq",
                "effects_pending",
            ],
        ),
        (
            "instance_cancel",
            &[
                "instance_id",
                "status",
                "seq",
                "configuration",
                "state",
                "context",
                "deadlines_pending",
                "state_hash",
            ],
        ),
        (
            "instance_get",
            &[
                "instance_id",
                "configuration",
                "leaf",
                "state",
                "status",
                "context",
                "deadlines_pending",
                "seq",
                "history",
            ],
        ),
        ("instance_list", &["instances"]),
        (
            "instance_history",
            &["instance_id", "entries", "chain_verified"],
        ),
        ("simulate", &["steps", "final"]),
        (
            "invocation_start",
            &["parent_instance_id", "slot", "child_instance_id"],
        ),
        (
            "invocation_return",
            &["parent_instance_id", "slot", "outcome"],
        ),
        (
            "signal_deliver",
            &["sender_instance_id", "target_instance_id", "outcome"],
        ),
    ];
    let reg = fsm_cli::mcp::tools::registry();
    assert_eq!(reg.len(), 17);
    for (name, fields) in expect {
        let t = reg.iter().find(|t| t.name == *name).expect(name);
        let out = (t.output_schema)();
        let props = out.get("properties").and_then(Value::as_obj).unwrap();
        for f in *fields {
            assert!(props.contains_key(*f), "{name} missing output field {f}");
        }
        assert!(!props.is_empty(), "{} empty output schema", t.name);
        let req = out.get("required").and_then(Value::as_arr).unwrap();
        assert!(!req.is_empty(), "{name} output required empty");
    }
}

#[test]
fn history_default_wire_has_audit_metadata() {
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
    let _ = store.send_event("i1", "resume", Value::Obj(BTreeMap::new()), "bad", None);
    let mut clock = fsm_cli::clock::FixedClock::new(1000, 1);
    let v = fsm_cli::mcp::tools::dispatch(
        &mut store,
        &mut clock,
        "instance_history",
        &Value::Obj(BTreeMap::from([(
            "instance_id".into(),
            Value::Str("i1".into()),
        )])),
    )
    .unwrap();
    assert_eq!(v.get("chain_verified").and_then(Value::as_bool), Some(true));
    let entries = v.get("entries").and_then(Value::as_arr).unwrap();
    assert!(entries.len() >= 2, "{v:?}");
    for e in entries {
        assert!(e.get("ts").is_some(), "{e:?}");
        assert!(e.get("hash").is_some(), "{e:?}");
        assert!(e.get("request_id").is_some(), "{e:?}");
        assert!(e.get("from_leaf").is_some(), "{e:?}");
        assert!(e.get("to_leaf").is_some(), "{e:?}");
        assert!(e.get("context_after").is_some(), "{e:?}");
    }
    let hidden = fsm_cli::mcp::tools::dispatch(
        &mut store,
        &mut clock,
        "instance_history",
        &Value::Obj(BTreeMap::from([
            ("instance_id".into(), Value::Str("i1".into())),
            ("include_rejected".into(), Value::Bool(false)),
        ])),
    )
    .unwrap();
    let hid = hidden.get("entries").and_then(Value::as_arr).unwrap();
    assert!(
        hid.iter()
            .all(|e| e.get("kind").and_then(Value::as_str) != Some("EventRejected")),
        "{hidden:?}"
    );
    assert!(
        hid.len() < entries.len(),
        "rejected filter did not drop rows"
    );
    for i in 0..501 {
        let _ = store.send_event(
            "i1",
            "note_added",
            Value::Obj(BTreeMap::from([("text".into(), Value::Str("n".into()))])),
            &format!("n{i}"),
            None,
        );
    }
    let capped = fsm_cli::mcp::tools::dispatch(
        &mut store,
        &mut clock,
        "instance_history",
        &Value::Obj(BTreeMap::from([
            ("instance_id".into(), Value::Str("i1".into())),
            ("limit".into(), Value::Num("500".into())),
        ])),
    )
    .unwrap();
    let cap_entries = capped.get("entries").and_then(Value::as_arr).unwrap();
    assert_eq!(cap_entries.len(), 500, "500-row cap");
    assert!(capped.get("next_from_seq").is_some(), "{capped:?}");
    let apply_seq = store
        .records
        .iter()
        .find(|r| r.body.get("request_id").and_then(Value::as_str) == Some("s1"))
        .map(|r| r.seq)
        .unwrap();
    drop(store);
    let bin = fsm_bin();
    let exp = Command::new(&bin)
        .args([
            "--data-dir",
            dir.to_str().unwrap(),
            "--json",
            "explain",
            "i1",
            "--seq",
            &apply_seq.to_string(),
        ])
        .output()
        .unwrap();
    let exp_out = String::from_utf8_lossy(&exp.stdout);
    assert_eq!(exp.status.code(), Some(0), "{exp_out}");
    assert!(
        exp_out.contains("from_leaf") && exp_out.contains("to_leaf"),
        "{exp_out}"
    );
    assert!(
        exp_out.contains("context_after") || exp_out.contains("after_context"),
        "{exp_out}"
    );
}

#[test]
fn dispatch_reads_do_not_force_ms() {
    fsm_cli::clock::reset_injected();
    let dir = tmp("clk");
    let mut store = Store::open(&dir).unwrap();
    store.define_machine(case(), false, false).unwrap();
    let mut clock = fsm_cli::clock::FixedClock::new(9_000, 1);
    fsm_cli::mcp::tools::dispatch(
        &mut store,
        &mut clock,
        "machine_list",
        &Value::Obj(BTreeMap::new()),
    )
    .unwrap();
    assert_eq!(clock.now, 9_000, "read must not consume the injected clock");
}

#[test]
fn verify_report_has_state_hashes() {
    let _g = gate();
    let dir = tmp("vr");
    let mut store = Store::open(&dir).unwrap();
    store.define_machine(case(), false, false).unwrap();
    store
        .create_instance("case_review", "i1", "c1", None)
        .unwrap();
    drop(store);
    let v = fsm_cli::journal_io::verify(&dir);
    assert!(!v.instance_hashes.is_empty(), "{:?}", v.instance_hashes);
}

#[test]
fn validate_aggregates_fields() {
    let send = fsm_cli::mcp::tools::registry()
        .into_iter()
        .find(|t| t.name == "instance_send")
        .unwrap();
    let err =
        fsm_cli::mcp::tools::validate_args(&(send.input_schema)(), &Value::Obj(BTreeMap::new()))
            .unwrap_err();
    let fields = err.details.get("fields").and_then(Value::as_arr).unwrap();
    assert!(fields.len() >= 2, "{err:?}");
}

#[test]
fn journal_replay_prefix_agrees() {
    let _g = gate();
    let dir = tmp("jrp");
    let bin = fsm_bin();
    Command::new(&bin)
        .args(["--data-dir", dir.to_str().unwrap(), "machine", "add"])
        .arg(format!(
            "@{}/tests/fixtures/machines/case_review.json",
            concat!(env!("CARGO_MANIFEST_DIR"), "/../fsm-core")
        ))
        .status()
        .unwrap();
    Command::new(&bin)
        .args([
            "--data-dir",
            dir.to_str().unwrap(),
            "instance",
            "new",
            "case_review",
            "--request-id",
            "c1",
        ])
        .status()
        .unwrap();
    Command::new(&bin)
        .args([
            "--data-dir",
            dir.to_str().unwrap(),
            "instance",
            "send",
            "inst-c1",
            "docs_ok",
            "--request-id",
            "s1",
        ])
        .status()
        .unwrap();
    let recs = fsm_cli::journal_io::load_records(&dir).unwrap();
    let n = recs[recs.len() / 2].seq;
    let out = Command::new(&bin)
        .args([
            "--data-dir",
            dir.to_str().unwrap(),
            "--json",
            "journal",
            "replay",
            "--to-seq",
            &n.to_string(),
        ])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(0), "{stdout}");
    assert!(
        stdout.contains("\"agreement\":true") || stdout.contains("agreement\": true"),
        "{stdout}"
    );
}

#[test]
fn journal_replay_agrees_with_live_after_snapshot() {
    let _g = gate();
    let dir = tmp("replay");
    let mut store = Store::open(&dir).unwrap();
    store.define_machine(case(), false, false).unwrap();
    store
        .create_instance("case_review", "i1", "c1", None)
        .unwrap();
    store.shutdown_snapshot().unwrap();
    drop(store);
    let recs = fsm_cli::journal_io::load_records(&dir).unwrap();
    let folded = fsm_core::replay::fold_with(recs, &mut fsm_core::replay::NopSink).unwrap();
    let live = Store::open(&dir).unwrap();
    assert_eq!(folded.last_seq, live.state.last_seq);
    assert_eq!(folded.last_hash, live.state.last_hash);
    assert_eq!(folded.instances.len(), live.state.instances.len());
}

fn required_fields(schema: &Value) -> Vec<String> {
    schema
        .get("required")
        .and_then(Value::as_arr)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn output_schemas_required_nested() {
    let reg = fsm_cli::mcp::tools::registry();
    let send = reg.iter().find(|t| t.name == "instance_send").unwrap();
    let ev = (send.input_schema)()
        .get("properties")
        .and_then(|p| p.get("event"))
        .cloned()
        .unwrap();
    let ev_req = required_fields(&ev);
    assert!(ev_req.iter().any(|s| s == "name"), "{ev:?}");
    let list = reg.iter().find(|t| t.name == "machine_list").unwrap();
    let items = (list.output_schema)()
        .get("properties")
        .and_then(|p| p.get("machines"))
        .and_then(|a| a.get("items"))
        .cloned()
        .unwrap();
    for f in [
        "machine_id",
        "name",
        "defined_seq",
        "topology",
        "regions",
        "states",
        "events",
        "deadlines",
        "instances",
    ] {
        assert!(
            required_fields(&items).iter().any(|s| s == f),
            "missing {f}"
        );
    }
    let il = reg.iter().find(|t| t.name == "instance_list").unwrap();
    let iitems = (il.output_schema)()
        .get("properties")
        .and_then(|p| p.get("instances"))
        .and_then(|a| a.get("items"))
        .cloned()
        .unwrap();
    for f in [
        "instance_id",
        "configuration",
        "status",
        "machine_name",
        "seq",
        "tags",
    ] {
        assert!(
            required_fields(&iitems).iter().any(|s| s == f),
            "missing {f}"
        );
    }
    let hist = reg.iter().find(|t| t.name == "instance_history").unwrap();
    let hitems = (hist.output_schema)()
        .get("properties")
        .and_then(|p| p.get("entries"))
        .and_then(|a| a.get("items"))
        .cloned()
        .unwrap();
    assert_eq!(hitems.get("type").and_then(Value::as_str), Some("object"));
    let sim = reg.iter().find(|t| t.name == "simulate").unwrap();
    let sout = (sim.output_schema)();
    let initial = sout
        .get("properties")
        .and_then(|p| p.get("initial"))
        .unwrap();
    assert!(
        required_fields(initial)
            .iter()
            .any(|s| s == "configuration")
    );
    assert!(required_fields(initial).iter().any(|s| s == "context"));
    let final_s = sout.get("properties").and_then(|p| p.get("final")).unwrap();
    assert!(required_fields(final_s).iter().any(|s| s == "context"));
    let steps = sout
        .get("properties")
        .and_then(|p| p.get("steps"))
        .and_then(|a| a.get("items"))
        .unwrap();
    for f in [
        "from_configuration",
        "to_configuration",
        "applied",
        "context",
        "index",
        "event",
    ] {
        assert!(required_fields(steps).iter().any(|s| s == f), "missing {f}");
    }
}

#[test]
fn machine_list_defaults_and_cursor() {
    let _g = gate();
    let dir = tmp("mlist");
    let mut store = Store::open(&dir).unwrap();
    let mut clock = fsm_cli::clock::FixedClock::new(1000, 1);
    for i in 0..51u32 {
        let src = format!(
            r#"{{"format":"fsm.machine/1","name":"ml{i}","states":[{{"name":"a"}}],"initial":"a","context":[],"events":[],"transitions":[]}}"#
        );
        let spec = parse(src.as_bytes(), &JsonLimits::DEFAULT).unwrap();
        store.define_machine(spec, false, false).unwrap();
    }
    let def = fsm_cli::mcp::tools::dispatch(
        &mut store,
        &mut clock,
        "machine_list",
        &Value::Obj(BTreeMap::new()),
    )
    .unwrap();
    let rows = def.get("machines").and_then(Value::as_arr).unwrap();
    assert_eq!(rows.len(), 50, "default limit 50");
    assert!(def.get("next_cursor").is_some(), "{def:?}");
    let cur = def.get("next_cursor").and_then(Value::as_str).unwrap();
    let page2 = fsm_cli::mcp::tools::dispatch(
        &mut store,
        &mut clock,
        "machine_list",
        &Value::Obj(BTreeMap::from([("cursor".into(), Value::Str(cur.into()))])),
    )
    .unwrap();
    let rows2 = page2.get("machines").and_then(Value::as_arr).unwrap();
    assert!(!rows2.is_empty(), "{page2:?}");
    let one = fsm_cli::mcp::tools::dispatch(
        &mut store,
        &mut clock,
        "machine_list",
        &Value::Obj(BTreeMap::from([("limit".into(), Value::Num("1".into()))])),
    )
    .unwrap();
    assert_eq!(
        one.get("machines").and_then(Value::as_arr).unwrap().len(),
        1
    );
}

#[test]
fn instance_list_row_shape() {
    let _g = gate();
    let dir = tmp("ilist");
    let mut store = Store::open(&dir).unwrap();
    store.define_machine(case(), false, false).unwrap();
    store
        .create_instance_ctx(
            "case_review",
            "vip1",
            "v1",
            None,
            &BTreeMap::new(),
            &["vip".into()],
        )
        .unwrap();
    let mut clock = fsm_cli::clock::FixedClock::new(1000, 1);
    let v = fsm_cli::mcp::tools::dispatch(
        &mut store,
        &mut clock,
        "instance_list",
        &Value::Obj(BTreeMap::new()),
    )
    .unwrap();
    let row = &v.get("instances").and_then(Value::as_arr).unwrap()[0];
    for f in [
        "instance_id",
        "state",
        "status",
        "machine_name",
        "seq",
        "tags",
    ] {
        assert!(row.get(f).is_some(), "missing {f} in {row:?}");
    }
    assert_eq!(
        row.get("machine_name").and_then(Value::as_str),
        Some("case_review")
    );
    let tagged = fsm_cli::mcp::tools::dispatch(
        &mut store,
        &mut clock,
        "instance_list",
        &Value::Obj(BTreeMap::from([("tag".into(), Value::Str("vip".into()))])),
    )
    .unwrap();
    assert_eq!(
        tagged
            .get("instances")
            .and_then(Value::as_arr)
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn simulate_complete_report() {
    let _g = gate();
    let dir = tmp("sim");
    let mut store = Store::open(&dir).unwrap();
    store.define_machine(case(), false, false).unwrap();
    let mut clock = fsm_cli::clock::FixedClock::new(1000, 1);
    let v = fsm_cli::mcp::tools::dispatch(
        &mut store,
        &mut clock,
        "simulate",
        &Value::Obj(BTreeMap::from([
            ("machine".into(), Value::Str("case_review".into())),
            (
                "events".into(),
                Value::Arr(vec![
                    Value::Obj(BTreeMap::from([(
                        "name".into(),
                        Value::Str("docs_ok".into()),
                    )])),
                    Value::Obj(BTreeMap::from([(
                        "name".into(),
                        Value::Str("resume".into()),
                    )])),
                ]),
            ),
        ])),
    )
    .unwrap();
    assert!(
        v.get("initial").and_then(|i| i.get("state")).is_some(),
        "{v:?}"
    );
    assert!(
        v.get("final").and_then(|f| f.get("context")).is_some(),
        "{v:?}"
    );
    assert_eq!(
        v.get("stopped_at").and_then(Value::as_num),
        Some("1"),
        "{v:?}"
    );
    let steps = v.get("steps").and_then(Value::as_arr).unwrap();
    assert!(!steps.is_empty(), "{v:?}");
    let first = &steps[0];
    assert!(first.get("from_leaf").is_some(), "{first:?}");
    assert!(first.get("args").is_some(), "{first:?}");
    assert_eq!(first.get("applied").and_then(Value::as_bool), Some(true));
    let ign_spec = parse(
        br#"{"format":"fsm.machine/1","name":"ig","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"go","fields":[]},{"name":"skip","fields":[]}],"transitions":[{"from":"a","on":"go"}],"on_unhandled":"ignore"}"#,
        &JsonLimits::DEFAULT,
    )
    .unwrap();
    let ign = fsm_cli::mcp::tools::dispatch(
        &mut store,
        &mut clock,
        "simulate",
        &Value::Obj(BTreeMap::from([
            ("spec".into(), ign_spec),
            (
                "events".into(),
                Value::Arr(vec![Value::Obj(BTreeMap::from([(
                    "name".into(),
                    Value::Str("skip".into()),
                )]))]),
            ),
        ])),
    )
    .unwrap();
    let istep = &ign.get("steps").and_then(Value::as_arr).unwrap()[0];
    assert_eq!(istep.get("ignored").and_then(Value::as_bool), Some(true));
    assert!(
        istep.get("error").is_none(),
        "ignored must not be rejected {istep:?}"
    );
}

#[test]
fn resources_newest_first() {
    let _g = gate();
    let dir = tmp("res");
    let mut store = Store::open(&dir).unwrap();
    let a = parse(
        br#"{"format":"fsm.machine/1","name":"oldm","states":[{"name":"a"}],"initial":"a","context":[],"events":[],"transitions":[]}"#,
        &JsonLimits::DEFAULT,
    )
    .unwrap();
    let b = parse(
        br#"{"format":"fsm.machine/1","name":"newm","states":[{"name":"a"}],"initial":"a","context":[],"events":[],"transitions":[]}"#,
        &JsonLimits::DEFAULT,
    )
    .unwrap();
    store.define_machine(a, false, false).unwrap();
    store.define_machine(b, false, false).unwrap();
    let listed = fsm_cli::mcp::resources::list(Some(&store));
    let items = listed.get("resources").and_then(Value::as_arr).unwrap();
    let machines: Vec<&str> = items
        .iter()
        .filter_map(|i| i.get("uri").and_then(Value::as_str))
        .filter(|u| u.starts_with("fsm://machine/"))
        .collect();
    assert!(machines.len() >= 2, "{listed:?}");
    assert!(
        machines[0].contains("newm") && machines[1].contains("oldm"),
        "{machines:?}"
    );
}

#[test]
fn dispatch_results_match_advertised_output_schemas() {
    let _g = gate();
    let dir = tmp("schm");
    let mut store = Store::open(&dir).unwrap();
    let mut clock = fsm_cli::clock::FixedClock::new(1000, 1);
    let created = fsm_cli::mcp::tools::dispatch(
        &mut store,
        &mut clock,
        "machine_create",
        &Value::Obj(BTreeMap::from([("spec".into(), case())])),
    )
    .unwrap();
    let reg = fsm_cli::mcp::tools::registry();
    let schema = |n: &str| (reg.iter().find(|t| t.name == n).unwrap().output_schema)();
    fsm_cli::mcp::tools::validate_args(&schema("machine_create"), &created).unwrap();
    let listed = fsm_cli::mcp::tools::dispatch(
        &mut store,
        &mut clock,
        "machine_list",
        &Value::Obj(BTreeMap::new()),
    )
    .unwrap();
    fsm_cli::mcp::tools::validate_args(&schema("machine_list"), &listed).unwrap();
    let got = fsm_cli::mcp::tools::dispatch(
        &mut store,
        &mut clock,
        "machine_get",
        &Value::Obj(BTreeMap::from([(
            "machine".into(),
            Value::Str("case_review".into()),
        )])),
    )
    .unwrap();
    fsm_cli::mcp::tools::validate_args(&schema("machine_get"), &got).unwrap();
    let an = fsm_cli::mcp::tools::dispatch(
        &mut store,
        &mut clock,
        "machine_analyze",
        &Value::Obj(BTreeMap::from([(
            "machine".into(),
            Value::Str("case_review".into()),
        )])),
    )
    .unwrap();
    fsm_cli::mcp::tools::validate_args(&schema("machine_analyze"), &an).unwrap();
    let inst = fsm_cli::mcp::tools::dispatch(
        &mut store,
        &mut clock,
        "instance_create",
        &Value::Obj(BTreeMap::from([
            ("machine".into(), Value::Str("case_review".into())),
            ("request_id".into(), Value::Str("c1".into())),
        ])),
    )
    .unwrap();
    fsm_cli::mcp::tools::validate_args(&schema("instance_create"), &inst).unwrap();
    let send = fsm_cli::mcp::tools::dispatch(
        &mut store,
        &mut clock,
        "instance_send",
        &Value::Obj(BTreeMap::from([
            ("instance_id".into(), Value::Str("inst-c1".into())),
            (
                "event".into(),
                Value::Obj(BTreeMap::from([(
                    "name".into(),
                    Value::Str("docs_ok".into()),
                )])),
            ),
            ("request_id".into(), Value::Str("s1".into())),
        ])),
    )
    .unwrap();
    fsm_cli::mcp::tools::validate_args(&schema("instance_send"), &send).unwrap();
    let get = fsm_cli::mcp::tools::dispatch(
        &mut store,
        &mut clock,
        "instance_get",
        &Value::Obj(BTreeMap::from([(
            "instance_id".into(),
            Value::Str("inst-c1".into()),
        )])),
    )
    .unwrap();
    fsm_cli::mcp::tools::validate_args(&schema("instance_get"), &get).unwrap();
    assert!(get.get("request_id").is_none(), "{get:?}");
    assert!(get.get("history").is_some(), "{get:?}");
    let ack = fsm_cli::mcp::tools::dispatch(
        &mut store,
        &mut clock,
        "effect_ack",
        &Value::Obj(BTreeMap::from([
            ("instance_id".into(), Value::Str("inst-c1".into())),
            (
                "effect_id".into(),
                send.get("effects_pending")
                    .and_then(Value::as_arr)
                    .and_then(|a| a.first())
                    .and_then(Value::as_str)
                    .map(|s| Value::Str(s.into()))
                    .unwrap_or(Value::Str("none".into())),
            ),
            ("outcome".into(), Value::Str("ok".into())),
            ("request_id".into(), Value::Str("ack1".into())),
        ])),
    );
    let ack = ack.expect("effect_ack must succeed on a pending id");
    fsm_cli::mcp::tools::validate_args(&schema("effect_ack"), &ack).unwrap();
    let hist = fsm_cli::mcp::tools::dispatch(
        &mut store,
        &mut clock,
        "instance_history",
        &Value::Obj(BTreeMap::from([(
            "instance_id".into(),
            Value::Str("inst-c1".into()),
        )])),
    )
    .unwrap();
    fsm_cli::mcp::tools::validate_args(&schema("instance_history"), &hist).unwrap();
    let diag = fsm_cli::mcp::tools::dispatch(
        &mut store,
        &mut clock,
        "machine_diagram",
        &Value::Obj(BTreeMap::from([
            ("machine".into(), Value::Str("case_review".into())),
            ("format".into(), Value::Str("mermaid".into())),
        ])),
    )
    .unwrap();
    fsm_cli::mcp::tools::validate_args(&schema("machine_diagram"), &diag).unwrap();
    let sim = fsm_cli::mcp::tools::dispatch(
        &mut store,
        &mut clock,
        "simulate",
        &Value::Obj(BTreeMap::from([
            ("machine".into(), Value::Str("case_review".into())),
            ("events".into(), Value::Arr(vec![])),
        ])),
    )
    .unwrap();
    fsm_cli::mcp::tools::validate_args(&schema("simulate"), &sim).unwrap();
    let listed_i = fsm_cli::mcp::tools::dispatch(
        &mut store,
        &mut clock,
        "instance_list",
        &Value::Obj(BTreeMap::from([("limit".into(), Value::Num("1".into()))])),
    )
    .unwrap();
    fsm_cli::mcp::tools::validate_args(&schema("instance_list"), &listed_i).unwrap();
    let cancel = fsm_cli::mcp::tools::dispatch(
        &mut store,
        &mut clock,
        "instance_cancel",
        &Value::Obj(BTreeMap::from([
            ("instance_id".into(), Value::Str("inst-c1".into())),
            ("reason".into(), Value::Str("done".into())),
            ("request_id".into(), Value::Str("k1".into())),
        ])),
    )
    .unwrap();
    fsm_cli::mcp::tools::validate_args(&schema("instance_cancel"), &cancel).unwrap();
    let ign_spec = parse(
        br#"{"format":"fsm.machine/1","name":"ig","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"go","fields":[]},{"name":"skip","fields":[]}],"transitions":[{"from":"a","on":"go"}],"on_unhandled":"ignore"}"#,
        &JsonLimits::DEFAULT,
    )
    .unwrap();
    fsm_cli::mcp::tools::dispatch(
        &mut store,
        &mut clock,
        "machine_create",
        &Value::Obj(BTreeMap::from([("spec".into(), ign_spec)])),
    )
    .unwrap();
    fsm_cli::mcp::tools::dispatch(
        &mut store,
        &mut clock,
        "instance_create",
        &Value::Obj(BTreeMap::from([
            ("machine".into(), Value::Str("ig".into())),
            ("request_id".into(), Value::Str("igc".into())),
        ])),
    )
    .unwrap();
    let ignored = fsm_cli::mcp::tools::dispatch(
        &mut store,
        &mut clock,
        "instance_send",
        &Value::Obj(BTreeMap::from([
            ("instance_id".into(), Value::Str("inst-igc".into())),
            (
                "event".into(),
                Value::Obj(BTreeMap::from([("name".into(), Value::Str("skip".into()))])),
            ),
            ("request_id".into(), Value::Str("igs".into())),
        ])),
    )
    .unwrap();
    fsm_cli::mcp::tools::validate_args(&schema("instance_send"), &ignored).unwrap();
    assert_eq!(ignored.get("ignored").and_then(Value::as_bool), Some(true));
    drop(store);
    let mut store = Store::open(&dir).unwrap();
    let mut clock = fsm_cli::clock::FixedClock::new(2000, 1);
    let dup = fsm_cli::mcp::tools::dispatch(
        &mut store,
        &mut clock,
        "instance_send",
        &Value::Obj(BTreeMap::from([
            ("instance_id".into(), Value::Str("inst-igc".into())),
            (
                "event".into(),
                Value::Obj(BTreeMap::from([("name".into(), Value::Str("skip".into()))])),
            ),
            ("request_id".into(), Value::Str("igs".into())),
        ])),
    )
    .unwrap();
    fsm_cli::mcp::tools::validate_args(&schema("instance_send"), &dup).unwrap();
    assert_eq!(dup.get("duplicate").and_then(Value::as_bool), Some(true));
    assert_eq!(dup.get("ignored").and_then(Value::as_bool), Some(true));
}
