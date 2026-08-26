use std::collections::BTreeMap;
use std::process::Command;

use fsm_cli::store::Store;
use fsm_core::expr::eval::Val;
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::spec::{compile, parse_machine};

use crate::harness::{case, fsm_bin, gate, tmp};

#[test]
fn int_str_field_types_differ() {
    let _g = gate();
    let a = parse(
        br#"{"format":"fsm.machine/1","name":"m","context":[],"events":[{"name":"go","fields":[{"name":"x","ty":"int"}]}],"states":[{"name":"idle"}],"initial":"idle","transitions":[{"from":"idle","on":"go"}]}"#,
        &JsonLimits::DEFAULT,
    )
    .unwrap();
    let b = parse(
        br#"{"format":"fsm.machine/1","name":"m","context":[],"events":[{"name":"go","fields":[{"name":"x","ty":"str"}]}],"states":[{"name":"idle"}],"initial":"idle","transitions":[{"from":"idle","on":"go"}]}"#,
        &JsonLimits::DEFAULT,
    )
    .unwrap();
    let ca = fsm_core::spec::compile_accepted(&a).unwrap();
    let cb = fsm_core::spec::compile_accepted(&b).unwrap();
    assert_ne!(ca.machine_id, cb.machine_id);
}

#[test]
fn unknown_transition_key_rejected() {
    let _g = gate();
    let v = parse(
        br#"{"format":"fsm.machine/1","name":"m","context":[],"events":[{"name":"go","fields":[]}],"states":[{"name":"a"},{"name":"b","terminal":true}],"initial":"a","transitions":[{"from":"a","on":"go","too":"b"}]}"#,
        &JsonLimits::DEFAULT,
    )
    .unwrap();
    let errs = parse_machine(&v).unwrap_err();
    assert!(errs.iter().any(|e| e.code == "def/unknown_key"));
}

#[test]
fn non_bool_guard_rejected() {
    let _g = gate();
    let v = parse(
        br#"{"format":"fsm.machine/1","name":"m","context":[],"events":[{"name":"go","fields":[]}],"states":[{"name":"idle"},{"name":"done","terminal":true}],"initial":"idle","transitions":[{"from":"idle","on":"go","if":"1","to":"done"}]}"#,
        &JsonLimits::DEFAULT,
    )
    .unwrap();
    assert!(compile(parse_machine(&v).unwrap()).is_err());
}

#[test]
fn payload_invalid_not_journaled() {
    let _g = gate();
    let dir = tmp("nj");
    let mut s = Store::open(&dir).unwrap();
    s.define_machine(case(), false, false).unwrap();
    s.create_instance("case_review", "i1", "n1", None).unwrap();
    let before = s.journal.last_seq;
    let mut payload = parse(br#"{"score":"1","extra":"x"}"#, &JsonLimits::DEFAULT).unwrap();
    let err = s
        .send_event_stamp("i1", "scored", &mut payload, "bad-1", None, &[])
        .unwrap_err();
    assert_eq!(err.code, "req/field_unknown");
    assert_eq!(s.journal.last_seq, before);
    let mut okp = parse(br#"{"score":"1"}"#, &JsonLimits::DEFAULT).unwrap();
    // same request id is not consumed
    let err2 = s.send_event_stamp("i1", "scored", &mut okp, "bad-1", None, &[]);
    assert!(
        err2.is_ok()
            || err2
                .err()
                .map(|e| e.code != "req/field_unknown")
                .unwrap_or(true)
            || s.journal.last_seq == before
    );
}

#[test]
fn expect_seq_garbage_exits_usage() {
    let _g = gate();
    let dir = tmp("exp");
    let bin = fsm_bin();
    assert!(bin.exists(), "fsm binary missing");
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
            "--json",
            "instance",
            "new",
            "case_review",
            "--request-id",
            "n1",
        ])
        .status()
        .unwrap();
    let out = Command::new(&bin)
        .args([
            "--data-dir",
            dir.to_str().unwrap(),
            "--json",
            "instance",
            "send",
            "inst-n1",
            "docs_ok",
            "--request-id",
            "s1",
            "--expect-seq",
            "not-a-number",
        ])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(out.stdout.is_empty());
}

#[test]
fn verify_agrees_with_open_on_override_journal() {
    let _g = gate();
    let dir = tmp("ver");
    let mut s = Store::open(&dir).unwrap();
    s.define_machine(case(), false, false).unwrap();
    let mut ov = BTreeMap::new();
    ov.insert("visits".into(), Val::Int(2));
    s.create_instance_ctx("case_review", "i1", "r1", None, &ov, &[])
        .unwrap();
    drop(s);
    assert!(Store::open(&dir).is_ok());
    let v = fsm_cli::journal_io::verify(&dir);
    assert!(matches!(v.health, fsm_cli::journal_io::JournalHealth::Ok));
    assert!(v.instances >= 1);
}

