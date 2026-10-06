//! Trusted-journal shape/replay fixtures; these do not authenticate native receipts.

use std::collections::BTreeMap;

use fsm_core::hashes::{STATE_FORMAT, state_hash};
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::machine::{ActiveConfiguration, InstanceState, Status};
use fsm_core::record::execution::{Admission, ExecutionState};
use fsm_core::record::{Record, RecordKind, seal, verify_line, zeros};
use fsm_core::replay::{
    NopSink, StoreState, StoredMachine, fold_from, fold_with, state_root_at, state_root_at_v3,
};
use fsm_core::spec::compile_accepted;
use fsm_core::tree::Tree;

fn json(text: &str) -> Value {
    parse(text.as_bytes(), &JsonLimits::DEFAULT).unwrap()
}

fn before() -> StoreState {
    let definition = json(
        r#"{"format":"fsm.machine/1","name":"ownership","states":[{"name":"waiting"}],"initial":"waiting","context":[],"events":[],"transitions":[]}"#,
    );
    let compiled = compile_accepted(&definition).unwrap();
    let machine_id = compiled.machine_id.clone();
    let tree = Tree::for_machine(&compiled.spec);
    StoreState {
        machines: BTreeMap::from([(
            machine_id.clone(),
            StoredMachine {
                def: definition,
                compiled,
                tree,
            },
        )]),
        instances: BTreeMap::from([(
            "instance".into(),
            InstanceState {
                configuration: ActiveConfiguration::Sequential {
                    leaf: "waiting".into(),
                },
                status: Status::Running,
                ctx: BTreeMap::new(),
                history: BTreeMap::new(),
                deadlines: BTreeMap::new(),
                pending: vec!["effect".into()],
                invocations: BTreeMap::new(),
                signals: BTreeMap::new(),
            },
        )]),
        instance_machines: BTreeMap::from([("instance".into(), machine_id)]),
        execution: ExecutionState::new(Admission::Enabled),
        ..StoreState::default()
    }
}

fn claim_body() -> Value {
    json(
        r#"{"run_id":1,"instance_id":"instance","effect_id":"effect","attempt":1,
      "handler_fingerprint":"sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
      "retry":{"attempts":3,"backoff_ms":10,"max_backoff_ms":40,"on":["timeout"]},
      "domain":{"backend":"linux-systemd/1","namespace":"0123456789abcdef0123456789abcdef","allocation":7,
        "boot":"01234567-89ab-cdef-0123-456789abcdef","cgroup":{"device":0,"inode":42},"authority":{"device":8,"inode":43},"generation":9}}"#,
    )
}

fn append(
    state: &StoreState,
    kind: RecordKind,
    body: Value,
    request_id: &str,
    timestamp: i64,
) -> Record {
    let mut body = body.as_obj().unwrap().clone();
    body.insert("request_id".into(), Value::Str(request_id.into()));
    body.insert(
        "request_fp".into(),
        Value::Str(format!("sha256:{}", "c".repeat(64))),
    );
    let record = seal(
        state.last_seq + 1,
        timestamp,
        kind,
        Value::Obj(body),
        &state.last_hash,
    );
    verify_line(&record.to_line(), record.seq, &state.last_hash).unwrap();
    record
}

fn stopped_body(status: &str) -> Value {
    let metadata = claim_body();
    Value::Obj(BTreeMap::from([
        ("run_id".into(), Value::Num("1".into())),
        ("instance_id".into(), Value::Str("instance".into())),
        ("effect_id".into(), Value::Str("effect".into())),
        (
            "handler_fingerprint".into(),
            metadata.get("handler_fingerprint").unwrap().clone(),
        ),
        (
            "closure".into(),
            Value::Obj(BTreeMap::from([
                ("run_id".into(), Value::Num("1".into())),
                ("domain".into(), metadata.get("domain").unwrap().clone()),
                (
                    "receipt".into(),
                    Value::Str(format!("sha256:{}", "b".repeat(64))),
                ),
            ])),
        ),
        (
            "outcome".into(),
            Value::Obj(BTreeMap::from([
                ("status".into(), Value::Str(status.into())),
                ("result".into(), Value::Null),
            ])),
        ),
    ]))
}

