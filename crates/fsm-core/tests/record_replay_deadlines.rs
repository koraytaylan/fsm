//! Hash-chain and replay proofs for deadline-poll records.
//!
//! Applied, not-due, and rejected polls must replay with request-id claims
//! while legacy state and root hash discriminators remain readable.

use std::collections::BTreeMap;

use fsm_core::expr::eval::Budget;
use fsm_core::hashes::{
    STATE_FORMAT, configuration_value, domain_hash, legacy_state_hash, state_hash,
};
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::machine::{ActiveConfiguration, CompiledMachine, InstanceState, Status};
use fsm_core::record::{Record, RecordKind, limits_value, seal, verify_line, zeros};
use fsm_core::replay::{NopSink, ReplayError, fold_with};
use fsm_core::step::{DeadlineOutcome, Rejection, create, poll_deadline};
use fsm_core::tree::Tree;

const APPLIED_MACHINE: &str = r#"{
    "format":"fsm.machine/1","name":"timed_replay",
    "states":[{"name":"waiting"},{"name":"done","terminal":true}],
    "initial":"waiting","context":[],"events":[],"transitions":[],
    "deadlines":[{"name":"expire","from":"waiting","after":"dur(5, ms)","to":"done"}]
}"#;

const REJECTED_MACHINE: &str = r#"{
    "format":"fsm.machine/1","name":"timed_reject_replay",
    "states":[{"name":"waiting"},{"name":"done"}],
    "initial":"waiting",
    "context":[{"name":"n","ty":"int","init":"9223372036854775807"}],
    "events":[],"transitions":[],
    "deadlines":[{
        "name":"expire","from":"waiting","after":"dur(5, ms)","to":"done",
        "do":[{"target":"n","value":"ctx.n + 1"}]
    }]
}"#;

struct Fixture {
    records: Vec<Record>,
    machine_id: String,
    machine: CompiledMachine,
    tree: Tree,
    initial: InstanceState,
}

fn fixture(source: &str) -> Fixture {
    let definition = parse(source.as_bytes(), &JsonLimits::DEFAULT).unwrap();
    let machine = fsm_core::spec::compile_accepted(&definition).unwrap();
    let tree = Tree::for_machine(&machine.spec);
    let machine_id = machine.machine_id.clone();
    let created = create(&machine, &tree, &BTreeMap::new(), 100).unwrap();
    let initial = InstanceState {
        status: created.status_after,
        configuration: created.configuration_after,
        ctx: created.ctx_after,
        history: created.history_after,
        deadlines: created.deadlines_after,
        pending: Vec::new(),
    };

    let mut records = Vec::new();
    push_record(
        &mut records,
        0,
        RecordKind::Genesis,
        Value::Obj(BTreeMap::from([
            ("format".into(), Value::Str("fsm.journal/1".into())),
            ("created_ts".into(), Value::Num("0".into())),
            ("limits".into(), limits_value()),
        ])),
    );
    push_record(
        &mut records,
        1,
        RecordKind::MachineDefined,
        Value::Obj(BTreeMap::from([
            ("machine_id".into(), Value::Str(machine_id.clone())),
            ("def".into(), definition),
        ])),
    );
    push_record(
        &mut records,
        100,
        RecordKind::InstanceCreated,
        Value::Obj(BTreeMap::from([
            ("instance_id".into(), Value::Str("i".into())),
            ("machine_id".into(), Value::Str(machine_id.clone())),
            ("request_id".into(), Value::Str("create".into())),
            (
                "state_hash".into(),
                Value::Str(state_hash(&machine_id, "i", 2, &initial)),
            ),
            ("state_format".into(), Value::Str(STATE_FORMAT.into())),
            (
                "configuration".into(),
                configuration_value(&initial.configuration),
            ),
            ("overrides".into(), Value::Obj(BTreeMap::new())),
        ])),
    );

    Fixture {
        records,
        machine_id,
        machine,
        tree,
        initial,
    }
}

fn push_record(records: &mut Vec<Record>, ts: i64, kind: RecordKind, body: Value) {
    let seq = records.len() as u64;
    let previous = records
        .last()
        .map(|record| record.hash.clone())
        .unwrap_or_else(zeros);
    records.push(seal(seq, ts, kind, body, &previous));
}

fn verify_chain(records: &[Record]) {
    let mut previous = zeros();
    for (seq, record) in records.iter().enumerate() {
        verify_line(&record.to_line(), seq as u64, &previous).unwrap();
        previous = record.hash.clone();
    }
}

