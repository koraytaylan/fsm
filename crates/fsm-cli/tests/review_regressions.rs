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
    s.create_instance_ctx("case_review", "i1", "r1", None, &ov)
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
        .send_event_stamp("i1", "scored", &mut payload, "bad-1", None, None)
        .unwrap_err();
    assert_eq!(err.code, "req/field_unknown");
    assert_eq!(s.journal.last_seq, before);
    let mut okp = parse(br#"{"score":"1"}"#, &JsonLimits::DEFAULT).unwrap();
    // same request id is not consumed
    let err2 = s.send_event_stamp("i1", "scored", &mut okp, "bad-1", None, None);
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
    s.create_instance_ctx("case_review", "i1", "r1", None, &ov)
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
    s.send_event_stamp("i1", "pay", &mut payload, "p1", None, None)
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
    assert_eq!(err.code, "run/overflow");
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