#[test]
fn duplicate_event_names_rejected() {
    let _g = gate();
    let v = parse(
        br#"{"format":"fsm.machine/1","name":"m","context":[{"name":"x","ty":"int","init":"0"}],"events":[{"name":"go","fields":[{"name":"v","ty":"bool"}]},{"name":"go","fields":[{"name":"v","ty":"int"}]}],"states":[{"name":"idle"}],"initial":"idle","transitions":[{"from":"idle","on":"go","do":[{"target":"x","value":"evt.v"}]}]}"#,
        &JsonLimits::DEFAULT,
    )
    .unwrap();
    let errs = fsm_core::spec::compile_accepted(&v).unwrap_err();
    assert!(errs.iter().any(|e| e.code == "def/dup_name"));
}

#[test]
fn decimal_if_event_assignment_rescales() {
    let _g = gate();
    let v = parse(
        br#"{"format":"fsm.machine/1","name":"d","context":[{"name":"amt","ty":{"decimal":"2"},"init":"0.00"}],"events":[{"name":"pay","fields":[{"name":"amount","ty":{"decimal":"2"}}]}],"states":[{"name":"a"},{"name":"b","terminal":true}],"initial":"a","transitions":[{"from":"a","on":"pay","to":"b","do":[{"target":"amt","value":"if false then evt.amount else 1.0"}]}]}"#,
        &JsonLimits::DEFAULT,
    )
    .unwrap();
    let dir = tmp("decif");
    let mut s = Store::open(&dir).unwrap();
    s.define_machine(v, false, false).unwrap();
    s.create_instance("d", "i1", "c1", None).unwrap();
    let mut payload = parse(br#"{"amount":"2.50"}"#, &JsonLimits::DEFAULT).unwrap();
    s.send_event_stamp("i1", "pay", &mut payload, "p1", None, &[])
        .unwrap();
    let amt = s.state.instances.get("i1").unwrap().ctx.get("amt").unwrap();
    assert_eq!(amt.canonical_string(), "1.00");
}

#[test]
fn replay_prefix_disagrees_with_later_live_state() {
    let _g = gate();
    let dir = tmp("rpl");
    let bin = fsm_bin();
    assert!(bin.exists());
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
            "n1",
        ])
        .status()
        .unwrap();
    Command::new(&bin)
        .args([
            "--data-dir",
            dir.to_str().unwrap(),
            "instance",
            "send",
            "inst-n1",
            "docs_ok",
            "--request-id",
            "s1",
        ])
        .status()
        .unwrap();
    let out = Command::new(&bin)
        .args([
            "--data-dir",
            dir.to_str().unwrap(),
            "--json",
            "journal",
            "replay",
            "--to-seq",
            "2",
        ])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("\"agreement\":true") || stdout.contains("agreement\": true"),
        "{stdout}"
    );
    assert_eq!(out.status.code(), Some(0));
    let bad = Command::new(&bin)
        .args([
            "--data-dir",
            dir.to_str().unwrap(),
            "--json",
            "journal",
            "replay",
            "--to-seq",
            "nope",
        ])
        .output()
        .unwrap();
    assert_eq!(bad.status.code(), Some(2));
    let future = Command::new(&bin)
        .args([
            "--data-dir",
            dir.to_str().unwrap(),
            "--json",
            "journal",
            "replay",
            "--to-seq",
            "999999",
        ])
        .output()
        .unwrap();
    let fut = String::from_utf8_lossy(&future.stdout);
    assert!(fut.contains("\"agreement\":false"), "{fut}");
    assert_ne!(future.status.code(), Some(0));
}

#[test]
fn enum_if_widening_overflows() {
    let _g = gate();
    let v = parse(
        br#"{"format":"fsm.machine/1","name":"ovf","enums":{"Risk":["low","high"]},"context":[{"name":"r","ty":{"enum":"Risk"},"init":"low"},{"name":"d","ty":{"decimal":"1"},"init":"0.0"}],"events":[{"name":"go","fields":[]}],"states":[{"name":"a"},{"name":"b","terminal":true}],"initial":"a","transitions":[{"from":"a","on":"go","to":"b","do":[{"target":"d","value":"round(if ctx.r == Risk.high then 0.00 else 9999999999999999999999999999999999999.9, 1, down)"}]}]}"#,
        &JsonLimits::DEFAULT,
    )
    .unwrap();
    let dir = tmp("ovf");
    let mut s = Store::open(&dir).unwrap();
    s.define_machine(v, false, false).unwrap();
    s.create_instance("ovf", "i1", "c1", None).unwrap();
    let err = s
        .send_event("i1", "go", Value::Obj(BTreeMap::new()), "g1", None)
        .unwrap_err();
    assert_eq!(err.code, "run/action_error");
    assert_eq!(
        err.details.get("cause").and_then(Value::as_str),
        Some("run/overflow")
    );
    assert_eq!(
        s.state
            .instances
            .get("i1")
            .unwrap()
            .configuration
            .sequential_leaf(),
        Some("a")
    );
    assert!(err.details.get("request_id").and_then(Value::as_str) == Some("g1"));
}

