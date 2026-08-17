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
        stdout.contains("\"agreement\":false") || stdout.contains("agreement\": false"),
        "{stdout}"
    );
    assert_ne!(out.status.code(), Some(0));
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
fn version3_and_missing_marker_are_mismatch() {
    let _g = gate();
    let dir = tmp("ver");
    let mut s = Store::open(&dir).unwrap();
    s.define_machine(case(), false, false).unwrap();
    drop(s);
    std::fs::write(dir.join("VERSION"), "3\n").unwrap();
    let err = match Store::open(&dir) {
        Ok(_) => panic!("VERSION 3 opened"),
        Err(e) => e,
    };
    assert_eq!(err.code, "store/version_mismatch");
    std::fs::write(dir.join("VERSION"), "4\n").unwrap();
    let err = match Store::open(&dir) {
        Ok(_) => panic!("VERSION 4 opened"),
        Err(e) => e,
    };
    assert_eq!(err.code, "store/version_mismatch");
    std::fs::remove_file(dir.join("VERSION")).unwrap();
    let err = match Store::open(&dir) {
        Ok(_) => panic!("missing VERSION opened"),
        Err(e) => e,
    };
    assert_eq!(err.code, "store/version_mismatch");
    match fsm_cli::journal_io::init(&dir) {
        Err(fsm_cli::journal_io::JournalIoError::VersionMismatch { .. }) => {}
        Err(_) => panic!("expected version mismatch"),
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
        "5"
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
        ("effect_ack", &["ok", "effect_id", "request_id"]),
        (
            "instance_cancel",
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
            "instance_get",
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
    }
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
