//! Hash-authenticated legacy history quirks remain replayable without
//! weakening current definition admission.

use std::collections::BTreeMap;

use fsm_core::expr::eval::Budget;
use fsm_core::hashes::legacy_state_hash;
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::machine::{ActiveConfiguration, InstanceState, Status};
use fsm_core::record::{Record, RecordKind, limits_value, seal, zeros};
use fsm_core::replay::{NopSink, fold_with};
use fsm_core::spec::{accepted_identity, compile_accepted, compile_accepted_historical_unchecked};
use fsm_core::step::{Applied, Outcome, create, step};
use fsm_core::tree::Tree;

fn definition(source: &[u8]) -> Value {
    parse(source, &JsonLimits::DEFAULT).expect("test definition parses")
}

fn historical_genesis() -> Record {
    let Value::Obj(mut limits) = limits_value() else {
        unreachable!("limits are an object")
    };
    limits.remove("max_regions");
    limits.remove("max_deadlines");
    limits.remove("max_eval_ticks");
    seal(
        0,
        0,
        RecordKind::Genesis,
        Value::Obj(BTreeMap::from([
            ("format".into(), Value::Str("fsm.journal/1".into())),
            ("created_ts".into(), Value::Num("0".into())),
            ("limits".into(), Value::Obj(limits)),
        ])),
        &zeros(),
    )
}

fn state_from_applied(applied: Applied, pending: Vec<String>) -> InstanceState {
    InstanceState {
        status: applied.status_after,
        configuration: applied.configuration_after,
        ctx: applied.ctx_after,
        history: applied.history_after,
        deadlines: applied.deadlines_after,
        pending,
        invocations: BTreeMap::new(),
        signals: BTreeMap::new(),
    }
}

#[derive(Clone, Copy)]
struct ExpectedApplied<'a> {
    event: &'a str,
    leaf_after: &'a str,
    history_after: &'a [(&'a str, &'a str)],
    exited: &'a [&'a str],
    entered: &'a [&'a str],
    source_state: &'a str,
}

fn literal_state(leaf: &str, history: &[(&str, &str)]) -> InstanceState {
    InstanceState {
        status: Status::Running,
        configuration: ActiveConfiguration::Sequential { leaf: leaf.into() },
        ctx: BTreeMap::new(),
        history: history
            .iter()
            .map(|(owner, binding)| ((*owner).into(), (*binding).into()))
            .collect(),
        deadlines: BTreeMap::new(),
        pending: Vec::new(),
        invocations: BTreeMap::new(),
        signals: BTreeMap::new(),
    }
}