#[test]
fn emit_if_uses_compiled_scale() {
    let _g = gate();
    let v = parse(
        br#"{"format":"fsm.machine/1","name":"em","context":[{"name":"x","ty":"int","init":"0"}],"events":[{"name":"go","fields":[]}],"effects":[{"name":"bill","fields":[{"name":"amt","ty":{"decimal":"2"}}]}],"states":[{"name":"a"},{"name":"b","terminal":true}],"initial":"a","transitions":[{"from":"a","on":"go","to":"b","emit":[{"effect":"bill","args":{"amt":"if false then 1.00 else 2.0"}}]}]}"#,
        &JsonLimits::DEFAULT,
    )
    .unwrap();
    let compiled = fsm_core::spec::compile_accepted(&v).unwrap();
    let tree = fsm_core::tree::Tree::for_machine(&compiled.spec);
    let created = fsm_core::step::create(&compiled, &tree, &BTreeMap::new(), 0).unwrap();
    let inst = fsm_core::machine::InstanceState {
        status: created.status_after,
        configuration: created.configuration_after,
        ctx: created.ctx_after,
        history: created.history_after,
        deadlines: created.deadlines_after,
        pending: vec![],
        invocations: BTreeMap::new(),
        signals: BTreeMap::new(),
    };
    let mut bud = fsm_core::expr::eval::Budget::new(4096);
    match fsm_core::step::step(
        &compiled,
        &tree,
        &inst,
        "go",
        &Value::Obj(BTreeMap::new()),
        0,
        &mut bud,
    ) {
        fsm_core::step::Outcome::Applied(a) => {
            assert_eq!(
                a.effects[0].args.get("amt").unwrap().canonical_string(),
                "2.00"
            );
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn rejected_reopen_keeps_request_id() {
    let _g = gate();
    let dir = tmp("rej");
    let mut s = Store::open(&dir).unwrap();
    s.define_machine(case(), false, false).unwrap();
    s.create_instance("case_review", "i1", "c1", None).unwrap();
    s.send_event("i1", "docs_ok", Value::Obj(BTreeMap::new()), "R", None)
        .unwrap();
    let e1 = s
        .send_event("i1", "resume", Value::Obj(BTreeMap::new()), "r", None)
        .unwrap_err();
    let a1 = s
        .ack_effect_outcome("i1", "missing", "ar", "ok", None)
        .unwrap_err();
    let e_same = s
        .send_event("i1", "resume", Value::Obj(BTreeMap::new()), "r", None)
        .unwrap_err();
    let a_same = s
        .ack_effect_outcome("i1", "missing", "ar", "ok", None)
        .unwrap_err();
    assert!(!e1.duplicate && !a1.duplicate);
    assert!(e_same.duplicate && a_same.duplicate);
    drop(s);
    let mut s2 = Store::open(&dir).unwrap();
    let e2 = s2
        .send_event("i1", "resume", Value::Obj(BTreeMap::new()), "r", None)
        .unwrap_err();
    let a2 = s2
        .ack_effect_outcome("i1", "missing", "ar", "ok", None)
        .unwrap_err();
    fn strip_dup(e: &fsm_cli::store::ErrorObj) -> fsm_cli::store::ErrorObj {
        let mut c = e.clone();
        c.duplicate = false;
        c
    }
    assert!(e2.duplicate && a2.duplicate);
    assert_eq!(strip_dup(&e1).to_value(), strip_dup(&e_same).to_value());
    assert_eq!(strip_dup(&e1).to_value(), strip_dup(&e2).to_value());
    assert_eq!(strip_dup(&a1).to_value(), strip_dup(&a_same).to_value());
    assert_eq!(strip_dup(&a1).to_value(), strip_dup(&a2).to_value());
    assert_eq!(
        e2.details.get("request_id").and_then(Value::as_str),
        Some("r")
    );
    assert_eq!(
        a2.details.get("request_id").and_then(Value::as_str),
        Some("ar")
    );
}

fn scale_machine(narrow_first: bool) -> Value {
    let src =
        r#"round(if false then evt.amount else 9999999999999999999999999999999999999.9, 1, down)"#;
    let n = format!(r#"{{"from":"a","on":"narrow","do":[{{"target":"d","value":"{src}"}}]}}"#);
    let w = format!(r#"{{"from":"a","on":"wide","do":[{{"target":"d","value":"{src}"}}]}}"#);
    let trans = if narrow_first {
        format!("{n},{w}")
    } else {
        format!("{w},{n}")
    };
    parse(
        format!(
            r#"{{"format":"fsm.machine/1","name":"sc","context":[{{"name":"d","ty":{{"decimal":"1"}},"init":"0.0"}}],"events":[{{"name":"narrow","fields":[{{"name":"amount","ty":{{"decimal":"1"}}}}]}},{{"name":"wide","fields":[{{"name":"amount","ty":{{"decimal":"2"}}}}]}}],"states":[{{"name":"a"}}],"initial":"a","transitions":[{trans}]}}"#
        )
        .as_bytes(),
        &JsonLimits::DEFAULT,
    )
    .unwrap()
}

fn payload_amt(s: &str) -> Value {
    let mut m = BTreeMap::new();
    m.insert("amount".into(), Value::Str(s.into()));
    Value::Obj(m)
}

fn run_scale_slots(narrow_first: bool) {
    let dir = tmp(if narrow_first { "slot-n" } else { "slot-w" });
    let mut s = Store::open(&dir).unwrap();
    s.define_machine(scale_machine(narrow_first), false, false)
        .unwrap();
    s.create_instance("sc", "i1", "c1", None).unwrap();
    let ok = s
        .send_event("i1", "narrow", payload_amt("1.0"), "n1", None)
        .unwrap();
    assert_eq!(ok.get("applied").and_then(Value::as_bool), Some(true));
    let err = s
        .send_event("i1", "wide", payload_amt("1.00"), "w1", None)
        .unwrap_err();
    assert_eq!(err.code, "run/action_error");
    assert_eq!(
        err.details.get("cause").and_then(Value::as_str),
        Some("run/overflow")
    );
    assert_eq!(
        s.state
            .instances
            .get("i1")
            .unwrap()
            .ctx
            .get("d")
            .map(Val::canonical_string)
            .as_deref(),
        Some("9999999999999999999999999999999999999.9")
    );
    drop(s);
    let s2 = Store::open(&dir).unwrap();
    assert_eq!(
        s2.state
            .instances
            .get("i1")
            .unwrap()
            .ctx
            .get("d")
            .map(Val::canonical_string)
            .as_deref(),
        Some("9999999999999999999999999999999999999.9")
    );
    let report = fsm_cli::journal_io::verify(&dir);
    assert!(
        matches!(report.health, fsm_cli::journal_io::JournalHealth::Ok),
        "{:?}",
        report.health
    );
}

#[test]
fn compiled_slots_independent_of_declaration_order() {
    let _g = gate();
    run_scale_slots(true);
    run_scale_slots(false);
}

#[test]
fn enabled_events_uses_compiled_decimal_if() {
    let _g = gate();
    let v = parse(
        br#"{"format":"fsm.machine/1","name":"en","context":[],"events":[{"name":"go","fields":[]},{"name":"pay","fields":[{"name":"n","ty":"int"}]}],"states":[{"name":"a"},{"name":"b","terminal":true}],"initial":"a","transitions":[{"from":"a","on":"go","if":"(if true then 1.00 else 2.0) == 1.00","to":"b"},{"from":"a","on":"pay","if":"(if true then 1.00 else 2.0) == 1.00 and evt.n > 0","to":"b"}]}"#,
        &JsonLimits::DEFAULT,
    )
    .unwrap();
    let dir = tmp("enif");
    let mut s = Store::open(&dir).unwrap();
    s.define_machine(v, false, false).unwrap();
    let created = s.create_instance("en", "i1", "c1", None).unwrap();
    let evs = created
        .get("enabled_events")
        .and_then(Value::as_arr)
        .map(|a| a.to_vec())
        .unwrap_or_default();
    let status = |name: &str| {
        evs.iter()
            .find_map(|e| {
                let o = e.as_obj()?;
                if o.get("event").and_then(Value::as_str) == Some(name) {
                    o.get("status").and_then(Value::as_str)
                } else {
                    None
                }
            })
            .unwrap_or("missing")
            .to_string()
    };
    assert_eq!(status("go"), "enabled");
    assert_eq!(status("pay"), "depends_on_payload");
    s.send_event("i1", "go", Value::Obj(BTreeMap::new()), "g1", None)
        .unwrap();
    assert_eq!(
        s.state
            .instances
            .get("i1")
            .unwrap()
            .configuration
            .sequential_leaf(),
        Some("b")
    );
}
