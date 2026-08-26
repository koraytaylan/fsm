//! The macrostep loop, the dumbest possible way: a `Vec` used as a queue
//! with `remove(0)`, a linear scan over every transition for eventless
//! candidates, no memoisation, and no helper shared with the engine. The
//! single-transition application is this oracle's own naive step, refactored
//! into the pieces the loop applies once per microstep.
//!
//! Plan 0009 task 4704.

use fsm_core::expr::eval::Bindings;
use fsm_core::expr::parser;
use fsm_core::machine::{CancelledChild, Invocation, InvokeStatus};
use fsm_core::trace::{DecisionTrace, MicrostepTrace, MicrostepTrigger, UnhandledInternalTrace};

use super::deadline::{clear_terminal_region_deadlines, update_deadline_schedules};
use super::eval::{Raised, eval_bool, eval_invariants, reject};
use super::step::{NaiveMicro, SelectedCandidate, apply_candidate, select_event_candidate};
use super::*;

/// The invocation outbox, the dumbest possible way: walk the spec for each
/// exited and entered state, drop the slots of the first and insert the slots
/// of the second, evaluating each `with` expression from source against the
/// context the pipeline produced.
fn naive_invocations(
    spec: &MachineSpec,
    prior: &BTreeMap<String, Invocation>,
    exited: &[String],
    entered: &[String],
    ctx: &BTreeMap<String, Val>,
    budget: &mut Budget,
) -> Result<(BTreeMap<String, Invocation>, Vec<CancelledChild>), Rejection> {
    let mut invocations = prior.clone();
    let mut cancelled = Vec::new();
    let slots_of = |name: &str| -> Vec<fsm_core::spec::InvokeSpec> {
        state_slices(spec)
            .into_iter()
            .find_map(|states| find(states, name).map(|node| node.invokes.clone()))
            .unwrap_or_default()
    };
    for name in exited {
        for invoke in slots_of(name) {
            if let Some(gone) = invocations.remove(&invoke.id) {
                if gone.status == InvokeStatus::Running {
                    cancelled.push(CancelledChild {
                        slot: invoke.id.clone(),
                        child_instance_id: String::new(),
                    });
                }
            }
        }
    }
    let bindings = Bindings {
        ctx,
        evt: None,
        active: None,
    };
    for name in entered {
        for invoke in slots_of(name) {
            let mut overrides = BTreeMap::new();
            for (field, source) in &invoke.with {
                let expression = parser::parse(source).map_err(|err| Rejection {
                    code: "run/action_error",
                    message: err.message,
                    hint: err.hint,
                    source_state: None,
                    transition_idx: None,
                    block: None,
                    span: Some((err.span.start, err.span.end)),
                    trace: Default::default(),
                    cause: Some(err.code),
                })?;
                let value = eval(&expression, &bindings, budget, false)
                    .0
                    .map_err(|err| Rejection {
                        code: "run/action_error",
                        message: err.message,
                        hint: err.hint,
                        source_state: None,
                        transition_idx: None,
                        block: None,
                        span: Some((err.span.start, err.span.end)),
                        trace: Default::default(),
                        cause: Some(err.code),
                    })?;
                overrides.insert(field.clone(), value);
            }
            invocations.insert(
                invoke.id.clone(),
                Invocation {
                    child_machine_id: invoke.machine.clone(),
                    status: InvokeStatus::Pending,
                    overrides,
                },
            );
        }
    }
    Ok((invocations, cancelled))
}

/// SPEC's ceiling, written as the number it is: the oracle must trip at the
/// same boundary as the engine without reading the engine's constant.
const CEILING: u32 = 64;

/// Every state slice of the definition: one for a sequential machine, one
/// per region otherwise.
fn state_slices(spec: &MachineSpec) -> Vec<&[StateNode]> {
    match &spec.topology {
        Topology::Sequential { states, .. } => vec![states.as_slice()],
        Topology::Parallel { regions } => regions
            .iter()
            .map(|region| region.states.as_slice())
            .collect(),
    }
}

/// A generated event nobody handles is never raised.
fn handled(spec: &MachineSpec, name: &str) -> bool {
    spec.transitions
        .iter()
        .any(|transition| transition.on.as_deref() == Some(name))
}

