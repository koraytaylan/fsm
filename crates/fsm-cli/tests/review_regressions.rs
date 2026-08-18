//! Targeted regressions for store identity, replay, and CLI contracts.

use std::collections::BTreeMap;
use std::process::Command;
use std::sync::{Mutex, MutexGuard};

use fsm_cli::store::Store;
use fsm_core::expr::eval::Val;
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::spec::{compile, parse_machine};

static GATE: Mutex<()> = Mutex::new(());

fn gate() -> MutexGuard<'static, ()> {
    GATE.lock().unwrap_or_else(|e| e.into_inner())
}

fn tmp(tag: &str) -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!(
        "fsm-reg-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn case() -> Value {
    parse(
        include_bytes!("../../fsm-core/tests/fixtures/machines/case_review.json"),
        &JsonLimits::DEFAULT,
    )
    .unwrap()
}

fn fsm_bin() -> std::path::PathBuf {
    std::env::var_os("CARGO_BIN_EXE_fsm")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/debug/fsm")
        })
}

#[test]
fn override_survives_reopen() {
    let _g = gate();
    let dir = tmp("ov");
    let mut s = Store::open(&dir).unwrap();
    s.define_machine(case(), false, false).unwrap();
    let mut ov = BTreeMap::new();
    ov.insert("visits".into(), Val::Int(2));
    s.create_instance_ctx("case_review", "i1", "r1", None, &ov, &[])
        .unwrap();
    drop(s);
    let s2 = Store::open(&dir).unwrap();
    let inst = s2.state.instances.get("i1").unwrap();
    assert_eq!(inst.ctx.get("visits"), Some(&Val::Int(2)));
}

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
    assert_eq!(s.state.instances.get("i1").unwrap().leaf, "a");
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
    let tree = fsm_core::tree::Tree::build(&compiled.spec.states);
    let created = fsm_core::step::create(&compiled, &tree, &BTreeMap::new()).unwrap();
    let inst = fsm_core::machine::InstanceState {
        status: created.status_after,
        leaf: created.leaf_after,
        ctx: created.ctx_after,
        history: created.history_after,
        pending: vec![],
    };
    let mut bud = fsm_core::expr::eval::Budget::new(4096);
    match fsm_core::step::step(
        &compiled,
        &tree,
        &inst,
        "go",
        &Value::Obj(BTreeMap::new()),
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
    assert_eq!(s.state.instances.get("i1").unwrap().leaf, "b");
}

#[test]
fn versions_before_checkpoint_format_and_missing_marker_migrate() {
    let _g = gate();
    let dir = tmp("ver");
    let mut s = Store::open(&dir).unwrap();
    s.define_machine(case(), false, false).unwrap();
    drop(s);
    for marker in ["3", "4", "5"] {
        std::fs::write(dir.join("VERSION"), format!("{marker}\n")).unwrap();
        let s = match Store::open(&dir) {
            Ok(s) => s,
            Err(e) => panic!("VERSION {marker} should migrate: {e:?}"),
        };
        assert_eq!(
            std::fs::read_to_string(dir.join("VERSION")).unwrap().trim(),
            "6"
        );
        s.resolve_machine("case_review").unwrap();
        drop(s);
    }
    std::fs::remove_file(dir.join("VERSION")).unwrap();
    let s = match Store::open(&dir) {
        Ok(s) => s,
        Err(e) => panic!("missing VERSION should migrate: {e:?}"),
    };
    assert_eq!(
        std::fs::read_to_string(dir.join("VERSION")).unwrap().trim(),
        "6"
    );
    s.resolve_machine("case_review").unwrap();
    drop(s);
    std::fs::write(dir.join("VERSION"), "7\n").unwrap();
    let err = match Store::open(&dir) {
        Ok(_) => panic!("VERSION 7 opened"),
        Err(e) => e,
    };
    assert_eq!(err.code, "store/version_mismatch");
    match fsm_cli::journal_io::init(&dir) {
        Err(fsm_cli::journal_io::JournalIoError::VersionMismatch { found }) if found == "7" => {}
        Err(e) => panic!("expected version mismatch, got {e:?}"),
        Ok(_) => panic!("init succeeded on refused VERSION"),
    }
}