fn deadline_details(rejection: &Rejection, request_id: &str) -> Value {
    let mut details = BTreeMap::new();
    if let Some(block) = &rejection.block {
        details.insert("block".into(), Value::Str(block.clone()));
    }
    if let Some(cause) = rejection.cause {
        details.insert("cause".into(), Value::Str(cause.into()));
    }
    if let (Some(source), Some(index)) = (&rejection.source_state, rejection.transition_idx) {
        details.insert("source_state".into(), Value::Str(source.clone()));
        details.insert("transition_idx".into(), Value::Num(index.to_string()));
    }
    details.insert("trace".into(), rejection.trace.to_value());
    details.insert("request_id".into(), Value::Str(request_id.into()));
    Value::Obj(details)
}

#[test]
fn replay_verifies_and_applies_due_deadline() {
    let mut fixture = fixture(APPLIED_MACHINE);
    let mut budget = Budget::new(4096);
    let applied = match poll_deadline(
        &fixture.machine,
        &fixture.tree,
        &fixture.initial,
        105,
        &mut budget,
    ) {
        DeadlineOutcome::Applied(applied) => applied,
        outcome => panic!("{outcome:?}"),
    };
    let new = InstanceState {
        status: applied.transition.status_after,
        configuration: applied.transition.configuration_after.clone(),
        ctx: applied.transition.ctx_after.clone(),
        history: applied.transition.history_after.clone(),
        deadlines: applied.transition.deadlines_after.clone(),
        pending: Vec::new(),
    };
    push_record(
        &mut fixture.records,
        105,
        RecordKind::DeadlineApplied,
        Value::Obj(BTreeMap::from([
            ("instance_id".into(), Value::Str("i".into())),
            ("request_id".into(), Value::Str("poll-1".into())),
            ("deadline".into(), Value::Str(applied.deadline.name)),
            (
                "deadline_idx".into(),
                Value::Num(applied.deadline.deadline_idx.to_string()),
            ),
            (
                "due_ms".into(),
                Value::Num(applied.deadline.due_ms.to_string()),
            ),
            (
                "state_hash".into(),
                Value::Str(state_hash(&fixture.machine_id, "i", 3, &new)),
            ),
            ("state_format".into(), Value::Str(STATE_FORMAT.into())),
            (
                "source_state".into(),
                Value::Str(applied.transition.source_state),
            ),
            (
                "exited".into(),
                Value::Arr(
                    applied
                        .transition
                        .exited
                        .into_iter()
                        .map(Value::Str)
                        .collect(),
                ),
            ),
            (
                "entered".into(),
                Value::Arr(
                    applied
                        .transition
                        .entered
                        .into_iter()
                        .map(Value::Str)
                        .collect(),
                ),
            ),
        ])),
    );
    verify_chain(&fixture.records);

    let state = fold_with(fixture.records, &mut NopSink).unwrap();
    let instance = state.instances.get("i").unwrap();
    assert_eq!(instance.status, Status::Completed);
    assert!(instance.deadlines.is_empty());
    assert!(matches!(
        instance.configuration,
        ActiveConfiguration::Sequential { ref leaf } if leaf == "done"
    ));
    assert_eq!(state.dedup.get("poll-1").unwrap().seq, 3);
}

#[test]
fn replay_verifies_not_due_observation_and_claims_request_id() {
    let mut fixture = fixture(APPLIED_MACHINE);
    push_record(
        &mut fixture.records,
        104,
        RecordKind::DeadlineNotDue,
        Value::Obj(BTreeMap::from([
            ("instance_id".into(), Value::Str("i".into())),
            ("request_id".into(), Value::Str("poll-idle".into())),
            (
                "state_hash".into(),
                Value::Str(state_hash(&fixture.machine_id, "i", 3, &fixture.initial)),
            ),
            ("state_format".into(), Value::Str(STATE_FORMAT.into())),
            ("next_deadline".into(), Value::Str("expire".into())),
            ("next_deadline_idx".into(), Value::Num("0".into())),
            ("next_due_ms".into(), Value::Num("105".into())),
        ])),
    );
    verify_chain(&fixture.records);
    let state = fold_with(fixture.records.clone(), &mut NopSink).unwrap();
    assert_eq!(state.dedup.get("poll-idle").unwrap().seq, 3);
    assert_eq!(state.instances.get("i").unwrap().deadlines["expire"], 105);

    push_record(
        &mut fixture.records,
        104,
        RecordKind::DeadlineNotDue,
        Value::Obj(BTreeMap::from([
            ("instance_id".into(), Value::Str("i".into())),
            ("request_id".into(), Value::Str("poll-idle".into())),
            (
                "state_hash".into(),
                Value::Str(state_hash(&fixture.machine_id, "i", 4, &fixture.initial)),
            ),
            ("state_format".into(), Value::Str(STATE_FORMAT.into())),
            ("next_deadline".into(), Value::Str("expire".into())),
            ("next_deadline_idx".into(), Value::Num("0".into())),
            ("next_due_ms".into(), Value::Num("105".into())),
        ])),
    );
    assert!(matches!(
        fold_with(fixture.records, &mut NopSink),
        Err(ReplayError::FieldMismatch {
            field: "request_id",
            ..
        })
    ));
}