/// `$done.state.<parent>` for every entered `final` leaf, in entry order.
fn done_state_events(spec: &MachineSpec, entered: &[String]) -> Raised {
    let mut out = Vec::new();
    for name in entered {
        for states in state_slices(spec) {
            let Some(node) = find(states, name) else {
                continue;
            };
            if node.final_state && node.history.is_none() {
                if let Some(parent) = parent_of(states, name) {
                    let event = format!("$done.state.{parent}");
                    if handled(spec, &event) {
                        out.push((event, BTreeMap::new()));
                    }
                }
            }
            break;
        }
    }
    out
}

/// `$done.region.<region>` for every region whose leaf became terminal, in
/// region document order.
fn done_region_events(
    spec: &MachineSpec,
    before: &ActiveConfiguration,
    after: &ActiveConfiguration,
) -> Raised {
    let (Topology::Parallel { regions }, ActiveConfiguration::Parallel { leaves: after }) =
        (&spec.topology, after)
    else {
        return Vec::new();
    };
    let ActiveConfiguration::Parallel { leaves: before } = before else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for region in regions {
        let terminal = |leaves: &BTreeMap<String, String>| {
            leaves
                .get(&region.name)
                .and_then(|leaf| find(&region.states, leaf))
                .is_some_and(|node| node.terminal)
        };
        if terminal(after) && !terminal(before) {
            let event = format!("$done.region.{}", region.name);
            if handled(spec, &event) {
                out.push((event, BTreeMap::new()));
            }
        }
    }
    out
}

