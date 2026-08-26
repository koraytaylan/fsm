use super::*;
use crate::clock::FixedClock;
use fsm_core::hashes::state_hash;
use fsm_core::json::{JsonLimits, parse};

/// A scratch directory that removes itself. A suite that leaks one per run
/// exhausts a long-lived machine's tmpfs inodes long before it exhausts its
/// bytes, and the failure looks like a broken toolchain rather than a leaky
/// test.
struct Scratch(std::path::PathBuf);

impl std::ops::Deref for Scratch {
    type Target = std::path::Path;
    fn deref(&self) -> &std::path::Path {
        &self.0
    }
}

impl AsRef<std::path::Path> for Scratch {
    fn as_ref(&self) -> &std::path::Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn tmp() -> Scratch {
    let p = std::env::temp_dir().join(format!(
        "fsm-tools-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&p).unwrap();
    Scratch(p)
}

fn case() -> Value {
    parse(
        include_bytes!("../../../../fsm-core/tests/fixtures/machines/case_review.json"),
        &JsonLimits::DEFAULT,
    )
    .unwrap()
}

#[test]
fn resolution_and_post_state() {
    let dir = tmp();
    let mut store = Store::open(&dir).unwrap();
    let mut clock = FixedClock::new(1000, 1000);
    let created = run_machine_create(
        &mut store,
        &mut clock,
        &Value::Obj(BTreeMap::from([("spec".into(), case())])),
    )
    .unwrap();
    let mid = created
        .get("machine_id")
        .and_then(Value::as_str)
        .unwrap()
        .to_string();
    store.resolve_machine(&mid).unwrap();
    store
        .resolve_machine(&mid[mid.find(':').unwrap() + 1..][..12.min(mid.len())])
        .ok();
    let hex = mid.split(':').next_back().unwrap()[..12].to_string();
    store.resolve_machine(&hex).unwrap();
    store.resolve_machine("case_review").unwrap();
    let v2 = {
        let mut c = case();
        if let Value::Obj(o) = &mut c {
            o.insert("description".into(), Value::Str("other".into()));
        }
        c
    };
    run_machine_create(
        &mut store,
        &mut clock,
        &Value::Obj(BTreeMap::from([("spec".into(), v2)])),
    )
    .unwrap();
    let err = store.resolve_machine("case_review").unwrap_err();
    assert_eq!(err.code, "req/machine_ambiguous");
    let short = store.resolve_machine("abc").unwrap_err();
    assert!(short.code.contains("not_found") || short.hint.contains("12"));

    // fresh store with one version for send
    let dir = tmp();
    let mut store = Store::open(&dir).unwrap();
    run_machine_create(
        &mut store,
        &mut clock,
        &Value::Obj(BTreeMap::from([("spec".into(), case())])),
    )
    .unwrap();
    let inst = run_instance_create(
        &mut store,
        &mut clock,
        &Value::Obj(BTreeMap::from([
            ("machine".into(), Value::Str("case_review".into())),
            ("request_id".into(), Value::Str("c1".into())),
        ])),
    )
    .unwrap();
    let iid = inst
        .get("instance_id")
        .and_then(Value::as_str)
        .unwrap()
        .to_string();
    let sent = run_instance_send(
        &mut store,
        &mut clock,
        &Value::Obj(BTreeMap::from([
            ("instance_id".into(), Value::Str(iid.clone())),
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
    for k in [
        "state",
        "configuration",
        "context",
        "effects_pending",
        "trace",
        "enabled_events",
        "seq",
        "state_hash",
    ] {
        assert!(sent.get(k).is_some(), "missing {k}");
    }
    let inst_st = store.state.instances.get(&iid).unwrap();
    let mid = store.state.instance_machines.get(&iid).unwrap();
    let recomputed = state_hash(mid, &iid, store.journal.last_seq, inst_st);
    assert_eq!(
        sent.get("state_hash").and_then(Value::as_str),
        Some(recomputed.as_str())
    );
    let again = run_instance_send(
        &mut store,
        &mut clock,
        &Value::Obj(BTreeMap::from([
            ("instance_id".into(), Value::Str(iid)),
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
    assert_eq!(again.get("duplicate").and_then(Value::as_bool), Some(true));
}

#[test]
fn completeness_and_flags() {
    let dir = tmp();
    let mut store = Store::open(&dir).unwrap();
    let mut clock = FixedClock::new(1, 1);
    for t in registry() {
        let args =
            match t.name {
                "machine_create" => Value::Obj(BTreeMap::from([("spec".into(), case())])),
                "machine_list" | "instance_list" => Value::Obj(BTreeMap::new()),
                "machine_get" | "machine_analyze" | "machine_diagram" => Value::Obj(
                    BTreeMap::from([("machine".into(), Value::Str("case_review".into()))]),
                ),
                "instance_create" => Value::Obj(BTreeMap::from([
                    ("machine".into(), Value::Str("case_review".into())),
                    ("request_id".into(), Value::Str("cx".into())),
                ])),
                "instance_send" => Value::Obj(BTreeMap::from([
                    ("instance_id".into(), Value::Str("inst-cx".into())),
                    (
                        "event".into(),
                        Value::Obj(BTreeMap::from([(
                            "name".into(),
                            Value::Str("docs_ok".into()),
                        )])),
                    ),
                    ("request_id".into(), Value::Str("sx".into())),
                ])),
                "effect_ack" => Value::Obj(BTreeMap::from([
                    ("instance_id".into(), Value::Str("inst-cx".into())),
                    ("effect_id".into(), Value::Str("none".into())),
                    ("outcome".into(), Value::Str("ok".into())),
                    ("request_id".into(), Value::Str("ax".into())),
                ])),
                "instance_cancel" => Value::Obj(BTreeMap::from([
                    ("instance_id".into(), Value::Str("inst-cx".into())),
                    ("reason".into(), Value::Str("x".into())),
                    ("request_id".into(), Value::Str("kx".into())),
                ])),
                "instance_get" | "instance_history" => Value::Obj(BTreeMap::from([(
                    "instance_id".into(),
                    Value::Str("inst-cx".into()),
                )])),
                "simulate" => Value::Obj(BTreeMap::from([
                    ("machine".into(), Value::Str("case_review".into())),
                    ("events".into(), Value::Arr(vec![])),
                ])),
                _ => Value::Obj(BTreeMap::new()),
            };
        let r = (t.run)(&mut store, &mut clock, &args);
        if let Err(e) = r {
            assert_ne!(e.code, "internal/unimplemented", "{}", t.name);
        }
    }
    let dry = run_machine_create(
        &mut Store::open(&tmp()).unwrap(),
        &mut clock,
        &Value::Obj(BTreeMap::from([
            ("spec".into(), case()),
            ("dry_run".into(), Value::Bool(true)),
        ])),
    )
    .unwrap();
    assert_eq!(dry.get("dry_run").and_then(Value::as_bool), Some(true));
}