#[test]
fn replay_verifies_deadline_rejection_without_mutating_state() {
    let mut fixture = fixture(REJECTED_MACHINE);
    let mut budget = Budget::new(4096);
    let rejected = match poll_deadline(
        &fixture.machine,
        &fixture.tree,
        &fixture.initial,
        105,
        &mut budget,
    ) {
        DeadlineOutcome::Rejected(rejected) => rejected,
        outcome => panic!("{outcome:?}"),
    };
    let selected = rejected.deadline.unwrap();
    let rejection = rejected.rejection;
    let mut body = BTreeMap::from([
        ("instance_id".into(), Value::Str("i".into())),
        ("request_id".into(), Value::Str("poll-reject".into())),
        ("deadline".into(), Value::Str(selected.name)),
        (
            "deadline_idx".into(),
            Value::Num(selected.deadline_idx.to_string()),
        ),
        ("due_ms".into(), Value::Num(selected.due_ms.to_string())),
        (
            "state_hash".into(),
            Value::Str(state_hash(&fixture.machine_id, "i", 3, &fixture.initial)),
        ),
        ("state_format".into(), Value::Str(STATE_FORMAT.into())),
        ("code".into(), Value::Str(rejection.code.into())),
        ("message".into(), Value::Str(rejection.message.clone())),
        ("hint".into(), Value::Str(rejection.hint.clone())),
        (
            "details".into(),
            deadline_details(&rejection, "poll-reject"),
        ),
    ]);
    if let Some((start, end)) = rejection.span {
        body.insert(
            "span".into(),
            Value::Obj(BTreeMap::from([
                ("start".into(), Value::Num(start.to_string())),
                ("end".into(), Value::Num(end.to_string())),
            ])),
        );
    }
    push_record(
        &mut fixture.records,
        105,
        RecordKind::DeadlineRejected,
        Value::Obj(body),
    );
    verify_chain(&fixture.records);

    let state = fold_with(fixture.records, &mut NopSink).unwrap();
    let instance = state.instances.get("i").unwrap();
    assert_eq!(instance, &fixture.initial);
    assert_eq!(state.dedup.get("poll-reject").unwrap().seq, 3);
}

#[test]
fn replay_verifies_unselected_poll_rejection_as_request_rejected() {
    let mut fixture = fixture(APPLIED_MACHINE);
    let mut cancelled = fixture.initial.clone();
    cancelled.status = Status::Cancelled;
    cancelled.deadlines.clear();
    push_record(
        &mut fixture.records,
        101,
        RecordKind::InstanceCancelled,
        Value::Obj(BTreeMap::from([
            ("instance_id".into(), Value::Str("i".into())),
            ("request_id".into(), Value::Str("cancel".into())),
            ("reason".into(), Value::Str("test".into())),
            (
                "state_hash".into(),
                Value::Str(state_hash(&fixture.machine_id, "i", 3, &cancelled)),
            ),
            ("state_format".into(), Value::Str(STATE_FORMAT.into())),
        ])),
    );

    let mut budget = Budget::new(4096);
    let rejection = match poll_deadline(
        &fixture.machine,
        &fixture.tree,
        &cancelled,
        102,
        &mut budget,
    ) {
        DeadlineOutcome::Rejected(rejected) if rejected.deadline.is_none() => rejected.rejection,
        outcome => panic!("{outcome:?}"),
    };
    push_record(
        &mut fixture.records,
        102,
        RecordKind::RequestRejected,
        Value::Obj(BTreeMap::from([
            ("instance_id".into(), Value::Str("i".into())),
            ("request_id".into(), Value::Str("poll-cancelled".into())),
            ("operation".into(), Value::Str("poll_deadline".into())),
            (
                "state_hash".into(),
                Value::Str(state_hash(&fixture.machine_id, "i", 4, &cancelled)),
            ),
            ("state_format".into(), Value::Str(STATE_FORMAT.into())),
            ("code".into(), Value::Str(rejection.code.into())),
            ("message".into(), Value::Str(rejection.message.clone())),
            ("hint".into(), Value::Str(rejection.hint.clone())),
            (
                "details".into(),
                deadline_details(&rejection, "poll-cancelled"),
            ),
        ])),
    );
    verify_chain(&fixture.records);

    let state = fold_with(fixture.records, &mut NopSink).unwrap();
    assert_eq!(state.instances.get("i").unwrap(), &cancelled);
    assert_eq!(state.dedup.get("poll-cancelled").unwrap().seq, 4);
}