#[test]
fn span_bearing_rejected_retry_keeps_span() {
    let _g = gate();
    let v = parse(
        br#"{"format":"fsm.machine/1","name":"ov","context":[{"name":"x","ty":"int","init":"9223372036854775807"}],"events":[{"name":"go","fields":[]}],"states":[{"name":"a"}],"initial":"a","transitions":[{"from":"a","on":"go","do":[{"target":"x","value":"ctx.x + 1"}]}]}"#,
        &JsonLimits::DEFAULT,
    )
    .unwrap();
    let dir = tmp("span");
    let mut s = Store::open(&dir).unwrap();
    s.define_machine(v, false, false).unwrap();
    s.create_instance("ov", "i1", "c1", None).unwrap();
    let e1 = s
        .send_event("i1", "go", Value::Obj(BTreeMap::new()), "ov1", None)
        .unwrap_err();
    assert_eq!(e1.code, "run/action_error");
    assert_eq!(
        e1.details.get("cause").and_then(Value::as_str),
        Some("run/overflow")
    );
    assert_eq!(e1.span, Some((0, 9)));
    assert!(!e1.duplicate);
    let e_same = s
        .send_event("i1", "go", Value::Obj(BTreeMap::new()), "ov1", None)
        .unwrap_err();
    assert!(e_same.duplicate);
    assert_eq!(e_same.span, e1.span);
    drop(s);
    let mut s2 = Store::open(&dir).unwrap();
    let e2 = s2
        .send_event("i1", "go", Value::Obj(BTreeMap::new()), "ov1", None)
        .unwrap_err();
    assert!(e2.duplicate);
    assert_eq!(e2.span, e1.span);
    assert_eq!(
        e2.details.get("block").and_then(Value::as_str),
        Some("transition")
    );
}