/// The eventless scan: the same walk as an event's, over transitions with
/// no `on`, reading `ctx` alone. Every guard false is quiescence for this
/// scan; a guard that fails to evaluate rejects the macrostep.
fn select_eventless(
    spec: &MachineSpec,
    active: &[ActiveLeaf<'_>],
    ctx: &BTreeMap<String, Val>,
    budget: &mut Budget,
) -> Result<Option<SelectedCandidate>, Rejection> {
    let no_event = BTreeMap::new();
    for active_leaf in active {
        if find(active_leaf.states, &active_leaf.leaf).is_some_and(|node| node.terminal) {
            continue;
        }
        for source in chain(active_leaf.states, &active_leaf.leaf) {
            for (transition_index, transition) in spec.transitions.iter().enumerate() {
                if transition.from != source || transition.on.is_some() {
                    continue;
                }
                if eval_bool(transition.guard.as_deref(), ctx, &no_event, None, budget)? {
                    return Ok(Some(SelectedCandidate {
                        region: active_leaf.region.map(str::to_string),
                        leaf: active_leaf.leaf.clone(),
                        source,
                        transition_index,
                    }));
                }
            }
        }
    }
    Ok(None)
}

fn settle(
    spec: &MachineSpec,
    prior: &BTreeMap<String, i64>,
    micro: &NaiveMicro,
    ctx: &BTreeMap<String, Val>,
    now_ms: i64,
    budget: &mut Budget,
) -> Result<(BTreeMap<String, i64>, Status), Rejection> {
    let mut deadlines = update_deadline_schedules(
        spec,
        prior,
        &micro.exited,
        &micro.entered,
        ctx,
        now_ms,
        budget,
    )?;
    clear_terminal_region_deadlines(spec, &micro.configuration_after, &mut deadlines);
    let status = if configuration_is_terminal(spec, &micro.configuration_after) {
        deadlines.clear();
        Status::Completed
    } else {
        Status::Running
    };
    Ok((deadlines, status))
}

fn limit_rejection(last: &NaiveMicro) -> Rejection {
    Rejection {
        code: "run/microstep_limit",
        message: format!("more than {CEILING} reactions in one macrostep"),
        hint: "make a guard on the cycle become false".into(),
        source_state: Some(last.source.clone()),
        transition_idx: Some(last.transition_index as u32),
        block: None,
        span: None,
        trace: Default::default(),
        cause: None,
    }
}

/// Run the reactions after `first` — the trigger's microstep, already
/// applied to `st` — to quiescence, then evaluate the invariants once, then
/// settle the last microstep's schedules. Any failure rejects the whole
/// macrostep and `st` is what the caller keeps.
pub(super) fn run_reactions(
    m: &CompiledMachine,
    st: &InstanceState,
    first: NaiveMicro,
    mut effects: Vec<EffectOut>,
    now_ms: i64,
    budget: &mut Budget,
) -> Result<Applied, Rejection> {
    let spec = &m.spec;
    let trigger_internal = first.internal;
    let trigger_region = first.region.clone();
    let trigger_source = first.source.clone();
    let trigger_index = first.transition_index as u32;
    let trigger_exited = first.exited.clone();
    let trigger_entered = first.entered.clone();

    let mut queue: Raised = Vec::new();
    queue.extend(first.raised.clone());
    queue.extend(done_state_events(spec, &first.entered));
    queue.extend(done_region_events(
        spec,
        &st.configuration,
        &first.configuration_after,
    ));

    let mut configuration = first.configuration_after.clone();
    let mut ctx = first.ctx.clone();
    let mut history = first.history_after.clone();
    let mut deadlines = st.deadlines.clone();
    let mut status = st.status;
    // Signals are collected in pipeline order and numbered continuously, the
    // same way the effects are.
    let mut signalled: Vec<(u32, fsm_core::machine::PendingSignal)> = Vec::new();
    let collect = |emitted: &super::eval::Signalled,
                   out: &mut Vec<(u32, fsm_core::machine::PendingSignal)>| {
        for (target, event, payload) in emitted {
            let k = out.len() as u32;
            out.push((
                k,
                fsm_core::machine::PendingSignal {
                    target_instance_id: target.clone(),
                    event: event.clone(),
                    payload: payload.clone(),
                },
            ));
        }
    };
    collect(&first.signalled, &mut signalled);
    let (mut invocations, mut cancelled_children) = naive_invocations(
        spec,
        &st.invocations,
        &first.exited,
        &first.entered,
        &first.ctx,
        budget,
    )?;
    let mut unsettled = first;
    let mut microsteps: Vec<MicrostepTrace> = Vec::new();
    let mut unhandled: Vec<UnhandledInternalTrace> = Vec::new();
    let mut counted = 0u32;

    loop {
        let Some(active) = active_leaves(spec, &configuration) else {
            return Err(reject(
                "run/configuration_invalid",
                "the working configuration no longer matches the topology",
            ));
        };
        let (winner, fields, trigger, sees_event) =
            match select_eventless(spec, &active, &ctx, budget)? {
                Some(winner) => {
                    counted += 1;
                    if counted > CEILING {
                        return Err(limit_rejection(&unsettled));
                    }
                    (winner, BTreeMap::new(), MicrostepTrigger::Eventless, false)
                }
                None => {
                    if queue.is_empty() {
                        break;
                    }
                    let (name, payload) = queue.remove(0);
                    counted += 1;
                    if counted > CEILING {
                        return Err(limit_rejection(&unsettled));
                    }
                    let view = InstanceState {
                        status,
                        configuration: configuration.clone(),
                        ctx: ctx.clone(),
                        history: history.clone(),
                        deadlines: deadlines.clone(),
                        pending: Vec::new(),
                        invocations: BTreeMap::new(),
                        signals: BTreeMap::new(),
                    };
                    let (selection, _) =
                        select_event_candidate(spec, &active, &view, &name, &payload, budget)?;
                    match selection {
                        Some(winner) => (winner, payload, MicrostepTrigger::Internal(name), true),
                        None => {
                            unhandled.push(UnhandledInternalTrace {
                                event: name,
                                after_microstep: microsteps.len() as u32,
                            });
                            continue;
                        }
                    }
                }
            };
        // Another reaction follows, so the previous microstep's schedules
        // settle now; the last microstep's settle after the invariants.
        let (settled, new_status) = settle(spec, &deadlines, &unsettled, &ctx, now_ms, budget)?;
        deadlines = settled;
        status = new_status;
        let view = InstanceState {
            status,
            configuration: configuration.clone(),
            ctx: ctx.clone(),
            history: history.clone(),
            deadlines: deadlines.clone(),
            pending: Vec::new(),
            invocations: invocations.clone(),
            signals: BTreeMap::new(),
        };
        let micro = apply_candidate(m, &view, &winner, &fields, sees_event, budget, &mut effects)?;
        let before = configuration.clone();
        configuration = micro.configuration_after.clone();
        ctx = micro.ctx.clone();
        history = micro.history_after.clone();
        let (settled, mut cancelled) = naive_invocations(
            spec,
            &invocations,
            &micro.exited,
            &micro.entered,
            &ctx,
            budget,
        )?;
        invocations = settled;
        cancelled_children.append(&mut cancelled);
        queue.extend(micro.raised.clone());
        collect(&micro.signalled, &mut signalled);
        queue.extend(done_state_events(spec, &micro.entered));
        queue.extend(done_region_events(spec, &before, &configuration));
        microsteps.push(MicrostepTrace {
            index: microsteps.len() as u32 + 1,
            trigger,
            source_state: micro.source.clone(),
            transition_idx: micro.transition_index as u32,
            region: micro.region.clone(),
            exited: micro.exited.clone(),
            entered: micro.entered.clone(),
            candidates: Vec::new(),
            pipeline: Vec::new(),
        });
        unsettled = micro;
    }

    let active_names = active_state_names(spec, &configuration);
    let monitor_flags = eval_invariants(spec, &ctx, &active_names, budget)?;
    let (deadlines_after, status_after) =
        settle(spec, &deadlines, &unsettled, &ctx, now_ms, budget)?;
    Ok(Applied {
        configuration_after: configuration,
        ctx_after: ctx,
        history_after: history,
        deadlines_after,
        invocations_after: invocations,
        cancelled_children,
        signals: signalled,
        effects,
        monitor_flags,
        status_after,
        internal: trigger_internal,
        region: trigger_region,
        source_state: trigger_source,
        transition_idx: trigger_index,
        exited: trigger_exited,
        entered: trigger_entered,
        trace: DecisionTrace {
            microsteps,
            internal_unhandled: unhandled,
            ..Default::default()
        },
    })
}

/// Guardless, or a guard whose parsed form is the literal `true`: the scan
/// selects such a transition whenever it reaches it.
fn certain_guard(guard: &Option<String>) -> bool {
    match guard {
        None => true,
        Some(source) => {
            parser::parse(source).is_ok_and(|expr| fsm_core::expr::ast::render_ast(&expr) == "true")
        }
    }
}

/// The leaf a transition to `target` lands in, with no history binding.
fn landing_leaf(states: &[StateNode], target: &str) -> String {
    let Some(node) = find(states, target) else {
        return target.to_string();
    };
    if node.history.is_some() {
        let owner = parent_of(states, target).unwrap_or_else(|| target.to_string());
        return initial_descent(states, &owner)
            .last()
            .cloned()
            .unwrap_or(owner);
    }
    initial_descent(states, target)
        .last()
        .cloned()
        .unwrap_or_else(|| target.to_string())
}

/// Whether the definition has an eventless cycle a macrostep provably cannot
/// leave, decided by brute force: some nonempty set of leaves in which every
/// leaf's scan reaches a certain candidate, and every candidate it could
/// select on the way lands back in the set. `def/eventless_cycle` names the
/// same machines, by a different route.
pub fn naive_certain_cycle(spec: &MachineSpec) -> bool {
    for states in state_slices(spec) {
        let mut leaves = Vec::new();
        collect_leaves(states, &mut leaves);
        let edges: Vec<(Vec<String>, bool)> = leaves
            .iter()
            .map(|leaf| {
                let mut landings = Vec::new();
                let mut certain = false;
                'scan: for source in chain(states, leaf) {
                    for transition in &spec.transitions {
                        if transition.from != source || transition.on.is_some() {
                            continue;
                        }
                        landings.push(match &transition.to {
                            None => leaf.clone(),
                            Some(target) => landing_leaf(states, target),
                        });
                        if certain_guard(&transition.guard) {
                            certain = true;
                            break 'scan;
                        }
                    }
                }
                (landings, certain)
            })
            .collect();
        for mask in 1u32..(1u32 << leaves.len()) {
            let inside = |name: &str| {
                leaves
                    .iter()
                    .position(|leaf| leaf == name)
                    .is_some_and(|i| mask & (1 << i) != 0)
            };
            let closed = leaves.iter().enumerate().all(|(i, _)| {
                mask & (1 << i) == 0
                    || (edges[i].1 && edges[i].0.iter().all(|landing| inside(landing)))
            });
            if closed {
                return true;
            }
        }
    }
    false
}

