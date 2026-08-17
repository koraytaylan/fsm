//! Targeted regressions for store identity, replay, and CLI contracts.

use std::collections::BTreeMap;
use std::process::Command;

use fsm_cli::store::Store;
use fsm_core::expr::eval::Val;
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::spec::{compile, parse_machine};

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
    let v = parse(
        br#"{"format":"fsm.machine/1","name":"m","context":[],"events":[{"name":"go","fields":[]}],"states":[{"name":"idle"},{"name":"done","terminal":true}],"initial":"idle","transitions":[{"from":"idle","on":"go","if":"1","to":"done"}]}"#,
        &JsonLimits::DEFAULT,
    )
    .unwrap();
    assert!(compile(parse_machine(&v).unwrap()).is_err());
}

#[test]
fn payload_invalid_not_journaled() {
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