fn settlement_body(state: &StoreState, disposition: &str) -> Value {
    let mut instance = state.instances["instance"].clone();
    if disposition == "acked" {
        instance.pending.clear();
    }
    let machine_id = &state.instance_machines["instance"];
    let mut body = BTreeMap::from([
        ("run_id".into(), Value::Num("1".into())),
        ("instance_id".into(), Value::Str("instance".into())),
        ("effect_id".into(), Value::Str("effect".into())),
        ("disposition".into(), Value::Str(disposition.into())),
        ("state_format".into(), Value::Str(STATE_FORMAT.into())),
        (
            "state_hash".into(),
            Value::Str(state_hash(
                machine_id,
                "instance",
                state.last_seq + 1,
                &instance,
            )),
        ),
    ]);
    if disposition != "interrupted" {
        body.insert(
            "outcome".into(),
            Value::Str(
                if disposition == "acked" {
                    "ok"
                } else {
                    "failed"
                }
                .into(),
            ),
        );
        body.insert("result".into(), Value::Null);
    }
    if disposition == "attempted" {
        body.insert("attempt".into(), Value::Num("1".into()));
    }
    Value::Obj(body)
}

fn fold(state: StoreState, record: Record) -> StoreState {
    fold_from(state, [record], &mut NopSink).unwrap()
}

#[test]
fn claimed_stopped_and_atomic_failed_settlement_replay_durable_ownership() {
    let initial = before();
    let record = append(
        &initial,
        RecordKind::ExecutionClaimed,
        claim_body(),
        "claim",
        100,
    );
    let claimed = fold(initial.clone(), record.clone());
    let original = format!("sha256:{}", record.hash);
    let claim = claimed.execution.claim_for("instance", "effect").unwrap();
    assert_eq!(
        claimed.execution.claim_record_hash(claim),
        Some(original.as_str())
    );
    let decoded = ExecutionState::from_value(&claimed.execution.to_value()).unwrap();
    assert_eq!(decoded.claim_record_hash(claim), None);
    let mut logical_only = claimed.clone();
    logical_only.execution = decoded;
    assert_eq!(state_root_at(&claimed, 1), state_root_at(&logical_only, 1));
    assert_eq!(claimed.execution.run_high_water(), 1);
    assert_eq!(claimed.instances, initial.instances);
    let mut without_ownership = claimed.clone();
    without_ownership.execution = initial.execution.clone();
    assert_ne!(
        state_root_at(&claimed, 1),
        state_root_at(&without_ownership, 1)
    );
    assert_eq!(
        state_root_at_v3(&claimed, 1),
        state_root_at_v3(&without_ownership, 1)
    );
    let stop_record = append(
        &claimed,
        RecordKind::ExecutionStopped,
        stopped_body("timeout"),
        "stop",
        100,
    );
    let stopped = fold(claimed.clone(), stop_record.clone());
    assert!(
        stopped
            .execution
            .stopped_for("instance", "effect")
            .is_some()
    );
    assert!(stopped.execution.claim_for("instance", "effect").is_some());
    let settle_record = append(
        &stopped,
        RecordKind::ExecutionSettled,
        settlement_body(&stopped, "attempted"),
        "settle",
        100,
    );
    let settled = fold(stopped.clone(), settle_record.clone());
    assert_eq!(settled.execution.claim_record_hash(claim), None);
    assert_eq!(settled.instances["instance"].pending, ["effect"]);
    assert_eq!(settled.execution.failed_count("instance", "effect"), 1);
    assert!(settled.execution.claim_for("instance", "effect").is_none());
    assert_eq!(settled.execution.run_high_water(), 1);
    let recovered = fold_from(
        initial,
        [record, stop_record, settle_record.clone()],
        &mut NopSink,
    )
    .unwrap();
    assert_eq!(recovered.execution, settled.execution);
    assert!(fold_from(settled, [settle_record], &mut NopSink).is_err());
    let mut next_body = claim_body().as_obj().unwrap().clone();
    next_body.insert("run_id".into(), Value::Num("2".into()));
    next_body.insert("attempt".into(), Value::Num("2".into()));
    let early = append(
        &recovered,
        RecordKind::ExecutionClaimed,
        Value::Obj(next_body.clone()),
        "early",
        109,
    );
    assert!(fold_from(recovered.clone(), [early], &mut NopSink).is_err());
    let due = append(
        &recovered,
        RecordKind::ExecutionClaimed,
        Value::Obj(next_body),
        "due",
        110,
    );
    assert_eq!(fold(recovered, due).execution.run_high_water(), 2);
}