fn collect_leaves(states: &[StateNode], out: &mut Vec<String>) {
    for node in states {
        if node.history.is_some() {
            continue;
        }
        if node.states.is_empty() {
            if !node.terminal {
                out.push(node.name.clone());
            }
        } else {
            collect_leaves(&node.states, out);
        }
    }
}

#[cfg(test)]
mod tests {
    use fsm_core::json::{JsonLimits, parse};
    use fsm_core::spec::{compile, parse_machine};
    use fsm_core::step::{Outcome, create, step};
    use fsm_core::tree::Tree;

    use super::super::naive_step_at;
    use super::*;

    fn spec_of(src: &str) -> MachineSpec {
        parse_machine(&parse(src.as_bytes(), &JsonLimits::DEFAULT).unwrap()).unwrap()
    }

    #[test]
    fn the_brute_force_cycle_check_names_certain_cycles_only() {
        let certain = r#"{"format":"fsm.machine/1","name":"c","states":[{"name":"a"},{"name":"b"}],"initial":"a","context":[],"events":[],"transitions":[{"from":"a","to":"b"},{"from":"b","if":"true","to":"a"}]}"#;
        assert!(naive_certain_cycle(&spec_of(certain)));
        let guarded = certain
            .replace(r#""if":"true""#, r#""if":"ctx.b""#)
            .replace(
                r#""context":[]"#,
                r#""context":[{"name":"b","ty":"bool","init":"true"}]"#,
            );
        assert!(!naive_certain_cycle(&spec_of(&guarded)));
        let escape = r#"{"format":"fsm.machine/1","name":"e","states":[{"name":"a"},{"name":"b"},{"name":"out"}],"initial":"a","context":[{"name":"b","ty":"bool","init":"true"}],"events":[],"transitions":[{"from":"a","if":"ctx.b","to":"out"},{"from":"a","to":"b"},{"from":"b","to":"a"}]}"#;
        assert!(!naive_certain_cycle(&spec_of(escape)));
        let self_loop = r#"{"format":"fsm.machine/1","name":"s","states":[{"name":"a"}],"initial":"a","context":[],"events":[],"transitions":[{"from":"a"}]}"#;
        assert!(naive_certain_cycle(&spec_of(self_loop)));
        // The engine says the same of each.
        for (src, refused) in [
            (certain, true),
            (guarded.as_str(), false),
            (escape, false),
            (self_loop, true),
        ] {
            let outcome = compile(spec_of(src));
            assert_eq!(
                outcome.is_err_and(|f| f.iter().any(|f| f.code == "def/eventless_cycle")),
                refused,
                "{src}"
            );
        }
    }

    #[test]
    fn a_raise_payload_reads_the_pre_block_snapshot_like_an_emit() {
        let src = r#"{"format":"fsm.machine/1","name":"r","states":[{"name":"a"},{"name":"b"},{"name":"c"}],"initial":"a","context":[{"name":"n","ty":"int","init":"0"},{"name":"seen","ty":"int","init":"-1"}],"events":[{"name":"go","fields":[]},{"name":"r","fields":[{"name":"v","ty":"int"}],"internal":true}],"transitions":[{"from":"a","on":"go","to":"b","do":[{"target":"n","value":"7"}],"raise":[{"event":"r","with":{"v":"ctx.n"}}]},{"from":"b","on":"r","to":"c","do":[{"target":"seen","value":"evt.v"}]}]}"#;
        let m = compile(spec_of(src)).unwrap();
        let t = Tree::for_machine(&m.spec);
        let created = create(&m, &t, &BTreeMap::new(), 0).unwrap();
        let st = InstanceState {
            status: created.status_after,
            configuration: created.configuration_after,
            ctx: created.ctx_after,
            history: created.history_after,
            deadlines: created.deadlines_after,
            pending: Vec::new(),
            invocations: BTreeMap::new(),
            signals: BTreeMap::new(),
        };
        let mut b1 = Budget::new(4096 * 66);
        let mut b2 = Budget::new(4096 * 66);
        let engine = step(&m, &t, &st, "go", &Value::Obj(BTreeMap::new()), 0, &mut b1);
        let naive = naive_step_at(&m, &st, "go", &Value::Obj(BTreeMap::new()), 0, &mut b2);
        match (&engine, &naive) {
            (Outcome::Applied(x), Outcome::Applied(y)) => {
                assert_eq!(x.ctx_after, y.ctx_after);
                assert_eq!(
                    x.ctx_after.get("seen"),
                    Some(&Val::Int(0)),
                    "the snapshot, not 7"
                );
                assert_eq!(x.configuration_after.sequential_leaf(), Some("c"));
                assert_eq!(x.trace.microsteps.len(), 1);
                assert_eq!(y.trace.microsteps.len(), 1);
            }
            other => panic!("{other:?}"),
        }
    }
}