#[test]
fn replay_keeps_legacy_state_and_root_discriminators_readable() {
    let definition = parse(
        br#"{"format":"fsm.machine/1","name":"legacy","states":[{"name":"only"}],"initial":"only","context":[],"events":[],"transitions":[]}"#,
        &JsonLimits::DEFAULT,
    )
    .unwrap();
    let machine = fsm_core::spec::compile_accepted(&definition).unwrap();
    let machine_id = machine.machine_id.clone();
    let tree = Tree::for_machine(&machine.spec);
    let created = create(&machine, &tree, &BTreeMap::new(), 0).unwrap();
    let instance = InstanceState {
        status: created.status_after,
        configuration: created.configuration_after,
        ctx: created.ctx_after,
        history: created.history_after,
        deadlines: created.deadlines_after,
        pending: Vec::new(),
    };

    let mut records = Vec::new();
    push_record(
        &mut records,
        0,
        RecordKind::Genesis,
        Value::Obj(BTreeMap::from([
            ("format".into(), Value::Str("fsm.journal/1".into())),
            ("created_ts".into(), Value::Num("0".into())),
            ("limits".into(), limits_value()),
        ])),
    );
    push_record(
        &mut records,
        1,
        RecordKind::MachineDefined,
        Value::Obj(BTreeMap::from([
            ("machine_id".into(), Value::Str(machine_id.clone())),
            ("def".into(), definition.clone()),
        ])),
    );
    push_record(
        &mut records,
        2,
        RecordKind::InstanceCreated,
        Value::Obj(BTreeMap::from([
            ("instance_id".into(), Value::Str("i".into())),
            ("machine_id".into(), Value::Str(machine_id.clone())),
            ("request_id".into(), Value::Str("legacy-create".into())),
            ("leaf".into(), Value::Str("only".into())),
            (
                "state_hash".into(),
                Value::Str(legacy_state_hash(&machine_id, "i", 2, &instance).unwrap()),
            ),
            ("overrides".into(), Value::Obj(BTreeMap::new())),
        ])),
    );

    let instance_root = Value::Obj(BTreeMap::from([
        ("leaf".into(), Value::Str("only".into())),
        ("status".into(), Value::Str("running".into())),
        ("machine_id".into(), Value::Str(machine_id.clone())),
        ("context".into(), Value::Obj(BTreeMap::new())),
        ("history".into(), Value::Obj(BTreeMap::new())),
        ("pending".into(), Value::Arr(Vec::new())),
        (
            "state_hash".into(),
            Value::Str(legacy_state_hash(&machine_id, "i", 3, &instance).unwrap()),
        ),
    ]));
    let root_material = Value::Obj(BTreeMap::from([
        ("seq".into(), Value::Num("3".into())),
        (
            "machines".into(),
            Value::Obj(BTreeMap::from([(machine_id, definition)])),
        ),
        (
            "instances".into(),
            Value::Obj(BTreeMap::from([("i".into(), instance_root)])),
        ),
        (
            "dedup".into(),
            Value::Obj(BTreeMap::from([(
                "legacy-create".into(),
                Value::Num("2".into()),
            )])),
        ),
    ]));
    let legacy_root = format!(
        "sha256:{}",
        fsm_core::sha256::to_hex(&domain_hash("fsm:state-root:2", &root_material))
    );
    push_record(
        &mut records,
        3,
        RecordKind::StateCheckpoint,
        Value::Obj(BTreeMap::from([(
            "state_root".into(),
            Value::Str(legacy_root),
        )])),
    );
    verify_chain(&records);

    let state = fold_with(records, &mut NopSink).unwrap();
    assert_eq!(state.last_seq, 3);
    assert_eq!(state.instances.get("i").unwrap(), &instance);
}