fn fold_literal_historical_events(
    source: &[u8],
    initial_leaf: &str,
    expected_events: &[ExpectedApplied<'_>],
) -> InstanceState {
    let definition = definition(source);
    assert!(
        compile_accepted(&definition)
            .expect_err("current admission must reject the malformed history shape")
            .iter()
            .any(|finding| finding.code == "def/shape")
    );
    let machine = compile_accepted_historical_unchecked(&definition)
        .expect("the legacy compatibility compiler accepts the old shape");
    let tree = Tree::for_machine(&machine.spec);
    let genesis = historical_genesis();
    let machine_id = accepted_identity(&definition).1;
    let defined = seal(
        1,
        1,
        RecordKind::MachineDefined,
        Value::Obj(BTreeMap::from([
            ("machine_id".into(), Value::Str(machine_id.clone())),
            ("def".into(), definition),
        ])),
        &genesis.hash,
    );

    let instance_id = "literal-legacy-history-instance";
    let mut state = literal_state(initial_leaf, &[]);
    let created = create(&machine, &tree, &BTreeMap::new(), 2)
        .expect("historical machine creates from its real initial state");
    assert_eq!(
        state_from_applied(created, Vec::new()),
        state,
        "literal initial state must not be derived from the current create implementation"
    );
    let created_record = seal(
        2,
        2,
        RecordKind::InstanceCreated,
        Value::Obj(BTreeMap::from([
            ("instance_id".into(), Value::Str(instance_id.into())),
            ("machine_id".into(), Value::Str(machine_id)),
            (
                "request_id".into(),
                Value::Str("literal-request-create".into()),
            ),
            (
                "state_hash".into(),
                Value::Str(
                    legacy_state_hash(&machine.machine_id, instance_id, 2, &state)
                        .expect("historical state is sequential"),
                ),
            ),
            ("leaf".into(), Value::Str(initial_leaf.into())),
            ("overrides".into(), Value::Obj(BTreeMap::new())),
        ])),
        &defined.hash,
    );

    let mut records = vec![genesis, defined, created_record];
    for expected in expected_events {
        let next = literal_state(expected.leaf_after, expected.history_after);
        let payload = Value::Obj(BTreeMap::new());
        let mut budget = Budget::new(fsm_core::limits::MAX_EVAL_TICKS);
        let Outcome::Applied(actual) = step(
            &machine,
            &tree,
            &state,
            expected.event,
            &payload,
            records.len() as i64,
            &mut budget,
        ) else {
            panic!("literal historical event {} must apply", expected.event)
        };
        assert_eq!(actual.configuration_after, next.configuration);
        assert_eq!(actual.ctx_after, next.ctx);
        assert_eq!(actual.history_after, next.history);
        assert_eq!(actual.deadlines_after, next.deadlines);
        assert_eq!(actual.status_after, next.status);
        assert!(actual.effects.is_empty());
        assert_eq!(actual.exited, expected.exited);
        assert_eq!(actual.entered, expected.entered);
        assert_eq!(actual.source_state, expected.source_state);

        // The record and committed state below come only from the literal
        // legacy expectations above, never from `actual`.
        let seq = records.len() as u64;
        let record = seal(
            seq,
            seq as i64,
            RecordKind::EventApplied,
            Value::Obj(BTreeMap::from([
                ("instance_id".into(), Value::Str(instance_id.into())),
                (
                    "request_id".into(),
                    Value::Str(format!("literal-request-{seq}")),
                ),
                ("event".into(), Value::Str(expected.event.into())),
                ("payload".into(), payload),
                (
                    "state_hash".into(),
                    Value::Str(
                        legacy_state_hash(&machine.machine_id, instance_id, seq, &next)
                            .expect("historical state is sequential"),
                    ),
                ),
                (
                    "exited".into(),
                    Value::Arr(
                        expected
                            .exited
                            .iter()
                            .map(|state| Value::Str((*state).into()))
                            .collect(),
                    ),
                ),
                (
                    "entered".into(),
                    Value::Arr(
                        expected
                            .entered
                            .iter()
                            .map(|state| Value::Str((*state).into()))
                            .collect(),
                    ),
                ),
                (
                    "source_state".into(),
                    Value::Str(expected.source_state.into()),
                ),
            ])),
            &records.last().expect("genesis exists").hash,
        );
        state = next;
        records.push(record);
    }

    let replayed = fold_with(records, &mut NopSink)
        .expect("literal hash-valid legacy records must still full-fold");
    assert_eq!(replayed.instances.get(instance_id), Some(&state));
    state
}

fn append_applied_event(
    machine: &fsm_core::machine::CompiledMachine,
    tree: &Tree,
    state: &InstanceState,
    instance_id: &str,
    event: &str,
    seq: u64,
    previous_hash: &str,
) -> (InstanceState, Record) {
    let payload = Value::Obj(BTreeMap::new());
    let mut budget = Budget::new(fsm_core::limits::MAX_EVAL_TICKS);
    let Outcome::Applied(applied) = step(
        machine,
        tree,
        state,
        event,
        &payload,
        seq as i64,
        &mut budget,
    ) else {
        panic!("historical event {event} must apply")
    };
    let mut pending = state.pending.clone();
    pending.extend(
        applied
            .effects
            .iter()
            .map(|effect| format!("{instance_id}/{seq}/{}", effect.k)),
    );
    let exited = applied.exited.iter().cloned().map(Value::Str).collect();
    let entered = applied.entered.iter().cloned().map(Value::Str).collect();
    let source_state = applied.source_state.clone();
    let next = state_from_applied(applied, pending);
    let state_hash = legacy_state_hash(&machine.machine_id, instance_id, seq, &next)
        .expect("historical definitions have sequential state");
    let record = seal(
        seq,
        seq as i64,
        RecordKind::EventApplied,
        Value::Obj(BTreeMap::from([
            ("instance_id".into(), Value::Str(instance_id.into())),
            ("request_id".into(), Value::Str(format!("request-{seq}"))),
            ("event".into(), Value::Str(event.into())),
            ("payload".into(), payload),
            ("state_hash".into(), Value::Str(state_hash)),
            ("exited".into(), Value::Arr(exited)),
            ("entered".into(), Value::Arr(entered)),
            ("source_state".into(), Value::Str(source_state)),
        ])),
        previous_hash,
    );
    (next, record)
}

fn fold_historical_events(source: &[u8], events: &[&str]) -> InstanceState {
    let definition = definition(source);
    assert!(
        compile_accepted(&definition)
            .expect_err("current admission must reject the malformed history shape")
            .iter()
            .any(|finding| finding.code == "def/shape")
    );
    let machine = compile_accepted_historical_unchecked(&definition)
        .expect("the legacy compatibility compiler accepts the old shape");
    let tree = Tree::for_machine(&machine.spec);
    let genesis = historical_genesis();
    let machine_id = accepted_identity(&definition).1;
    let defined = seal(
        1,
        1,
        RecordKind::MachineDefined,
        Value::Obj(BTreeMap::from([
            ("machine_id".into(), Value::Str(machine_id.clone())),
            ("def".into(), definition),
        ])),
        &genesis.hash,
    );

    let created = create(&machine, &tree, &BTreeMap::new(), 2)
        .expect("historical machine creates from its real initial state");
    let instance_id = "legacy-history-instance";
    let mut state = state_from_applied(created, Vec::new());
    let created_record = seal(
        2,
        2,
        RecordKind::InstanceCreated,
        Value::Obj(BTreeMap::from([
            ("instance_id".into(), Value::Str(instance_id.into())),
            ("machine_id".into(), Value::Str(machine_id)),
            ("request_id".into(), Value::Str("request-create".into())),
            (
                "state_hash".into(),
                Value::Str(
                    legacy_state_hash(&machine.machine_id, instance_id, 2, &state)
                        .expect("historical state is sequential"),
                ),
            ),
            (
                "leaf".into(),
                Value::Str(
                    state
                        .configuration
                        .sequential_leaf()
                        .expect("historical state is sequential")
                        .into(),
                ),
            ),
            ("overrides".into(), Value::Obj(BTreeMap::new())),
        ])),
        &defined.hash,
    );

    let mut records = vec![genesis, defined, created_record];
    for event in events {
        let seq = records.len() as u64;
        let previous_hash = records.last().expect("genesis exists").hash.clone();
        let (next, record) = append_applied_event(
            &machine,
            &tree,
            &state,
            instance_id,
            event,
            seq,
            &previous_hash,
        );
        state = next;
        records.push(record);
    }

    let replayed = fold_with(records, &mut NopSink)
        .expect("hash-valid legacy history records must still full-fold");
    assert_eq!(replayed.instances.get(instance_id), Some(&state));
    state
}

#[test]
fn historical_compiler_preserves_each_legacy_history_shape_only() {
    let historical_shapes: &[&[u8]] = &[
        br#"{"format":"fsm.machine/1","name":"top_level_history","states":[{"name":"start"},{"name":"h","history":"deep"}],"initial":"start","context":[],"events":[{"name":"go","fields":[]}],"transitions":[{"from":"start","on":"go","to":"h"}]}"#,
        br#"{"format":"fsm.machine/1","name":"child_bearing_history","states":[{"name":"box","initial":"start","states":[{"name":"start"},{"name":"h","history":"deep","states":[{"name":"buried"}]}]}],"initial":"box","context":[],"events":[],"transitions":[]}"#,
        br#"{"format":"fsm.machine/1","name":"terminal_history","states":[{"name":"box","initial":"start","states":[{"name":"start"},{"name":"h","history":"deep","terminal":true}]}],"initial":"box","context":[],"events":[],"transitions":[]}"#,
        br#"{"format":"fsm.machine/1","name":"initial_history","states":[{"name":"box","initial":"start","states":[{"name":"start"},{"name":"h","history":"deep","initial":"missing"}]}],"initial":"box","context":[],"events":[],"transitions":[]}"#,
    ];

    for source in historical_shapes {
        let value = definition(source);
        assert!(
            compile_accepted(&value)
                .expect_err("current admission rejects old malformed history")
                .iter()
                .any(|finding| finding.code == "def/shape")
        );
        compile_accepted_historical_unchecked(&value)
            .expect("historical persistence preserves legacy admission");
    }

    let malformed_parallel_history = definition(
        br#"{"format":"fsm.machine/1","name":"not_historical_parallel","regions":[{"name":"one","initial":"a","states":[{"name":"a"},{"name":"h","history":"deep","terminal":true}]},{"name":"two","initial":"b","states":[{"name":"b"}]}],"context":[],"events":[],"transitions":[]}"#,
    );
    assert!(
        compile_accepted_historical_unchecked(&malformed_parallel_history)
            .expect_err("parallel history shapes were never admitted under the legacy engine")
            .iter()
            .any(|finding| finding.code == "def/shape")
    );

    let malformed_timed_history = definition(
        br#"{"format":"fsm.machine/1","name":"not_historical_timed","states":[{"name":"a"},{"name":"b"},{"name":"h","history":"deep","terminal":true}],"initial":"a","context":[],"events":[],"transitions":[],"deadlines":[{"name":"wait","from":"a","after":"dur(1, ms)","to":"b"}]}"#,
    );
    assert!(
        compile_accepted_historical_unchecked(&malformed_timed_history)
            .expect_err("timed history shapes were never admitted under the legacy engine")
            .iter()
            .any(|finding| finding.code == "def/shape")
    );

    let valid_current_parallel_deadline = definition(
        br#"{"format":"fsm.machine/1","name":"current_parallel_deadline","regions":[{"name":"one","initial":"a","states":[{"name":"a"},{"name":"done_a"}]},{"name":"two","initial":"b","states":[{"name":"b"},{"name":"done_b"}]}],"context":[],"events":[],"transitions":[],"deadlines":[{"name":"wait","from":"a","after":"dur(1, ms)","to":"done_a"}]}"#,
    );
    compile_accepted(&valid_current_parallel_deadline)
        .expect("fixture satisfies current structural admission");
    compile_accepted_historical_unchecked(&valid_current_parallel_deadline)
        .expect("historical whole-journal mode also accepts current-valid definitions");
}

#[test]
fn deep_and_shallow_bindings_emitted_through_history_children_still_full_fold() {
    for (kind, expected_binding) in [("deep", "buried"), ("shallow", "h")] {
        let source = format!(
            r#"{{"format":"fsm.machine/1","name":"legacy_{kind}","states":[{{"name":"box","initial":"start","states":[{{"name":"start"}},{{"name":"h","history":"{kind}","initial":"buried","states":[{{"name":"buried"}}]}}]}},{{"name":"outside"}}],"initial":"box","context":[],"events":[{{"name":"dive","fields":[]}},{{"name":"leave","fields":[]}},{{"name":"restore","fields":[]}}],"transitions":[{{"from":"start","on":"dive","to":"buried"}},{{"from":"buried","on":"leave","to":"outside"}},{{"from":"outside","on":"restore","to":"h"}}]}}"#
        );
        let state = fold_literal_historical_events(
            source.as_bytes(),
            "start",
            &[
                ExpectedApplied {
                    event: "dive",
                    leaf_after: "buried",
                    history_after: &[],
                    exited: &["start"],
                    entered: &["h", "buried"],
                    source_state: "start",
                },
                ExpectedApplied {
                    event: "leave",
                    leaf_after: "outside",
                    history_after: &[("box", expected_binding)],
                    exited: &["buried", "h", "box"],
                    entered: &["outside"],
                    source_state: "buried",
                },
                ExpectedApplied {
                    event: "restore",
                    leaf_after: "buried",
                    history_after: &[("box", expected_binding)],
                    exited: &["outside"],
                    entered: &["box", "h", "buried"],
                    source_state: "outside",
                },
            ],
        );
        assert_eq!(
            state.history.get("box").map(String::as_str),
            Some(expected_binding)
        );
    }
}

#[test]
fn non_child_history_initial_reproduces_the_legacy_global_name_jump() {
    let state = fold_literal_historical_events(
        br#"{"format":"fsm.machine/1","name":"legacy_global_history_initial","states":[{"name":"box","initial":"start","states":[{"name":"start"},{"name":"h","history":"shallow","initial":"outside","states":[{"name":"buried"}]}]},{"name":"outside"}],"initial":"box","context":[],"events":[{"name":"dive","fields":[]},{"name":"leave","fields":[]},{"name":"restore","fields":[]}],"transitions":[{"from":"start","on":"dive","to":"buried"},{"from":"buried","on":"leave","to":"outside"},{"from":"outside","on":"restore","to":"h"}]}"#,
        "start",
        &[
            ExpectedApplied {
                event: "dive",
                leaf_after: "buried",
                history_after: &[],
                exited: &["start"],
                entered: &["h", "buried"],
                source_state: "start",
            },
            ExpectedApplied {
                event: "leave",
                leaf_after: "outside",
                history_after: &[("box", "h")],
                exited: &["buried", "h", "box"],
                entered: &["outside"],
                source_state: "buried",
            },
            ExpectedApplied {
                event: "restore",
                leaf_after: "outside",
                history_after: &[("box", "h")],
                exited: &["outside"],
                entered: &["box", "h", "outside"],
                source_state: "outside",
            },
        ],
    );
    assert_eq!(state.configuration.sequential_leaf(), Some("outside"));
    assert_eq!(state.history.get("box").map(String::as_str), Some("h"));
}

#[test]
fn global_history_initial_can_restore_a_childless_history_pseudostate() {
    let state = fold_literal_historical_events(
        br#"{"format":"fsm.machine/1","name":"legacy_global_history_to_history","states":[{"name":"box","initial":"start","states":[{"name":"start"},{"name":"h","history":"shallow","initial":"orphan_history","states":[{"name":"buried"}]}]},{"name":"outside"},{"name":"orphan_history","history":"deep"}],"initial":"box","context":[],"events":[{"name":"dive","fields":[]},{"name":"leave","fields":[]},{"name":"restore","fields":[]}],"transitions":[{"from":"start","on":"dive","to":"buried"},{"from":"buried","on":"leave","to":"outside"},{"from":"outside","on":"restore","to":"h"}]}"#,
        "start",
        &[
            ExpectedApplied {
                event: "dive",
                leaf_after: "buried",
                history_after: &[],
                exited: &["start"],
                entered: &["h", "buried"],
                source_state: "start",
            },
            ExpectedApplied {
                event: "leave",
                leaf_after: "outside",
                history_after: &[("box", "h")],
                exited: &["buried", "h", "box"],
                entered: &["outside"],
                source_state: "buried",
            },
            ExpectedApplied {
                event: "restore",
                leaf_after: "orphan_history",
                history_after: &[("box", "h")],
                exited: &["outside"],
                entered: &["box", "h", "orphan_history"],
                source_state: "outside",
            },
        ],
    );
    assert_eq!(
        state.configuration.sequential_leaf(),
        Some("orphan_history")
    );
}

#[test]
fn unreachable_childless_history_chain_is_not_a_compatibility_leaf() {
    let value = definition(
        br#"{"format":"fsm.machine/1","name":"unreachable_history_chain","states":[{"name":"start"},{"name":"h","history":"deep","initial":"orphan_history"},{"name":"orphan_history","history":"shallow"}],"initial":"start","context":[],"events":[{"name":"tick","fields":[]}],"transitions":[]}"#,
    );
    let machine = compile_accepted_historical_unchecked(&value).unwrap();
    let tree = Tree::for_machine(&machine.spec);
    let forged = literal_state("orphan_history", &[]);
    tree.validate_instance_state(&machine, &forged)
        .expect_err("a childless history chain has no structurally enterable seed");

    let mut budget = Budget::new(fsm_core::limits::MAX_EVAL_TICKS);
    let Outcome::Rejected(rejection) = step(
        &machine,
        &tree,
        &forged,
        "tick",
        &Value::Obj(BTreeMap::new()),
        0,
        &mut budget,
    ) else {
        panic!("unreachable compatibility state must reject")
    };
    assert_eq!(rejection.code, "run/configuration_invalid");
}

#[test]
fn standalone_ownerless_child_bearing_history_is_not_a_compatibility_leaf() {
    let value = definition(
        br#"{"format":"fsm.machine/1","name":"unreachable_ownerless_history","states":[{"name":"start"},{"name":"h","history":"shallow","states":[{"name":"buried"}]}],"initial":"start","context":[],"events":[],"transitions":[]}"#,
    );
    let machine = compile_accepted_historical_unchecked(&value).unwrap();
    let tree = Tree::for_machine(&machine.spec);
    let forged = literal_state("h", &[]);
    tree.validate_instance_state(&machine, &forged)
        .expect_err("an ownerless child-bearing history node has no restoration seed");
}

#[test]
fn nested_history_owner_outcomes_still_full_fold() {
    let state = fold_historical_events(
        br#"{"format":"fsm.machine/1","name":"legacy_nested_history","states":[{"name":"box","initial":"normal","states":[{"name":"normal"},{"name":"outer","history":"deep","states":[{"name":"inner","history":"deep"}]}]},{"name":"outside"}],"initial":"outside","context":[],"events":[{"name":"enter","fields":[]},{"name":"leave","fields":[]},{"name":"continue","fields":[]}],"transitions":[{"from":"outside","on":"enter","to":"inner"},{"from":"box","on":"leave","to":"outside"},{"from":"outside","on":"continue"}]}"#,
        &["enter", "leave", "continue"],
    );
    assert_eq!(state.configuration.sequential_leaf(), Some("outside"));
    assert_eq!(state.history.get("box").map(String::as_str), Some("outer"));
}

#[test]
fn ownerless_historical_history_target_rejects_without_panicking() {
    let value = definition(
        br#"{"format":"fsm.machine/1","name":"legacy_ownerless_target","states":[{"name":"start"},{"name":"h","history":"deep"}],"initial":"start","context":[],"events":[{"name":"go","fields":[]}],"transitions":[{"from":"start","on":"go","to":"h"}]}"#,
    );
    let machine = compile_accepted_historical_unchecked(&value).unwrap();
    let tree = Tree::for_machine(&machine.spec);
    let state = state_from_applied(
        create(&machine, &tree, &BTreeMap::new(), 0).unwrap(),
        Vec::new(),
    );
    let outcome = std::panic::catch_unwind(|| {
        let mut budget = Budget::new(fsm_core::limits::MAX_EVAL_TICKS);
        step(
            &machine,
            &tree,
            &state,
            "go",
            &Value::Obj(BTreeMap::new()),
            1,
            &mut budget,
        )
    })
    .expect("historical ownerless target must not panic");
    let Outcome::Rejected(rejection) = outcome else {
        panic!("ownerless historical target must reject")
    };
    assert_eq!(rejection.code, "run/action_error");
    assert_eq!(rejection.cause, Some("def/shape"));
}

#[test]
fn cyclic_malformed_history_initial_terminates_with_a_valid_state() {
    let value = definition(
        br#"{"format":"fsm.machine/1","name":"legacy_cyclic_history_initial","states":[{"name":"box","initial":"start","states":[{"name":"start"},{"name":"h","history":"shallow","initial":"inner_history","states":[{"name":"buried"},{"name":"inner_history","history":"deep","initial":"h","states":[{"name":"inner_leaf"}]}]}]},{"name":"outside"}],"initial":"box","context":[],"events":[{"name":"dive","fields":[]},{"name":"leave","fields":[]},{"name":"restore","fields":[]}],"transitions":[{"from":"start","on":"dive","to":"buried"},{"from":"buried","on":"leave","to":"outside"},{"from":"outside","on":"restore","to":"h"}]}"#,
    );
    let machine = compile_accepted_historical_unchecked(&value).unwrap();
    let tree = Tree::for_machine(&machine.spec);
    let mut state = state_from_applied(
        create(&machine, &tree, &BTreeMap::new(), 0).unwrap(),
        Vec::new(),
    );
    for (timestamp, event) in [(1, "dive"), (2, "leave")] {
        let mut budget = Budget::new(fsm_core::limits::MAX_EVAL_TICKS);
        let Outcome::Applied(applied) = step(
            &machine,
            &tree,
            &state,
            event,
            &Value::Obj(BTreeMap::new()),
            timestamp,
            &mut budget,
        ) else {
            panic!("setup event {event} applies")
        };
        state = state_from_applied(applied, Vec::new());
    }

    let outcome = std::panic::catch_unwind(|| {
        let mut budget = Budget::new(fsm_core::limits::MAX_EVAL_TICKS);
        step(
            &machine,
            &tree,
            &state,
            "restore",
            &Value::Obj(BTreeMap::new()),
            3,
            &mut budget,
        )
    })
    .expect("cyclic historical initial must not panic");
    let Outcome::Applied(applied) = outcome else {
        panic!("bounded historical fallback applies")
    };
    let restored = state_from_applied(applied, Vec::new());
    assert_eq!(restored.configuration.sequential_leaf(), Some("h"));
    tree.validate_instance_state(&machine, &restored)
        .expect("cycle fallback must not emit an invalid state");
}