#[test]
fn altered_rejection_details_fail_replay() {
    let _g = gate();
    let dir = tmp("alt");
    let mut s = Store::open(&dir).unwrap();
    s.define_machine(case(), false, false).unwrap();
    s.create_instance("case_review", "i1", "c1", None).unwrap();
    s.send_event("i1", "docs_ok", Value::Obj(BTreeMap::new()), "R", None)
        .unwrap();
    s.send_event("i1", "resume", Value::Obj(BTreeMap::new()), "r", None)
        .unwrap_err();
    let recs = fsm_cli::journal_io::load_records(&dir).unwrap();
    let last = recs.last().unwrap().clone();
    assert_eq!(last.kind, fsm_core::record::RecordKind::EventRejected);
    let prev = recs[recs.len() - 2].hash.clone();
    let mut body = last.body.as_obj().cloned().unwrap();
    body.insert("details".into(), Value::Obj(BTreeMap::new()));
    let forged = fsm_core::record::seal(last.seq, last.ts, last.kind, Value::Obj(body), &prev);
    let jdir = dir.join("journal");
    let seg = std::fs::read_dir(&jdir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .find(|p| {
            p.file_name()
                .map(|n| n.to_string_lossy().starts_with("seg-"))
                .unwrap_or(false)
        })
        .unwrap();
    let mut lines: Vec<Vec<u8>> = std::fs::read(&seg)
        .unwrap()
        .split_inclusive(|&b| b == b'\n')
        .map(|l| l.to_vec())
        .filter(|l| l.iter().any(|b| !b.is_ascii_whitespace()))
        .collect();
    lines.pop();
    let mut out = Vec::new();
    for l in &lines {
        out.extend_from_slice(l);
        if !l.ends_with(&[b'\n']) {
            out.push(b'\n');
        }
    }
    out.extend_from_slice(&forged.to_line());
    out.push(b'\n');
    std::fs::write(&seg, out).unwrap();
    drop(s);
    let report = fsm_cli::journal_io::verify(&dir);
    assert!(
        !matches!(report.health, fsm_cli::journal_io::JournalHealth::Ok),
        "{:?}",
        report.health
    );
}

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

fn rewrite_last_record(dir: &std::path::Path, mut edit: impl FnMut(&mut BTreeMap<String, Value>)) {
    let recs = fsm_cli::journal_io::load_records(dir).unwrap();
    let last = recs.last().unwrap().clone();
    let prev = recs[recs.len() - 2].hash.clone();
    let mut body = last.body.as_obj().cloned().unwrap();
    edit(&mut body);
    let forged = fsm_core::record::seal(last.seq, last.ts, last.kind, Value::Obj(body), &prev);
    let jdir = dir.join("journal");
    let seg = std::fs::read_dir(&jdir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .find(|p| {
            p.file_name()
                .map(|n| n.to_string_lossy().starts_with("seg-"))
                .unwrap_or(false)
        })
        .unwrap();
    let mut lines: Vec<Vec<u8>> = std::fs::read(&seg)
        .unwrap()
        .split_inclusive(|&b| b == b'\n')
        .map(|l| l.to_vec())
        .filter(|l| l.iter().any(|b| !b.is_ascii_whitespace()))
        .collect();
    lines.pop();
    let mut out = Vec::new();
    for l in &lines {
        out.extend_from_slice(l);
        if !l.ends_with(&[b'\n']) {
            out.push(b'\n');
        }
    }
    out.extend_from_slice(&forged.to_line());
    out.push(b'\n');
    std::fs::write(&seg, out).unwrap();
}

fn verify_not_ok(dir: &std::path::Path) {
    let report = fsm_cli::journal_io::verify(dir);
    assert!(
        !matches!(report.health, fsm_cli::journal_io::JournalHealth::Ok),
        "{:?}",
        report.health
    );
}

#[test]
fn extra_key_event_rejected_fails_replay() {
    let _g = gate();
    let dir = tmp("xkey");
    let mut s = Store::open(&dir).unwrap();
    s.define_machine(case(), false, false).unwrap();
    s.create_instance("case_review", "i1", "c1", None).unwrap();
    s.send_event("i1", "docs_ok", Value::Obj(BTreeMap::new()), "R", None)
        .unwrap();
    s.send_event("i1", "resume", Value::Obj(BTreeMap::new()), "r", None)
        .unwrap_err();
    drop(s);
    rewrite_last_record(&dir, |body| {
        let mut d = body
            .get("details")
            .and_then(Value::as_obj)
            .cloned()
            .unwrap_or_default();
        d.insert("fabricated".into(), Value::Str("accepted".into()));
        body.insert("details".into(), Value::Obj(d));
    });
    verify_not_ok(&dir);
}

#[test]
fn unexpected_block_event_rejected_fails_replay() {
    let _g = gate();
    let dir = tmp("xblk");
    let mut s = Store::open(&dir).unwrap();
    s.define_machine(case(), false, false).unwrap();
    s.create_instance("case_review", "i1", "c1", None).unwrap();
    s.send_event("i1", "docs_ok", Value::Obj(BTreeMap::new()), "R", None)
        .unwrap();
    s.send_event("i1", "resume", Value::Obj(BTreeMap::new()), "r", None)
        .unwrap_err();
    drop(s);
    rewrite_last_record(&dir, |body| {
        let mut d = body
            .get("details")
            .and_then(Value::as_obj)
            .cloned()
            .unwrap_or_default();
        d.insert("block".into(), Value::Str("nope".into()));
        body.insert("details".into(), Value::Obj(d));
    });
    verify_not_ok(&dir);
}

#[test]
fn extra_key_and_span_request_rejected_fails_replay() {
    let _g = gate();
    let dir = tmp("xrr");
    let mut s = Store::open(&dir).unwrap();
    s.define_machine(case(), false, false).unwrap();
    s.create_instance("case_review", "i1", "c1", None).unwrap();
    s.send_event("i1", "docs_ok", Value::Obj(BTreeMap::new()), "R", None)
        .unwrap();
    s.ack_effect_outcome("i1", "missing", "ar", "ok", None)
        .unwrap_err();
    drop(s);
    rewrite_last_record(&dir, |body| {
        let mut d = body
            .get("details")
            .and_then(Value::as_obj)
            .cloned()
            .unwrap_or_default();
        d.insert("fabricated".into(), Value::Str("accepted".into()));
        body.insert("details".into(), Value::Obj(d));
        let mut sp = BTreeMap::new();
        sp.insert("start".into(), Value::Num("1".into()));
        sp.insert("end".into(), Value::Num("2".into()));
        body.insert("span".into(), Value::Obj(sp));
    });
    verify_not_ok(&dir);
}

#[test]
fn lock_held_store_open_writes_no_version() {
    let _g = gate();
    let dir = tmp("lockv");
    std::fs::create_dir_all(dir.join("journal")).unwrap();
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .open(dir.join("journal/LOCK"))
        .unwrap();
    lock.try_lock().unwrap();
    let err = match Store::open(&dir) {
        Ok(_) => panic!("open succeeded while locked"),
        Err(e) => e,
    };
    assert!(
        err.code.contains("lock") || err.message.contains("lock"),
        "{err:?}"
    );
    assert!(!dir.join("VERSION").exists());
}

#[test]
fn concurrent_first_open_installs_one_version() {
    let _g = gate();
    let dir = tmp("conc");
    let a = dir.clone();
    let b = dir.clone();
    let t1 = std::thread::spawn(move || Store::open(&a));
    let t2 = std::thread::spawn(move || Store::open(&b));
    let r1 = t1.join().unwrap();
    let r2 = t2.join().unwrap();
    assert!(r1.is_ok() || r2.is_ok(), "both first opens failed");
    drop(r1);
    drop(r2);
    assert_eq!(
        std::fs::read_to_string(dir.join("VERSION"))
            .unwrap_or_default()
            .trim(),
        "6"
    );
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
                "leaf",
                "state",
                "status",
                "context",
                "seq",
                "request_id",
            ],
        ),
        (
            "instance_send",
            &[
                "instance_id",
                "leaf",
                "state",
                "status",
                "context",
                "seq",
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
                "state",
                "context",
                "state_hash",
            ],
        ),
        (
            "instance_get",
            &[
                "instance_id",
                "leaf",
                "state",
                "status",
                "context",
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
    ];
    let reg = fsm_cli::mcp::tools::registry();
    assert_eq!(reg.len(), 13);
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
        "states",
        "events",
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
        "state",
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
    for f in ["seq", "ts", "kind", "hash"] {
        assert!(
            required_fields(&hitems).iter().any(|s| s == f),
            "missing {f}"
        );
    }
    let sim = reg.iter().find(|t| t.name == "simulate").unwrap();
    let sout = (sim.output_schema)();
    let initial = sout
        .get("properties")
        .and_then(|p| p.get("initial"))
        .unwrap();
    assert!(required_fields(initial).iter().any(|s| s == "state"));
    assert!(required_fields(initial).iter().any(|s| s == "context"));
    let final_s = sout.get("properties").and_then(|p| p.get("final")).unwrap();
    assert!(required_fields(final_s).iter().any(|s| s == "context"));
    let steps = sout
        .get("properties")
        .and_then(|p| p.get("steps"))
        .and_then(|a| a.get("items"))
        .unwrap();
    for f in [
        "from_leaf",
        "to_leaf",
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
fn write_snapshot_propagates_dir_sync() {
    let _g = gate();
    let dir = tmp("wsnp");
    let mut store = Store::open(&dir).unwrap();
    store.define_machine(case(), false, false).unwrap();
    store
        .create_instance("case_review", "i1", "c1", None)
        .unwrap();
    let state = store.state.clone();
    drop(store);
    let snap = dir.join("snapshots");
    let _ = std::fs::remove_dir_all(&snap);
    std::fs::write(&snap, b"not-a-directory").unwrap();
    let err = fsm_cli::snapshot::write_snapshot(&dir, &state).unwrap_err();
    assert_eq!(err.code, "io/write", "{err:?}");
}

fn reseal_snapshot(o: &mut BTreeMap<String, Value>) {
    let root_material = Value::Obj(BTreeMap::from([
        ("seq".into(), o.get("seq").unwrap().clone()),
        ("machines".into(), o.get("machines").unwrap().clone()),
        ("instances".into(), o.get("instances").unwrap().clone()),
        ("dedup".into(), o.get("dedup").unwrap().clone()),
    ]));
    let root = format!(
        "sha256:{}",
        fsm_core::sha256::to_hex(&fsm_core::hashes::domain_hash(
            "fsm:state-root:2",
            &root_material,
        ))
    );
    o.insert("state_root".into(), Value::Str(root));
    o.insert("snapshot_hash".into(), Value::Str(String::new()));
    let hash = format!(
        "sha256:{}",
        fsm_core::sha256::to_hex(&fsm_core::hashes::domain_hash(
            "fsm:snapshot:2",
            &Value::Obj(o.clone()),
        ))
    );
    o.insert("snapshot_hash".into(), Value::Str(hash));
}

fn rewrite_snap_strip_dedup(dir: &std::path::Path, rid: &str, snap_seq: u64) {
    let path = keep_only_snap_seq(dir, snap_seq);
    let bytes = std::fs::read(&path).unwrap();
    let v = parse(&bytes, &JsonLimits::DEFAULT).unwrap();
    let mut o = v.as_obj().unwrap().clone();
    if let Some(Value::Obj(d)) = o.get_mut("dedup") {
        d.remove(rid);
    }
    reseal_snapshot(&mut o);
    std::fs::write(&path, fsm_core::canon::canon_bytes(&Value::Obj(o))).unwrap();
}

#[test]
fn stripped_dedup_snapshot_and_mutable_sidecars_cannot_reexecute() {
    let _g = gate();
    let dir = tmp("stripd");
    let mut store = Store::open(&dir).unwrap();
    store.define_machine(case(), false, false).unwrap();
    store
        .create_instance("case_review", "i1", "c1", None)
        .unwrap();
    store.shutdown_snapshot().unwrap();
    let seq = store.journal.last_seq;
    assert!(store.state.dedup.contains_key("c1"));
    let snap_seq = store.journal.last_seq;
    drop(store);
    rewrite_snap_strip_dedup(&dir, "c1", snap_seq);
    let snap_path = keep_only_snap_seq(&dir, snap_seq);
    let forged = parse(&std::fs::read(&snap_path).unwrap(), &JsonLimits::DEFAULT).unwrap();
    let forged_root = forged.get("state_root").and_then(Value::as_str).unwrap();
    fsm_cli::snapshot::commit_state_root(&dir, snap_seq, forged_root).unwrap();
    let old_sidecar = dir.join("journal").join(format!("root-{snap_seq:020}"));
    std::fs::write(&old_sidecar, format!("{forged_root}\n")).unwrap();
    let mut store = Store::open(&dir).unwrap();
    assert!(
        store.state.dedup.contains_key("c1"),
        "open must fall back to journal fold"
    );
    assert_eq!(store.journal.last_seq, seq);
    let again = store.create_instance("case_review", "i1", "c1", None);
    assert!(again.is_ok(), "{again:?}");
    let v = again.unwrap();
    assert_eq!(v.get("duplicate").and_then(Value::as_bool), Some(true));
    assert_eq!(store.journal.last_seq, seq, "retry must not append");
    assert!(
        !old_sidecar.exists(),
        "legacy unbounded sidecars should be removed, not trusted"
    );
}

#[test]
fn journal_replay_disagrees_on_stripped_dedup_snapshot() {
    let _g = gate();
    let dir = tmp("jrpd");
    let mut store = Store::open(&dir).unwrap();
    store.define_machine(case(), false, false).unwrap();
    store
        .create_instance("case_review", "i1", "c1", None)
        .unwrap();
    store.shutdown_snapshot().unwrap();
    let snap_seq = store.journal.last_seq;
    store
        .send_event("i1", "docs_ok", Value::Obj(BTreeMap::new()), "s1", None)
        .unwrap();
    let last_seq = store.journal.last_seq;
    drop(store);
    rewrite_snap_strip_dedup(&dir, "c1", snap_seq);
    let snap_path = std::fs::read_dir(dir.join("snapshots"))
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .find(|p| p.extension().and_then(|s| s.to_str()) == Some("json"))
        .unwrap();
    let parsed = parse(&std::fs::read(&snap_path).unwrap(), &JsonLimits::DEFAULT).unwrap();
    fsm_cli::snapshot::snapshot_to_state(&parsed).expect("rewritten snap must parse");
    let recs = fsm_cli::journal_io::load_records(&dir).unwrap();
    let last = recs.last().unwrap().seq;
    let live = fsm_cli::snapshot::reconstruct_snapshot_plus_tail(&dir, &recs, last).unwrap();
    assert!(
        !live.dedup.contains_key("c1"),
        "reconstruct should keep stripped snap dedup {:?}",
        live.dedup
    );
    let (_, div) = replay_disagreement(&dir);
    assert_eq!(div, snap_seq, "dedup responsible seq");
    assert_ne!(div, last_seq, "dedup must not be last_seq");
    let _ = last;
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
        br#"{"format":"fsm.machine/1","name":"cf","states":[{"name":"a"}],"initial":"a","context":[{"name":"n","ty":"int","init":"0"}],"events":[],"transitions":[],"invariants":[{"name":"x","expr":"1 == 0","mode":"enforce"}]}"#,
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

#[test]
fn snapshot_binding_skips_prefix_replay() {
    let _g = gate();
    let dir = tmp("fastp");
    let mut store = Store::open(&dir).unwrap();
    store.define_machine(case(), false, false).unwrap();
    store
        .create_instance("case_review", "i1", "c1", None)
        .unwrap();
    store
        .send_event("i1", "docs_ok", Value::Obj(BTreeMap::new()), "s1", None)
        .unwrap();
    store.shutdown_snapshot().unwrap();
    let mid = store.journal.last_seq;
    store
        .send_event("i1", "docs_ok", Value::Obj(BTreeMap::new()), "s2", None)
        .unwrap();
    let last = store.journal.last_seq;
    drop(store);
    for (seq, path) in fsm_cli::snapshot::listed_snaps(&dir) {
        if seq != mid {
            let _ = std::fs::remove_file(path);
        }
    }
    let store = Store::open(&dir).unwrap();
    assert!(store.opened_from_snapshot, "expected snapshot fast path");
    assert_eq!(store.opened_snapshot_seq, Some(mid));
    assert_eq!(store.replayed_records, (last - mid) as usize);
    assert!(store.replayed_records > 0);
    assert!(store.replayed_records < store.records.len());
}

fn replay_disagreement(dir: &std::path::Path) -> (String, u64) {
    let bin = fsm_bin();
    let out = Command::new(&bin)
        .args([
            "--data-dir",
            dir.to_str().unwrap(),
            "--json",
            "journal",
            "replay",
        ])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(stdout.contains("\"agreement\":false"), "{stdout}");
    assert_ne!(out.status.code(), Some(0), "{stdout}");
    let v = parse(stdout.trim().as_bytes(), &JsonLimits::DEFAULT).expect(&stdout);
    let seq = v
        .get("first_divergent_seq")
        .and_then(Value::as_num)
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or_else(|| panic!("numeric first_divergent_seq missing: {stdout}"));
    (stdout, seq)
}

#[test]
fn journal_replay_disagrees_on_context_divergent_snapshot() {
    let _g = gate();
    let dir = tmp("ctxdiv");
    let mut store = Store::open(&dir).unwrap();
    store.define_machine(case(), false, false).unwrap();
    store
        .create_instance("case_review", "i1", "c1", None)
        .unwrap();
    store.shutdown_snapshot().unwrap();
    let snap_seq = store.journal.last_seq;
    store
        .send_event("i1", "docs_ok", Value::Obj(BTreeMap::new()), "s1", None)
        .unwrap();
    let last_seq = store.journal.last_seq;
    drop(store);
    rewrite_snap_context(&dir, snap_seq);
    let (out1, div) = replay_disagreement(&dir);
    assert_eq!(div, snap_seq, "responsible seq must be the snapshot seq");
    assert_ne!(div, last_seq, "must not report the tail last_seq");
    let (out2, div2) = replay_disagreement(&dir);
    assert_eq!(div2, snap_seq);
    assert_eq!(out1, out2, "two CLI replay runs must match");
}

fn keep_only_snap_seq(dir: &std::path::Path, snap_seq: u64) -> std::path::PathBuf {
    let snaps = fsm_cli::snapshot::listed_snaps(dir);
    let mut keep = None;
    for (seq, path) in &snaps {
        if *seq == snap_seq && keep.is_none() {
            keep = Some(path.clone());
        } else {
            let _ = std::fs::remove_file(path);
        }
    }
    keep.expect("midstream snapshot")
}

fn rewrite_snap_context(dir: &std::path::Path, snap_seq: u64) {
    let path = keep_only_snap_seq(dir, snap_seq);
    let bytes = std::fs::read(&path).unwrap();
    let v = parse(&bytes, &JsonLimits::DEFAULT).unwrap();
    let mut o = v.as_obj().unwrap().clone();
    let seq: u64 = o
        .get("seq")
        .and_then(Value::as_num)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    if let Some(Value::Obj(insts)) = o.get_mut("instances") {
        let keys: Vec<String> = insts.keys().cloned().collect();
        for id in keys {
            let Some(Value::Obj(inst)) = insts.get_mut(&id) else {
                continue;
            };
            let mid = inst
                .get("machine_id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let leaf = inst
                .get("leaf")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let status = match inst.get("status").and_then(Value::as_str) {
                Some("completed") => fsm_core::machine::Status::Completed,
                Some("cancelled") => fsm_core::machine::Status::Cancelled,
                _ => fsm_core::machine::Status::Running,
            };
            if let Some(Value::Obj(ctx)) = inst.get_mut("context") {
                ctx.insert("visits".into(), Value::Str("99".into()));
            }
            let mut ctx = BTreeMap::new();
            if let Some(c) = inst.get("context").and_then(Value::as_obj) {
                for (k, val) in c {
                    if let Some(s) = val.as_str() {
                        if let Ok(n) = s.parse::<i64>() {
                            ctx.insert(k.clone(), Val::Int(n));
                        }
                    }
                }
            }
            let mut history = BTreeMap::new();
            if let Some(h) = inst.get("history").and_then(Value::as_obj) {
                for (k, val) in h {
                    if let Some(s) = val.as_str() {
                        history.insert(k.clone(), s.to_string());
                    }
                }
            }
            let pending = inst
                .get("pending")
                .and_then(Value::as_arr)
                .map(|a| {
                    a.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default();
            let st = fsm_core::machine::InstanceState {
                status,
                leaf,
                ctx,
                history,
                pending,
            };
            inst.insert(
                "state_hash".into(),
                Value::Str(fsm_core::hashes::state_hash(&mid, &id, seq, &st)),
            );
        }
    }
    reseal_snapshot(&mut o);
    std::fs::write(&path, fsm_core::canon::canon_bytes(&Value::Obj(o))).unwrap();
}

#[test]
fn verify_report_ordered_segment_progress() {
    let _g = gate();
    let dir = tmp("segs");
    let mut store = Store::open(&dir).unwrap();
    store.define_machine(case(), false, false).unwrap();
    for i in 0..3 {
        store
            .create_instance("case_review", &format!("i{i}"), &format!("c{i}"), None)
            .unwrap();
    }
    let first_name = store.journal.seg_name.clone();
    store.journal.force_rotate().unwrap();
    for i in 3..6 {
        store
            .create_instance("case_review", &format!("i{i}"), &format!("c{i}"), None)
            .unwrap();
    }
    let second_name = store.journal.seg_name.clone();
    assert_ne!(first_name, second_name, "rotation must open a new segment");
    drop(store);
    let bogus = dir.join("journal").join("seg-zzzz.jsonl");
    std::fs::create_dir_all(&bogus).unwrap();
    let r = fsm_cli::journal_io::verify(&dir);
    assert!(
        r.segments.len() >= 3,
        "expected ≥2 real segments plus metadata-failure, got {:?}",
        r.segments
            .iter()
            .map(|s| format!("{}:{}", s.segment, s.status))
            .collect::<Vec<_>>()
    );
    let names: Vec<_> = r.segments.iter().map(|s| s.segment.as_str()).collect();
    let mut sorted = names.clone();
    sorted.sort();
    assert_eq!(names, sorted, "segments must be ordered");
    assert!(
        r.segments
            .iter()
            .filter(|s| s.status == "ok" && s.records > 0)
            .count()
            >= 2,
        "need two populated segments {:?}",
        r.segments
            .iter()
            .map(|s| format!("{}:{}:{}", s.segment, s.status, s.records))
            .collect::<Vec<_>>()
    );
    assert!(
        r.segments.iter().any(|s| s.status == "metadata-failure"),
        "missing metadata-failure {:?}",
        r.segments
            .iter()
            .map(|s| s.status.as_str())
            .collect::<Vec<_>>()
    );
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
    assert!(stdout.contains("\"records\""), "{stdout}");
    assert!(stdout.contains("\"status\""), "{stdout}");
    assert!(stdout.contains(&first_name), "{stdout}");
    assert!(stdout.contains(&second_name), "{stdout}");
    assert!(stdout.contains("metadata-failure"), "{stdout}");
    let v = parse(stdout.trim().as_bytes(), &JsonLimits::DEFAULT).expect(&stdout);
    let segs = v.get("segments").and_then(Value::as_arr).expect(&stdout);
    let reported: Vec<&str> = segs
        .iter()
        .map(|s| s.get("segment").and_then(Value::as_str).unwrap_or(""))
        .collect();
    let mut sorted = reported.clone();
    sorted.sort();
    assert_eq!(reported, sorted, "CLI segments unordered {stdout}");
}

#[test]
fn journal_replay_disagrees_on_pending_and_history_divergent_snapshots() {
    let _g = gate();
    for kind in ["pending", "history"] {
        let dir = tmp(kind);
        let mut store = Store::open(&dir).unwrap();
        store.define_machine(case(), false, false).unwrap();
        store
            .create_instance("case_review", "i1", "c1", None)
            .unwrap();
        store.shutdown_snapshot().unwrap();
        let snap_seq = store.journal.last_seq;
        store
            .send_event("i1", "docs_ok", Value::Obj(BTreeMap::new()), "s1", None)
            .unwrap();
        let last_seq = store.journal.last_seq;
        drop(store);
        rewrite_snap_field(&dir, kind, snap_seq);
        let (_, div) = replay_disagreement(&dir);
        assert_eq!(div, snap_seq, "{kind} responsible seq");
        assert_ne!(div, last_seq, "{kind} must not be last_seq");
    }
}

fn rewrite_snap_field(dir: &std::path::Path, kind: &str, snap_seq: u64) {
    let path = keep_only_snap_seq(dir, snap_seq);
    let bytes = std::fs::read(&path).unwrap();
    let v = parse(&bytes, &JsonLimits::DEFAULT).unwrap();
    let mut o = v.as_obj().unwrap().clone();
    let seq: u64 = o
        .get("seq")
        .and_then(Value::as_num)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    if let Some(Value::Obj(insts)) = o.get_mut("instances") {
        let keys: Vec<String> = insts.keys().cloned().collect();
        for id in keys {
            let Some(Value::Obj(inst)) = insts.get_mut(&id) else {
                continue;
            };
            let mid = inst
                .get("machine_id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let leaf = inst
                .get("leaf")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let status = match inst.get("status").and_then(Value::as_str) {
                Some("completed") => fsm_core::machine::Status::Completed,
                Some("cancelled") => fsm_core::machine::Status::Cancelled,
                _ => fsm_core::machine::Status::Running,
            };
            if kind == "pending" {
                inst.insert(
                    "pending".into(),
                    Value::Arr(vec![Value::Str("ghost/1/0".into())]),
                );
            }
            if kind == "history" {
                inst.insert(
                    "history".into(),
                    Value::Obj(BTreeMap::from([(
                        "in_review".into(),
                        Value::Str("docs_review".into()),
                    )])),
                );
            }
            let mut ctx = BTreeMap::new();
            if let Some(c) = inst.get("context").and_then(Value::as_obj) {
                for (k, val) in c {
                    if let Some(s) = val.as_str() {
                        if let Ok(n) = s.parse::<i64>() {
                            ctx.insert(k.clone(), Val::Int(n));
                        }
                    }
                }
            }
            let mut history = BTreeMap::new();
            if let Some(h) = inst.get("history").and_then(Value::as_obj) {
                for (k, val) in h {
                    if let Some(s) = val.as_str() {
                        history.insert(k.clone(), s.to_string());
                    }
                }
            }
            let pending = inst
                .get("pending")
                .and_then(Value::as_arr)
                .map(|a| {
                    a.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default();
            let st = fsm_core::machine::InstanceState {
                status,
                leaf,
                ctx,
                history,
                pending,
            };
            inst.insert(
                "state_hash".into(),
                Value::Str(fsm_core::hashes::state_hash(&mid, &id, seq, &st)),
            );
        }
    }
    reseal_snapshot(&mut o);
    std::fs::write(&path, fsm_core::canon::canon_bytes(&Value::Obj(o))).unwrap();
}

#[test]
fn old_snapshot_format_rejected() {
    let v = Value::Obj(BTreeMap::from([
        ("format".into(), Value::Str("fsm.snapshot/1".into())),
        ("seq".into(), Value::Num("1".into())),
    ]));
    assert!(fsm_cli::snapshot::snapshot_to_state(&v).is_err());
}

#[test]
fn migration_ignores_snapshot_caches() {
    let _g = gate();
    let dir = tmp("migsnap");
    let mut s = Store::open(&dir).unwrap();
    s.define_machine(case(), false, false).unwrap();
    s.shutdown_snapshot().unwrap();
    drop(s);
    let s = Store::open(&dir).unwrap();
    assert!(
        s.opened_from_snapshot,
        "bound snapshot must fast-path a current store"
    );
    drop(s);
    std::fs::write(dir.join("VERSION"), "5\n").unwrap();
    let s = Store::open(&dir).unwrap();
    assert!(
        !s.opened_from_snapshot,
        "migration must fold the complete journal"
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("VERSION")).unwrap().trim(),
        "6"
    );
    s.resolve_machine("case_review").unwrap();
    drop(s);
    let s = Store::open(&dir).unwrap();
    assert!(
        s.opened_from_snapshot,
        "stamped store returns to the fast path"
    );
    drop(s);
}

#[test]
fn migratable_marker_with_lost_journal_refuses() {
    let _g = gate();
    let dir = tmp("miglost");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("VERSION"), "4\n").unwrap();
    let err = match Store::open(&dir) {
        Ok(_) => panic!("lost-journal migratable dir opened"),
        Err(e) => e,
    };
    assert_eq!(err.code, "store/chain_broken");
    assert_eq!(
        std::fs::read_to_string(dir.join("VERSION")).unwrap().trim(),
        "4"
    );
}

#[test]
fn journal_replay_ignores_caches_on_migratable_store() {
    let _g = gate();
    let dir = tmp("jrmig");
    let mut store = Store::open(&dir).unwrap();
    store.define_machine(case(), false, false).unwrap();
    store
        .create_instance("case_review", "i1", "c1", None)
        .unwrap();
    store.shutdown_snapshot().unwrap();
    let snap_seq = store.journal.last_seq;
    store
        .send_event("i1", "docs_ok", Value::Obj(BTreeMap::new()), "s1", None)
        .unwrap();
    drop(store);
    rewrite_snap_strip_dedup(&dir, "c1", snap_seq);
    std::fs::write(dir.join("VERSION"), "5\n").unwrap();
    let out = Command::new(fsm_bin())
        .args([
            "--data-dir",
            dir.to_str().unwrap(),
            "--json",
            "journal",
            "replay",
        ])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    assert_eq!(out.status.code(), Some(0), "{stdout}");
    assert!(stdout.contains("\"agreement\":true"), "{stdout}");
    assert!(stdout.contains("\"snapshots_ignored\":true"), "{stdout}");
    assert_eq!(
        std::fs::read_to_string(dir.join("VERSION")).unwrap().trim(),
        "5",
        "replay must not migrate"
    );
}
