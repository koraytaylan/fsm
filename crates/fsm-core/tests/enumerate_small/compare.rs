use super::*;

pub(super) fn sequences<'a>(events: &'a [&'a str]) -> Vec<Vec<&'a str>> {
    fn extend<'a>(events: &'a [&'a str], prefix: &mut Vec<&'a str>, out: &mut Vec<Vec<&'a str>>) {
        out.push(prefix.clone());
        if prefix.len() == 4 {
            return;
        }
        for event in events {
            prefix.push(*event);
            extend(events, prefix, out);
            prefix.pop();
        }
    }

    let mut out = Vec::new();
    extend(events, &mut Vec::new(), &mut out);
    out
}

#[derive(Debug, Default)]
pub(super) struct RunCounts {
    pub(super) sequences: u64,
    pub(super) steps: u64,
    pub(super) applied: u64,
    pub(super) rejected: u64,
    pub(super) ignored: u64,
    pub(super) internal_applied: u64,
    pub(super) external_applied: u64,
    pub(super) leaf_changes: u64,
    pub(super) effects: u64,
    pub(super) history_changes: u64,
    pub(super) reactions: u64,
    pub(super) discards: u64,
    pub(super) microstep_limits: u64,
    pub(super) create_rejections: u64,
}

pub(super) fn compare_run(src: &str) -> RunCounts {
    let (machine, tree) = compile_src(src);
    let event_names: Vec<String> = machine
        .spec
        .events
        .iter()
        .map(|event| event.name.clone())
        .collect();
    let event_refs: Vec<&str> = event_names.iter().map(String::as_str).collect();
    let engine_create = create(&machine, &tree, &BTreeMap::new(), 0)
        .unwrap_or_else(|err| panic!("engine create failed for generated machine: {err:?}\n{src}"));
    let oracle_create = oracle::naive_create(&machine, &BTreeMap::new())
        .unwrap_or_else(|err| panic!("oracle create failed for generated machine: {err:?}\n{src}"));
    assert_eq!(
        engine_create.configuration_after, oracle_create.configuration_after,
        "create configuration {src}"
    );
    assert_eq!(
        engine_create.ctx_after, oracle_create.ctx_after,
        "create ctx {src}"
    );
    assert_eq!(
        engine_create.history_after, oracle_create.history_after,
        "create history {src}"
    );
    assert_eq!(
        engine_create.deadlines_after, oracle_create.deadlines_after,
        "create deadlines {src}"
    );
    assert_eq!(
        engine_create.effects, oracle_create.effects,
        "create effects {src}"
    );
    assert_eq!(
        engine_create.monitor_flags, oracle_create.monitor_flags,
        "create monitor flags {src}"
    );
    assert_eq!(
        engine_create.status_after, oracle_create.status_after,
        "create status {src}"
    );
    assert_eq!(
        engine_create.entered, oracle_create.entered,
        "create entry path {src}"
    );

    if matches!(&machine.spec.topology, Topology::Sequential { .. }) {
        let engine_enterable = fsm_core::analyze::enterable(&machine, &tree);
        let oracle_enterable = oracle::brute_enterable(&machine);
        assert_eq!(engine_enterable, oracle_enterable, "enterable {src}");
    }

    let initial_engine = InstanceState {
        status: engine_create.status_after,
        configuration: engine_create.configuration_after,
        ctx: engine_create.ctx_after,
        history: engine_create.history_after,
        deadlines: engine_create.deadlines_after,
        pending: vec![],
    };
    let initial_oracle = InstanceState {
        status: oracle_create.status_after,
        configuration: oracle_create.configuration_after,
        ctx: oracle_create.ctx_after,
        history: oracle_create.history_after,
        deadlines: oracle_create.deadlines_after,
        pending: vec![],
    };
    let all_sequences = sequences(&event_refs);
    let mut counts = RunCounts {
        sequences: all_sequences.len() as u64,
        ..RunCounts::default()
    };
    for sequence in &all_sequences {
        let mut engine_state = initial_engine.clone();
        let mut oracle_state = initial_oracle.clone();
        for event in sequence {
            counts.steps += 1;
            let pre_engine = engine_state.clone();
            let pre_oracle = oracle_state.clone();
            let mut engine_budget = Budget::new(4096);
            let mut oracle_budget = Budget::new(4096);
            let engine_outcome = step(
                &machine,
                &tree,
                &engine_state,
                event,
                &payload(),
                0,
                &mut engine_budget,
            );
            let oracle_outcome = oracle::naive_step(
                &machine,
                &oracle_state,
                event,
                &payload(),
                &mut oracle_budget,
            );
            match (&engine_outcome, &oracle_outcome) {
                (Outcome::Applied(engine), Outcome::Applied(oracle)) => {
                    counts.applied += 1;
                    counts.effects += engine.effects.len() as u64;
                    if engine.internal {
                        counts.internal_applied += 1;
                    } else {
                        counts.external_applied += 1;
                    }
                    if engine.configuration_after != pre_engine.configuration {
                        counts.leaf_changes += 1;
                    }
                    if engine.history_after != pre_engine.history {
                        counts.history_changes += 1;
                    }
                    assert_eq!(
                        engine.configuration_after, oracle.configuration_after,
                        "{src} {sequence:?}"
                    );
                    assert_eq!(engine.ctx_after, oracle.ctx_after, "{src} {sequence:?}");
                    assert_eq!(
                        engine.history_after, oracle.history_after,
                        "{src} {sequence:?}"
                    );
                    assert_eq!(
                        engine.status_after, oracle.status_after,
                        "{src} {sequence:?}"
                    );
                    assert_eq!(engine.effects, oracle.effects, "{src} {sequence:?}");
                    assert_eq!(
                        engine.monitor_flags, oracle.monitor_flags,
                        "{src} {sequence:?}"
                    );
                    assert_eq!(engine.internal, oracle.internal, "{src} {sequence:?}");
                    assert_eq!(engine.region, oracle.region, "{src} {sequence:?}");
                    assert_eq!(
                        engine.source_state, oracle.source_state,
                        "{src} {sequence:?}"
                    );
                    assert_eq!(
                        engine.transition_idx, oracle.transition_idx,
                        "{src} {sequence:?}"
                    );
                    assert_eq!(engine.exited, oracle.exited, "{src} {sequence:?}");
                    assert_eq!(engine.entered, oracle.entered, "{src} {sequence:?}");
                    assert_eq!(
                        engine.deadlines_after, oracle.deadlines_after,
                        "{src} {sequence:?}"
                    );
                    engine_state.configuration = engine.configuration_after.clone();
                    engine_state.ctx = engine.ctx_after.clone();
                    engine_state.history = engine.history_after.clone();
                    engine_state.deadlines = engine.deadlines_after.clone();
                    engine_state.status = engine.status_after;
                    oracle_state.configuration = oracle.configuration_after.clone();
                    oracle_state.ctx = oracle.ctx_after.clone();
                    oracle_state.history = oracle.history_after.clone();
                    oracle_state.deadlines = oracle.deadlines_after.clone();
                    oracle_state.status = oracle.status_after;
                }
                (Outcome::Rejected(engine), Outcome::Rejected(oracle)) => {
                    counts.rejected += 1;
                    assert_eq!(engine.code, oracle.code, "{src} {sequence:?}");
                    assert_eq!(engine.cause, oracle.cause, "{src} {sequence:?}");
                    assert_eq!(engine_state, pre_engine, "engine mutated on reject {src}");
                    assert_eq!(oracle_state, pre_oracle, "oracle mutated on reject {src}");
                    assert_ne!(
                        engine.code, "internal/budget",
                        "normal budget tripped {src}"
                    );
                    assert_ne!(
                        engine.cause,
                        Some("internal/budget"),
                        "normal budget tripped inside a block {src}"
                    );
                }
                (Outcome::Ignored, Outcome::Ignored) => {
                    counts.ignored += 1;
                    assert_eq!(engine_state, pre_engine, "engine mutated on ignore {src}");
                    assert_eq!(oracle_state, pre_oracle, "oracle mutated on ignore {src}");
                }
                _ => panic!(
                    "outcome mismatch for {sequence:?}: engine={engine_outcome:?} oracle={oracle_outcome:?}\n{src}"
                ),
            }
        }
    }
    counts
}

#[derive(Debug, Default)]
pub(super) struct SuiteCounts {
    pub(super) generated: u64,
    pub(super) executed: u64,
    pub(super) runs: RunCounts,
}

pub(super) fn execute_case(src: String, counts: &mut SuiteCounts) {
    counts.generated += 1;
    let run = compare_run(&src);
    counts.executed += 1;
    counts.runs.sequences += run.sequences;
    counts.runs.steps += run.steps;
    counts.runs.applied += run.applied;
    counts.runs.rejected += run.rejected;
    counts.runs.ignored += run.ignored;
    counts.runs.internal_applied += run.internal_applied;
    counts.runs.external_applied += run.external_applied;
    counts.runs.leaf_changes += run.leaf_changes;
    counts.runs.effects += run.effects;
    counts.runs.history_changes += run.history_changes;
    counts.runs.reactions += run.reactions;
    counts.runs.discards += run.discards;
    counts.runs.microstep_limits += run.microstep_limits;
}

/// Admission parity: the engine refuses a certain eventless cycle with
/// `def/eventless_cycle`, and the oracle's own brute-force check must say so
/// of exactly the same machines. Returns whether the machine was accepted.
pub(super) fn assert_admission_parity(src: &str) -> bool {
    let value = fsm_core::json::parse(src.as_bytes(), &fsm_core::json::JsonLimits::DEFAULT)
        .unwrap_or_else(|err| panic!("generated JSON did not parse: {err:?}\n{src}"));
    let spec = parse_machine(&value)
        .unwrap_or_else(|findings| panic!("generated machine did not parse: {findings:?}\n{src}"));
    let naive_cycle = oracle::naive_certain_cycle(&spec);
    match compile(spec) {
        Ok(_) => {
            assert!(
                !naive_cycle,
                "the engine admitted a machine the naive cycle check refuses\n{src}"
            );
            true
        }
        Err(findings) => {
            let codes: Vec<&str> = findings.iter().map(|f| f.code).collect();
            assert!(
                codes.iter().all(|code| *code == "def/eventless_cycle"),
                "generated machine refused for something other than a cycle: {codes:?}\n{src}"
            );
            assert!(
                naive_cycle,
                "the engine refused a cycle the naive check does not see: {codes:?}\n{src}"
            );
            false
        }
    }
}

/// Admission parity, then a run under the macrostep budget comparing every
/// macrostep's reactions; a refused machine counts as generated only.
pub(super) fn execute_macrostep_case(src: String, counts: &mut SuiteCounts) -> bool {
    counts.generated += 1;
    if !assert_admission_parity(&src) {
        return false;
    }
    let run = compare_macrostep_run(&src);
    counts.executed += 1;
    counts.runs.sequences += run.sequences;
    counts.runs.steps += run.steps;
    counts.runs.applied += run.applied;
    counts.runs.rejected += run.rejected;
    counts.runs.ignored += run.ignored;
    counts.runs.effects += run.effects;
    counts.runs.reactions += run.reactions;
    counts.runs.discards += run.discards;
    counts.runs.microstep_limits += run.microstep_limits;
    counts.runs.create_rejections += run.create_rejections;
    true
}

/// Everything `assert_applied_parity` compares, plus the macrostep's
/// reactions: each microstep's index, trigger, source, transition, region,
/// exit set, and entry set, and every discarded internal event.
pub(super) fn assert_macrostep_parity(engine: &Applied, oracle: &Applied, case: &str) {
    assert_applied_parity(engine, oracle, case);
    let reactions = |applied: &Applied| -> Vec<(
        u32,
        String,
        String,
        u32,
        Option<String>,
        Vec<String>,
        Vec<String>,
    )> {
        applied
            .trace
            .microsteps
            .iter()
            .map(|m| {
                (
                    m.index,
                    format!("{:?}", m.trigger),
                    m.source_state.clone(),
                    m.transition_idx,
                    m.region.clone(),
                    m.exited.clone(),
                    m.entered.clone(),
                )
            })
            .collect()
    };
    assert_eq!(reactions(engine), reactions(oracle), "microsteps {case}");
    let discards = |applied: &Applied| -> Vec<(String, u32)> {
        applied
            .trace
            .internal_unhandled
            .iter()
            .map(|u| (u.event.clone(), u.after_microstep))
            .collect()
    };
    assert_eq!(discards(engine), discards(oracle), "discards {case}");
}

/// Like `compare_run`, under the macrostep budget, over the sendable events
/// only, comparing reactions and rejection codes including the ceiling's.
pub(super) fn compare_macrostep_run(src: &str) -> RunCounts {
    let (machine, tree) = compile_src(src);
    let sendable: Vec<String> = machine
        .spec
        .events
        .iter()
        .filter(|event| !event.internal)
        .map(|event| event.name.clone())
        .collect();
    let event_refs: Vec<&str> = sendable.iter().map(String::as_str).collect();
    // Creation cascades too, so it can be refused — by both, for the same
    // reason: the engine reports `run/create_failed` and keeps the cause.
    let (engine_create, oracle_create) = match (
        create(&machine, &tree, &BTreeMap::new(), 0),
        oracle::naive_create(&machine, &BTreeMap::new()),
    ) {
        (Ok(engine), Ok(oracle)) => (engine, oracle),
        (Err(engine), Err(oracle)) => {
            assert_eq!(engine.code, "run/create_failed", "{src}");
            let expected_cause = if oracle.code == "run/microstep_limit" {
                Some("run/microstep_limit")
            } else {
                oracle.cause
            };
            assert_eq!(engine.cause, expected_cause, "create cause {src}");
            if oracle.code == "run/microstep_limit" {
                assert_eq!(engine.trace.microsteps.len(), 64, "{src}");
            }
            return RunCounts {
                create_rejections: 1,
                ..RunCounts::default()
            };
        }
        (engine, oracle) => panic!("create mismatch: engine={engine:?} oracle={oracle:?}\n{src}"),
    };
    assert_macrostep_parity(&engine_create, &oracle_create, &format!("create {src}"));
    let initial_engine = state_from_applied(engine_create.clone());
    let initial_oracle = state_from_applied(oracle_create);
    let all_sequences = sequences(&event_refs);
    let mut counts = RunCounts {
        sequences: all_sequences.len() as u64,
        reactions: engine_create.trace.microsteps.len() as u64,
        discards: engine_create.trace.internal_unhandled.len() as u64,
        ..RunCounts::default()
    };
    for sequence in &all_sequences {
        let mut engine_state = initial_engine.clone();
        let mut oracle_state = initial_oracle.clone();
        for event in sequence {
            counts.steps += 1;
            let pre_engine = engine_state.clone();
            let pre_oracle = oracle_state.clone();
            let mut engine_budget = Budget::new(4096 * 66);
            let mut oracle_budget = Budget::new(4096 * 66);
            let engine_outcome = step(
                &machine,
                &tree,
                &engine_state,
                event,
                &payload(),
                0,
                &mut engine_budget,
            );
            let oracle_outcome = oracle::naive_step(
                &machine,
                &oracle_state,
                event,
                &payload(),
                &mut oracle_budget,
            );
            let case = format!("{src} {sequence:?}");
            match (&engine_outcome, &oracle_outcome) {
                (Outcome::Applied(engine), Outcome::Applied(oracle)) => {
                    counts.applied += 1;
                    counts.effects += engine.effects.len() as u64;
                    counts.reactions += engine.trace.microsteps.len() as u64;
                    counts.discards += engine.trace.internal_unhandled.len() as u64;
                    assert_macrostep_parity(engine, oracle, &case);
                    engine_state = state_from_applied(engine.clone());
                    oracle_state = state_from_applied(oracle.clone());
                }
                (Outcome::Rejected(engine), Outcome::Rejected(oracle)) => {
                    counts.rejected += 1;
                    assert_eq!(engine.code, oracle.code, "{case}");
                    assert_eq!(engine.cause, oracle.cause, "{case}");
                    assert_eq!(engine_state, pre_engine, "engine mutated on reject {case}");
                    assert_eq!(oracle_state, pre_oracle, "oracle mutated on reject {case}");
                    assert_ne!(
                        engine.code, "internal/budget",
                        "macrostep budget tripped {case}"
                    );
                    assert_ne!(
                        engine.cause,
                        Some("internal/budget"),
                        "macrostep budget tripped {case}"
                    );
                    if engine.code == "run/microstep_limit" {
                        counts.microstep_limits += 1;
                        assert_eq!(
                            engine.trace.microsteps.len(),
                            64,
                            "the ceiling is a specified number {case}"
                        );
                    }
                }
                (Outcome::Ignored, Outcome::Ignored) => {
                    counts.ignored += 1;
                    assert_eq!(engine_state, pre_engine, "engine mutated on ignore {case}");
                    assert_eq!(oracle_state, pre_oracle, "oracle mutated on ignore {case}");
                }
                _ => panic!(
                    "outcome mismatch for {sequence:?}: engine={engine_outcome:?} oracle={oracle_outcome:?}\n{src}"
                ),
            }
        }
    }
    counts
}

pub(super) fn state_from_create(
    machine: &fsm_core::machine::CompiledMachine,
    tree: &Tree,
) -> InstanceState {
    let created = create(machine, tree, &BTreeMap::new(), 0).unwrap();
    InstanceState {
        status: created.status_after,
        configuration: created.configuration_after,
        ctx: created.ctx_after,
        history: created.history_after,
        deadlines: created.deadlines_after,
        pending: vec![],
    }
}

pub(super) fn state_from_applied(applied: Applied) -> InstanceState {
    InstanceState {
        status: applied.status_after,
        configuration: applied.configuration_after,
        ctx: applied.ctx_after,
        history: applied.history_after,
        deadlines: applied.deadlines_after,
        pending: Vec::new(),
    }
}

pub(super) fn assert_applied_parity(engine: &Applied, oracle: &Applied, case: &str) {
    assert_eq!(
        engine.configuration_after, oracle.configuration_after,
        "{case}"
    );
    assert_eq!(engine.ctx_after, oracle.ctx_after, "{case}");
    assert_eq!(engine.history_after, oracle.history_after, "{case}");
    assert_eq!(engine.deadlines_after, oracle.deadlines_after, "{case}");
    assert_eq!(engine.effects, oracle.effects, "{case}");
    assert_eq!(engine.monitor_flags, oracle.monitor_flags, "{case}");
    assert_eq!(engine.status_after, oracle.status_after, "{case}");
    assert_eq!(engine.internal, oracle.internal, "{case}");
    assert_eq!(engine.region, oracle.region, "{case}");
    assert_eq!(engine.source_state, oracle.source_state, "{case}");
    assert_eq!(engine.transition_idx, oracle.transition_idx, "{case}");
    assert_eq!(engine.exited, oracle.exited, "{case}");
    assert_eq!(engine.entered, oracle.entered, "{case}");
}

pub(super) fn generated_deadline_machine(
    first_after: i64,
    second_after: i64,
    initial_n: i64,
    first_value: &str,
    second_value: &str,
) -> String {
    format!(
        r#"{{"format":"fsm.machine/1","name":"generated_deadlines","states":[{{"name":"waiting"}},{{"name":"away"}}],"initial":"waiting","context":[{{"name":"n","ty":"int","init":"{initial_n}"}}],"events":[{{"name":"leave","fields":[]}},{{"name":"return","fields":[]}}],"transitions":[{{"from":"waiting","on":"leave","to":"away"}},{{"from":"away","on":"return","to":"waiting"}}],"deadlines":[{{"name":"first","from":"waiting","after":"dur({first_after}, ms)","to":"waiting","do":[{{"target":"n","value":"{first_value}"}}]}},{{"name":"second","from":"waiting","after":"dur({second_after}, ms)","to":"waiting","do":[{{"target":"n","value":"{second_value}"}}]}}],"invariants":[{{"name":"nonnegative","expr":"ctx.n >= 0","mode":"enforce"}}]}}"#
    )
}