#[test]
fn one_settlement_consumes_ownership_and_applies_ack_exactly_once() {
    let initial = before();
    let record = append(
        &initial,
        RecordKind::ExecutionClaimed,
        claim_body(),
        "claim",
        100,
    );
    let claimed = fold(initial, record);
    let record = append(
        &claimed,
        RecordKind::ExecutionStopped,
        stopped_body("ok"),
        "stop",
        100,
    );
    let stopped = fold(claimed, record);
    let record = append(
        &stopped,
        RecordKind::ExecutionSettled,
        settlement_body(&stopped, "acked"),
        "settle",
        100,
    );
    let settled = fold(stopped, record.clone());
    assert!(settled.instances["instance"].pending.is_empty());
    assert!(settled.execution.claim_for("instance", "effect").is_none());
    assert!(fold_from(settled, [record], &mut NopSink).is_err());
}

#[test]
fn interruption_keeps_pending_effect_and_does_not_invent_failed_attempts() {
    let initial = before();
    let record = append(
        &initial,
        RecordKind::ExecutionClaimed,
        claim_body(),
        "claim",
        100,
    );
    let claimed = fold(initial, record);
    let record = append(
        &claimed,
        RecordKind::ExecutionStopped,
        stopped_body("interrupted"),
        "stop",
        100,
    );
    let stopped = fold(claimed, record);
    let record = append(
        &stopped,
        RecordKind::ExecutionSettled,
        settlement_body(&stopped, "interrupted"),
        "settle",
        100,
    );
    let settled = fold(stopped, record);
    assert_eq!(settled.instances["instance"].pending, ["effect"]);
    assert_eq!(settled.execution.failed_count("instance", "effect"), 0);
    assert!(settled.execution.claim_for("instance", "effect").is_none());
}

#[test]
fn legacy_genesis_is_quarantined_and_admission_binds_its_exact_prefix() {
    let mut genesis = BTreeMap::from([
        ("format".into(), Value::Str("fsm.journal/1".into())),
        ("created_ts".into(), Value::Num("0".into())),
        ("limits".into(), fsm_core::record::limits_value()),
    ]);
    let old = seal(
        0,
        0,
        RecordKind::Genesis,
        Value::Obj(genesis.clone()),
        &zeros(),
    );
    let legacy = fold_with([old.clone()], &mut NopSink).unwrap();
    assert_eq!(legacy.execution.admission(), Admission::Quarantined);
    genesis.insert("execution_admission".into(), Value::Str("enabled".into()));
    let new = seal(0, 0, RecordKind::Genesis, Value::Obj(genesis), &zeros());
    assert_eq!(
        fold_with([new], &mut NopSink)
            .unwrap()
            .execution
            .admission(),
        Admission::Enabled
    );
    let previous_head = format!("sha256:{}", old.hash);
    let body = Value::Obj(BTreeMap::from([
        ("previous_head".into(), Value::Str(previous_head.clone())),
        (
            "quiescence".into(),
            Value::Obj(BTreeMap::from([
                ("previous_head".into(), Value::Str(previous_head)),
                ("domain".into(), claim_body().get("domain").unwrap().clone()),
                (
                    "receipt".into(),
                    Value::Str(format!("sha256:{}", "b".repeat(64))),
                ),
            ])),
        ),
    ]));
    let record = append(
        &legacy,
        RecordKind::ExecutionEnabled,
        body.clone(),
        "enable",
        100,
    );
    let admitted = fold(legacy.clone(), record);
    assert_eq!(admitted.execution.admission(), Admission::Enabled);
    assert_eq!(admitted.execution.run_high_water(), 0);
    let mut stale = body.as_obj().unwrap().clone();
    stale.insert(
        "previous_head".into(),
        Value::Str(format!("sha256:{}", "e".repeat(64))),
    );
    let record = append(
        &legacy,
        RecordKind::ExecutionEnabled,
        Value::Obj(stale),
        "stale",
        100,
    );
    assert!(fold_from(legacy, [record], &mut NopSink).is_err());
}

#[test]
fn claim_request_id_accepts_4096_utf8_bytes_and_refuses_4097() {
    for length in [4096, 4097] {
        let initial = before();
        let mut body = claim_body().as_obj().unwrap().clone();
        body.insert(
            "request_id".into(),
            Value::Str(format!("{}{}", "é".repeat(2048), "x".repeat(length - 4096))),
        );
        body.insert(
            "request_fp".into(),
            Value::Str(format!("sha256:{}", "c".repeat(64))),
        );
        let record = seal(
            1,
            100,
            RecordKind::ExecutionClaimed,
            Value::Obj(body),
            &initial.last_hash,
        );
        assert_eq!(
            verify_line(&record.to_line(), 1, &initial.last_hash).is_ok(),
            length == 4096
        );
        assert_eq!(
            fold_from(initial, [record], &mut NopSink).is_ok(),
            length == 4096
        );
    }
}
