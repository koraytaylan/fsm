//! Pure instance creation, event stepping, and explicit deadline polling.
//!
//! All time enters as a caller-supplied millisecond timestamp. The engine does
//! not read a clock, run background timers, or drain more than one deadline in
//! a poll.

#![allow(
    clippy::collapsible_if,
    clippy::too_many_arguments,
    clippy::result_large_err,
    clippy::if_same_then_else
)]

use std::collections::BTreeMap;

use crate::expr::eval::{Bindings, Budget, Val, eval};
use crate::expr::parser;
use crate::expr::typeck::{Scope, ScopeKind, Ty, annotate_if_widening};
use crate::json::Value;
use crate::machine::ExprSlot;
use crate::machine::{ActiveConfiguration, CompiledMachine, EnforceMode, InstanceState, Status};
use crate::spec::{Block, DeadlineSpec, HistoryKind, MachineSpec, TransitionSpec, TySpec};

#[derive(Clone)]
enum ExprSlotOwner {
    Transition(usize),
    Deadline(usize),
    Entry(String),
    Exit(String),
}
use crate::trace::{
    BlockKind, BlockTrace, CandidateTrace, DecisionTrace, EmitTrace, GuardTrace, InvariantTrace,
    LevelTrace, SetTrace,
};
use crate::tree::{NodeKind, Tree};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectOut {
    pub name: String,
    pub args: BTreeMap<String, Val>,
    pub k: u32,
}

/// State and diagnostics produced by one successfully applied transition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Applied {
    /// Complete active configuration after the transition.
    pub configuration_after: ActiveConfiguration,
    /// Typed context after all exit, transition, and entry actions.
    pub ctx_after: BTreeMap<String, Val>,
    /// History bindings after exited compound states have been recorded.
    pub history_after: BTreeMap<String, String>,
    /// Active deadline name to absolute due timestamp after rescheduling.
    pub deadlines_after: BTreeMap<String, i64>,
    /// Effects emitted by the accepted pipeline, in execution order.
    pub effects: Vec<EffectOut>,
    /// Names of monitor-mode invariants that failed.
    pub monitor_flags: Vec<String>,
    /// Lifecycle status after the transition.
    ///
    /// A parallel instance is completed only when every region is terminal.
    pub status_after: Status,
    /// Whether the selected transition kept the same active state hierarchy.
    pub internal: bool,
    /// Winning region for a parallel transition, or `None` for sequential flow
    /// and instance creation.
    pub region: Option<String>,
    /// State that owned the selected transition or deadline.
    pub source_state: String,
    /// Document index of the selected transition or deadline definition.
    pub transition_idx: u32,
    /// States exited in leaf-to-root execution order.
    pub exited: Vec<String>,
    /// States entered in root-to-leaf execution order.
    pub entered: Vec<String>,
    /// Complete deterministic decision and action trace.
    pub trace: DecisionTrace,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rejection {
    pub code: &'static str,
    pub message: String,
    pub hint: String,
    pub source_state: Option<String>,
    pub transition_idx: Option<u32>,
    pub block: Option<String>,
    pub span: Option<(u32, u32)>,
    pub trace: DecisionTrace,
    /// Underlying stable cause when `code` is the public `run/action_error`
    /// wrapper: normally an evaluator code (`run/overflow`, `run/div_zero`,
    /// …), or `def/shape` for a grandfathered malformed history target. Never
    /// used as the public code.
    pub cause: Option<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Applied(Applied),
    Rejected(Rejection),
    Ignored,
}

/// A scheduled deadline visible to deterministic callers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingDeadline {
    /// Definition name.
    pub name: String,
    /// Zero-based definition document index.
    pub deadline_idx: u32,
    /// Absolute caller-supplied millisecond timestamp at which it becomes due.
    pub due_ms: i64,
}

/// The result of applying one due deadline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeadlineApplied {
    /// The selected schedule before it was applied.
    pub deadline: PendingDeadline,
    /// The ordinary transition pipeline result.
    pub transition: Applied,
}

/// A rejected poll, optionally after selecting a due deadline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeadlineRejected {
    /// The selected schedule, or `None` when the instance/configuration gate failed.
    pub deadline: Option<PendingDeadline>,
    /// Structured deterministic rejection.
    pub rejection: Rejection,
}

/// Pure result of polling an instance at a caller-supplied timestamp.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeadlineOutcome {
    /// One due deadline was applied.
    Applied(DeadlineApplied),
    /// Polling or application was rejected atomically.
    Rejected(DeadlineRejected),
    /// Nothing was due.
    NotDue {
        /// Earliest active schedule, if the configuration has one.
        next: Option<PendingDeadline>,
    },
}

pub fn validate_event(
    m: &CompiledMachine,
    name: &str,
    payload: &Value,
) -> Result<BTreeMap<String, Val>, Rejection> {
    let ev = m
        .spec
        .events
        .iter()
        .find(|e| e.name == name)
        .ok_or_else(|| {
            let mut r = reject("req/event_unknown", name);
            if let Some(s) =
                crate::ident::suggest(name, m.spec.events.iter().map(|e| e.name.as_str()))
            {
                r.hint = format!("did you mean `{s}`?");
            }
            r
        })?;
    let obj = match payload {
        Value::Obj(o) => o.clone(),
        _ => {
            return Err(reject("req/field_type", "payload must be an object"));
        }
    };
    let mut out = BTreeMap::new();
    for f in &ev.fields {
        let Some(raw) = obj.get(&f.name) else {
            return Err(reject("req/field_missing", &f.name));
        };
        if raw.as_num().is_some() {
            return Err(reject("req/number_token", &f.name));
        }
        let v = parse_typed(raw, &f.ty).map_err(|c| reject(c, &f.name))?;
        if let Val::Enum { ty, variant } = &v {
            let allowed = m.spec.enums.get(ty).cloned().unwrap_or_default();
            if !allowed.iter().any(|x| x == variant) {
                return Err(reject("req/field_type", &f.name));
            }
        }
        if let (Val::Dec(d), TySpec::Dec { scale }) = (&v, &f.ty) {
            if d.scale != *scale {
                return Err(reject("req/field_scale", &f.name));
            }
        }
        out.insert(f.name.clone(), v);
    }
    for k in obj.keys() {
        if !ev.fields.iter().any(|f| f.name == *k) {
            return Err(reject("req/field_unknown", k));
        }
    }
    Ok(out)
}

fn reject(code: &'static str, what: &str) -> Rejection {
    Rejection {
        code,
        message: format!("{code}: {what}"),
        hint: what.into(),
        source_state: None,
        transition_idx: None,
        block: None,
        span: None,
        trace: DecisionTrace::default(),
        cause: None,
    }
}

fn invalid_state_rejection(detail: &str) -> Rejection {
    let mut rejection = reject("run/configuration_invalid", detail);
    rejection.hint = "reconstruct the state from a trusted create/step/poll result".into();
    rejection
}

fn parse_typed(raw: &Value, ty: &TySpec) -> Result<Val, &'static str> {
    match ty {
        TySpec::Bool => raw.as_bool().map(Val::Bool).ok_or("req/field_type"),
        TySpec::Str => raw
            .as_str()
            .map(|s| Val::Str(s.into()))
            .ok_or("req/field_type"),
        TySpec::Int => {
            let s = raw.as_str().ok_or("req/field_type")?;
            s.parse::<i64>().map(Val::Int).map_err(|_| "req/field_type")
        }
        TySpec::Ts => {
            let s = raw.as_str().ok_or("req/field_type")?;
            s.parse::<i64>().map(Val::Ts).map_err(|_| "req/field_type")
        }
        TySpec::Dur => {
            let s = raw.as_str().ok_or("req/field_type")?;
            s.parse::<i64>().map(Val::Dur).map_err(|_| "req/field_type")
        }
        TySpec::Dec { scale } => {
            let s = raw.as_str().ok_or("req/field_type")?;
            match crate::decimal::Dec::parse(s, *scale) {
                Ok(d) => Ok(Val::Dec(d)),
                Err(crate::decimal::DecError::Parse) => {
                    // too many fraction digits
                    if s.contains('.')
                        && s.split('.').nth(1).map(|f| f.len()).unwrap_or(0) > *scale as usize
                    {
                        Err("req/field_scale")
                    } else {
                        Err("req/field_type")
                    }
                }
                Err(_) => Err("req/field_type"),
            }
        }
        TySpec::Enum { of } => {
            let s = raw.as_str().ok_or("req/field_type")?;
            Ok(Val::Enum {
                ty: of.clone(),
                variant: s.into(),
            })
        }
    }
}

/// Deliver one event and apply at most one globally selected transition.
///
/// Parallel regions are scanned in semantic region order; the input state is
/// never mutated. `now_ms` is used only to schedule deadlines on state entry.
pub fn step(
    m: &CompiledMachine,
    t: &Tree,
    st: &InstanceState,
    event: &str,
    payload: &Value,
    now_ms: i64,
    budget: &mut Budget,
) -> Outcome {
    if let Err(error) = t.validate_instance_state(m, st) {
        return Outcome::Rejected(invalid_state_rejection(error.detail()));
    }
    match st.status {
        Status::Completed => {
            return Outcome::Rejected(reject("run/instance_completed", "instance is completed"));
        }
        Status::Cancelled => {
            return Outcome::Rejected(reject("run/instance_cancelled", "instance is cancelled"));
        }
        Status::Running => {}
    }
    let fields = match validate_event(m, event, payload) {
        Ok(f) => f,
        Err(r) => return Outcome::Rejected(r),
    };
    let active_leaves = match t.active_leaves(&st.configuration) {
        Some(active_leaves) => active_leaves,
        None => {
            return Outcome::Rejected(reject(
                "run/configuration_invalid",
                "supply a configuration matching the machine topology and real leaf states",
            ));
        }
    };
    let mut trace = DecisionTrace::default();
    let mut winner: Option<(Option<String>, u16, u16, usize)> = None;
    for (region, leaf) in active_leaves {
        let leaf_name = &t.names[leaf as usize];
        if find_node(&m.spec, leaf_name).is_some_and(|node| node.terminal) {
            continue;
        }
        for sid in t.chain(leaf) {
            let state_name = t.names[sid as usize].clone();
            let indices = m
                .transitions_by
                .get(&(state_name.clone(), event.to_string()))
                .cloned()
                .unwrap_or_default();
            if indices.is_empty() {
                continue;
            }
            let mut level = LevelTrace {
                source_state: state_name.clone(),
                transitions: Vec::new(),
            };
            for index in indices {
                if winner.is_some() {
                    level.transitions.push(CandidateTrace {
                        transition_idx: index as u32,
                        guard: GuardTrace::NotConsidered,
                    });
                    continue;
                }
                let transition = &m.spec.transitions[index];
                match eval_guard(
                    transition,
                    &st.ctx,
                    &fields,
                    budget,
                    &m.spec,
                    event,
                    index,
                    &m.compiled_exprs,
                ) {
                    Ok((true, guard_trace)) => {
                        level.transitions.push(CandidateTrace {
                            transition_idx: index as u32,
                            guard: GuardTrace::Evaluated(guard_trace),
                        });
                        winner = Some((region.map(str::to_string), leaf, sid, index));
                    }
                    Ok((false, guard_trace)) => {
                        level.transitions.push(CandidateTrace {
                            transition_idx: index as u32,
                            guard: GuardTrace::Evaluated(guard_trace),
                        });
                    }
                    Err(mut rejection) => {
                        rejection.source_state = Some(state_name.clone());
                        rejection.transition_idx = Some(index as u32);
                        if let Some(failing_level) = rejection.trace.candidates.first_mut() {
                            failing_level.source_state = state_name.clone();
                        }
                        if !level.transitions.is_empty() {
                            if let Some(failing_level) = rejection.trace.candidates.first_mut() {
                                let mut evaluated = level.transitions;
                                evaluated.append(&mut failing_level.transitions);
                                failing_level.transitions = evaluated;
                            } else {
                                rejection.trace.candidates.insert(0, level);
                            }
                        }
                        let mut prior_trace = trace;
                        prior_trace
                            .candidates
                            .append(&mut rejection.trace.candidates);
                        rejection.trace.candidates = prior_trace.candidates;
                        return Outcome::Rejected(rejection);
                    }
                }
            }
            trace.candidates.push(level);
        }
    }
    let Some((region, leaf, src, tidx)) = winner else {
        let any = trace.candidates.iter().any(|l| !l.transitions.is_empty());
        if !any {
            return match m.spec.on_unhandled {
                crate::spec::Unhandled::Ignore => Outcome::Ignored,
                crate::spec::Unhandled::Reject => Outcome::Rejected(Rejection {
                    code: "run/unhandled",
                    message: format!("no handler for {event} in the active configuration"),
                    hint: "add a transition or send a handled event".into(),
                    source_state: None,
                    transition_idx: None,
                    block: None,
                    span: None,
                    trace,
                    cause: None,
                }),
            };
        }
        return Outcome::Rejected(Rejection {
            code: "run/not_enabled",
            message: format!("all guards false for {event}"),
            hint: "adjust the payload or add a child override".into(),
            source_state: None,
            transition_idx: None,
            block: None,
            span: None,
            trace,
            cause: None,
        });
    };
    let transition = &m.spec.transitions[tidx];
    apply_selected_transition(
        m,
        t,
        st,
        SelectedTransition {
            region,
            leaf,
            source: src,
            target: transition.to.as_deref(),
            action: Block {
                sets: transition.sets.clone(),
                emits: transition.emits.clone(),
            },
            action_kind: BlockKind::Transition,
            owner: ExprSlotOwner::Transition(tidx),
            event_name: event,
            event_fields: &fields,
            sees_event: true,
            public_index: tidx as u32,
            trace,
        },
        now_ms,
        budget,
    )
}

/// Poll the active configuration and apply at most one due deadline.
///
/// Selection is stable by `(due_ms, deadline document index)`. Time is explicit
/// caller input; this function never consults a clock and never loops to drain
/// multiple schedules.
pub fn poll_deadline(
    machine: &CompiledMachine,
    tree: &Tree,
    state: &InstanceState,
    now_ms: i64,
    budget: &mut Budget,
) -> DeadlineOutcome {
    if let Err(error) = tree.validate_instance_state(machine, state) {
        return DeadlineOutcome::Rejected(DeadlineRejected {
            deadline: None,
            rejection: invalid_state_rejection(error.detail()),
        });
    }
    match state.status {
        Status::Completed => {
            return DeadlineOutcome::Rejected(DeadlineRejected {
                deadline: None,
                rejection: reject("run/instance_completed", "instance is completed"),
            });
        }
        Status::Cancelled => {
            return DeadlineOutcome::Rejected(DeadlineRejected {
                deadline: None,
                rejection: reject("run/instance_cancelled", "instance is cancelled"),
            });
        }
        Status::Running => {}
    }
    let active_leaves = match tree.active_leaves(&state.configuration) {
        Some(active_leaves) => active_leaves,
        None => {
            return DeadlineOutcome::Rejected(DeadlineRejected {
                deadline: None,
                rejection: reject(
                    "run/configuration_invalid",
                    "supply a configuration matching the machine topology and real leaf states",
                ),
            });
        }
    };

    let mut selected: Option<(i64, usize, Option<String>, u16, u16)> = None;
    for (deadline_index, deadline) in machine.spec.deadlines.iter().enumerate() {
        let Some(&due_ms) = state.deadlines.get(&deadline.name) else {
            continue;
        };
        let source_location = active_leaves.iter().find_map(|(region, leaf)| {
            if find_node(&machine.spec, &tree.names[*leaf as usize])
                .is_some_and(|node| node.terminal)
            {
                return None;
            }
            tree.chain(*leaf)
                .into_iter()
                .find(|source| tree.names[*source as usize] == deadline.from)
                .map(|source| (region.map(str::to_string), *leaf, source))
        });
        let Some((region, leaf, source)) = source_location else {
            continue;
        };
        let candidate = (due_ms, deadline_index, region, leaf, source);
        if selected
            .as_ref()
            .is_none_or(|current| (candidate.0, candidate.1) < (current.0, current.1))
        {
            selected = Some(candidate);
        }
    }
    let Some((due_ms, deadline_index, region, leaf, source)) = selected else {
        return DeadlineOutcome::NotDue { next: None };
    };
    let deadline = &machine.spec.deadlines[deadline_index];
    let pending = PendingDeadline {
        name: deadline.name.clone(),
        deadline_idx: deadline_index as u32,
        due_ms,
    };
    if due_ms > now_ms {
        return DeadlineOutcome::NotDue {
            next: Some(pending),
        };
    }

    let empty_event = BTreeMap::new();
    let outcome = apply_selected_transition(
        machine,
        tree,
        state,
        SelectedTransition {
            region,
            leaf,
            source,
            target: Some(&deadline.to),
            action: Block {
                sets: deadline.sets.clone(),
                emits: deadline.emits.clone(),
            },
            action_kind: BlockKind::Deadline(deadline.name.clone()),
            owner: ExprSlotOwner::Deadline(deadline_index),
            event_name: "",
            event_fields: &empty_event,
            sees_event: false,
            public_index: deadline_index as u32,
            trace: DecisionTrace::default(),
        },
        now_ms,
        budget,
    );
    match outcome {
        Outcome::Applied(transition) => DeadlineOutcome::Applied(DeadlineApplied {
            deadline: pending,
            transition,
        }),
        Outcome::Rejected(rejection) => DeadlineOutcome::Rejected(DeadlineRejected {
            deadline: Some(pending),
            rejection,
        }),
        Outcome::Ignored => unreachable!("a selected deadline cannot be ignored"),
    }
}

struct SelectedTransition<'a> {
    region: Option<String>,
    leaf: u16,
    source: u16,
    target: Option<&'a str>,
    action: Block,
    action_kind: BlockKind,
    owner: ExprSlotOwner,
    event_name: &'a str,
    event_fields: &'a BTreeMap<String, Val>,
    sees_event: bool,
    public_index: u32,
    trace: DecisionTrace,
}

fn apply_selected_transition(
    machine: &CompiledMachine,
    tree: &Tree,
    state: &InstanceState,
    selected: SelectedTransition<'_>,
    now_ms: i64,
    budget: &mut Budget,
) -> Outcome {
    let SelectedTransition {
        region,
        leaf,
        source,
        target,
        action,
        action_kind,
        owner,
        event_name,
        event_fields,
        sees_event,
        public_index,
        mut trace,
    } = selected;
    let internal = target.is_none();
    let current_leaf = tree.names[leaf as usize].clone();
    let (exited_ids, entered_ids, new_leaf) = if let Some(target) = target {
        let mut target_name = target.to_string();
        let mut extra_descent = Vec::new();
        if let Some(target_id) = tree.id(&target_name) {
            if matches!(tree.kind[target_id as usize], NodeKind::History(_)) {
                let Some(owner_id) = tree.history_owner(target_id) else {
                    let mut rejection = reject(
                        "run/action_error",
                        &format!("history target {target} has no compound owner"),
                    );
                    rejection.hint =
                        "define a replacement machine with history under a compound state".into();
                    rejection.source_state = Some(tree.names[source as usize].clone());
                    rejection.transition_idx = Some(public_index);
                    rejection.cause = Some("def/shape");
                    rejection.trace = trace;
                    return Outcome::Rejected(rejection);
                };
                let owner_name = &tree.names[owner_id as usize];
                extra_descent = tree
                    .history_descent(target_id, state.history.get(owner_name).map(String::as_str));
                target_name = owner_name.clone();
            }
        }
        let target_id = tree.id(&target_name).expect("validated transition target");
        let external_self = target == tree.names[source as usize];
        let domain = if external_self {
            tree.parent[source as usize]
        } else {
            tree.proper_lca(source, target_id)
        };
        let exited = tree.exit_set(leaf, domain);
        let mut entered = tree.entry_path(domain, target_id);
        if matches!(tree.kind[target_id as usize], NodeKind::Compound) && extra_descent.is_empty() {
            entered.extend(tree.initial_descent(target_id));
        }
        entered.extend(extra_descent);
        let leaf_after = entered.last().copied().unwrap_or(target_id);
        (exited, entered, tree.names[leaf_after as usize].clone())
    } else {
        (Vec::new(), Vec::new(), current_leaf.clone())
    };
    let configuration_after = match state.configuration.with_leaf(region.as_deref(), new_leaf) {
        Some(configuration) => configuration,
        None => {
            return Outcome::Rejected(reject("run/unhandled", "transition region is not active"));
        }
    };

    let mut context = state.ctx.clone();
    let mut effects = Vec::new();
    let mut effect_index = 0u32;
    let mut pipeline = Vec::new();

    let apply = |block: &Block,
                 kind: BlockKind,
                 context: &mut BTreeMap<String, Val>,
                 effects: &mut Vec<EffectOut>,
                 effect_index: &mut u32,
                 see_event: bool,
                 block_owner: ExprSlotOwner,
                 budget: &mut Budget|
     -> Result<BlockTrace, Rejection> {
        apply_block(
            block,
            kind,
            context,
            effects,
            effect_index,
            see_event,
            event_fields,
            budget,
            &machine.spec,
            event_name,
            &machine.compiled_exprs,
            block_owner,
        )
    };

    for &id in &exited_ids {
        let name = &tree.names[id as usize];
        if let Some(block) = find_node(&machine.spec, name).and_then(|node| node.exit.as_ref()) {
            match apply(
                block,
                BlockKind::Exit(name.clone()),
                &mut context,
                &mut effects,
                &mut effect_index,
                false,
                ExprSlotOwner::Exit(name.clone()),
                budget,
            ) {
                Ok(block_trace) => pipeline.push(block_trace),
                Err(rejection) => {
                    return Outcome::Rejected(reject_pipeline(rejection, pipeline, &trace));
                }
            }
        }
    }
    match apply(
        &action,
        action_kind,
        &mut context,
        &mut effects,
        &mut effect_index,
        sees_event,
        owner,
        budget,
    ) {
        Ok(block_trace) => pipeline.push(block_trace),
        Err(rejection) => {
            return Outcome::Rejected(reject_pipeline(rejection, pipeline, &trace));
        }
    }
    for &id in &entered_ids {
        let name = &tree.names[id as usize];
        if let Some(block) = find_node(&machine.spec, name).and_then(|node| node.entry.as_ref()) {
            match apply(
                block,
                BlockKind::Entry(name.clone()),
                &mut context,
                &mut effects,
                &mut effect_index,
                false,
                ExprSlotOwner::Entry(name.clone()),
                budget,
            ) {
                Ok(block_trace) => pipeline.push(block_trace),
                Err(rejection) => {
                    return Outcome::Rejected(reject_pipeline(rejection, pipeline, &trace));
                }
            }
        }
    }

    let mut history_after = state.history.clone();
    for &id in &exited_ids {
        if matches!(tree.kind[id as usize], NodeKind::Compound) {
            for &child in &tree.children[id as usize] {
                if let NodeKind::History(kind) = tree.kind[child as usize] {
                    let bound = match kind {
                        HistoryKind::Deep => current_leaf.clone(),
                        HistoryKind::Shallow => tree
                            .chain(leaf)
                            .into_iter()
                            .find(|&node| tree.parent[node as usize] == Some(id))
                            .map(|node| tree.names[node as usize].clone())
                            .unwrap_or_else(|| current_leaf.clone()),
                    };
                    history_after.insert(tree.names[id as usize].clone(), bound);
                }
            }
        }
    }

    let (ok_invariants, monitor_flags, invariant_trace) =
        eval_invariants(&machine.spec, &machine.compiled_exprs, &context, budget);
    if !ok_invariants {
        for block in &mut pipeline {
            block.discarded = true;
        }
        trace.pipeline = pipeline;
        trace.invariants = invariant_trace;
        let evaluation_error = trace.invariants.iter().find_map(|invariant| {
            invariant.error.as_ref().map(|error| {
                (
                    invariant.name.clone(),
                    error.code,
                    error.message.clone(),
                    error.span,
                )
            })
        });
        let failed_invariant = evaluation_error
            .as_ref()
            .map(|(name, _, _, _)| name.clone())
            .or_else(|| {
                trace
                    .invariants
                    .iter()
                    .zip(&machine.spec.invariants)
                    .find(|(result, invariant)| {
                        !result.passed && invariant.mode == EnforceMode::Enforce
                    })
                    .map(|(result, _)| result.name.clone())
            });
        return Outcome::Rejected(Rejection {
            code: "run/invariant",
            message: evaluation_error
                .as_ref()
                .map(|(name, _, message, _)| format!("invariant {name}: {message}"))
                .unwrap_or_else(|| "enforce invariant failed".into()),
            hint: failed_invariant
                .as_ref()
                .map(|name| format!("adjust the action or invariant {name}"))
                .unwrap_or_else(|| "adjust the action or the invariant".into()),
            source_state: Some(tree.names[source as usize].clone()),
            transition_idx: Some(public_index),
            block: evaluation_error
                .as_ref()
                .map(|(name, _, _, _)| format!("invariant({name})")),
            span: evaluation_error.as_ref().and_then(|(_, _, _, span)| *span),
            cause: evaluation_error.as_ref().map(|(_, cause, _, _)| *cause),
            trace,
        });
    }

    let mut deadlines_after = match update_deadline_schedules(
        machine,
        &state.deadlines,
        &exited_ids,
        &entered_ids,
        tree,
        &context,
        now_ms,
        budget,
    ) {
        Ok(deadlines) => deadlines,
        Err(rejection) => {
            let mut rejection = reject_pipeline(rejection, pipeline, &trace);
            rejection.trace.invariants = invariant_trace;
            return Outcome::Rejected(rejection);
        }
    };

    clear_terminal_region_deadlines(machine, tree, &configuration_after, &mut deadlines_after);
    let status_after = if configuration_is_terminal(machine, tree, &configuration_after) {
        deadlines_after.clear();
        Status::Completed
    } else {
        Status::Running
    };
    trace.pipeline = pipeline;
    trace.invariants = invariant_trace;
    Outcome::Applied(Applied {
        configuration_after,
        ctx_after: context,
        history_after,
        deadlines_after,
        effects,
        monitor_flags,
        status_after,
        internal,
        region,
        source_state: tree.names[source as usize].clone(),
        transition_idx: public_index,
        exited: exited_ids
            .iter()
            .map(|&id| tree.names[id as usize].clone())
            .collect(),
        entered: entered_ids
            .iter()
            .map(|&id| tree.names[id as usize].clone())
            .collect(),
        trace,
    })
}

fn clear_terminal_region_deadlines(
    machine: &CompiledMachine,
    tree: &Tree,
    configuration: &ActiveConfiguration,
    schedules: &mut BTreeMap<String, i64>,
) {
    let Some(active_leaves) = tree.active_leaves(configuration) else {
        return;
    };
    for (_, leaf) in active_leaves {
        if !find_node(&machine.spec, &tree.names[leaf as usize]).is_some_and(|node| node.terminal) {
            continue;
        }
        let terminal_chain: std::collections::BTreeSet<&str> = tree
            .chain(leaf)
            .into_iter()
            .map(|state| tree.names[state as usize].as_str())
            .collect();
        schedules.retain(|name, _| {
            machine
                .spec
                .deadlines
                .iter()
                .find(|deadline| deadline.name == *name)
                .is_none_or(|deadline| !terminal_chain.contains(deadline.from.as_str()))
        });
    }
}

fn update_deadline_schedules(
    machine: &CompiledMachine,
    prior: &BTreeMap<String, i64>,
    exited: &[u16],
    entered: &[u16],
    tree: &Tree,
    context: &BTreeMap<String, Val>,
    now_ms: i64,
    budget: &mut Budget,
) -> Result<BTreeMap<String, i64>, Rejection> {
    let mut schedules = prior.clone();
    for state in exited {
        let state_name = &tree.names[*state as usize];
        for deadline in machine
            .spec
            .deadlines
            .iter()
            .filter(|deadline| deadline.from == *state_name)
        {
            schedules.remove(&deadline.name);
        }
    }
    let context_types: BTreeMap<String, Ty> = machine
        .spec
        .context
        .iter()
        .map(|variable| (variable.name.clone(), variable.ty.to_ty()))
        .collect();
    let bindings = Bindings {
        ctx: context,
        evt: None,
    };
    for state in entered {
        let state_name = &tree.names[*state as usize];
        for (index, deadline) in machine.spec.deadlines.iter().enumerate() {
            if deadline.from != *state_name {
                continue;
            }
            let expression = if let Some(compiled) =
                machine.compiled_exprs.get(&ExprSlot::DeadlineAfter(index))
            {
                compiled.expr.clone()
            } else {
                let mut expression = parser::parse(&deadline.after).map_err(|error| {
                    deadline_schedule_rejection(
                        deadline,
                        error.message,
                        error.hint,
                        Some((error.span.start, error.span.end)),
                        Some(error.code),
                    )
                })?;
                annotate_if_widening(
                    &mut expression,
                    &spec_scope(&machine.spec, ScopeKind::Block, &context_types, None),
                );
                expression
            };
            let duration = match eval(&expression, &bindings, budget, false).0 {
                Ok(Val::Dur(duration)) if duration >= 0 => duration,
                Ok(Val::Dur(_)) => {
                    return Err(deadline_schedule_rejection(
                        deadline,
                        "deadline duration is negative".into(),
                        "return a zero or positive duration".into(),
                        None,
                        Some("run/overflow"),
                    ));
                }
                Ok(_) => {
                    return Err(deadline_schedule_rejection(
                        deadline,
                        "deadline expression did not return a duration".into(),
                        "return a duration, for example dur(5, min)".into(),
                        None,
                        None,
                    ));
                }
                Err(error) => {
                    return Err(deadline_schedule_rejection(
                        deadline,
                        error.message,
                        error.hint,
                        Some((error.span.start, error.span.end)),
                        Some(error.code),
                    ));
                }
            };
            let due_ms = now_ms.checked_add(duration).ok_or_else(|| {
                deadline_schedule_rejection(
                    deadline,
                    "deadline due timestamp overflowed".into(),
                    "use a smaller timestamp or duration".into(),
                    None,
                    Some("run/overflow"),
                )
            })?;
            schedules.insert(deadline.name.clone(), due_ms);
        }
    }
    Ok(schedules)
}

fn deadline_schedule_rejection(
    deadline: &DeadlineSpec,
    message: String,
    hint: String,
    span: Option<(u32, u32)>,
    cause: Option<&'static str>,
) -> Rejection {
    Rejection {
        code: "run/action_error",
        message,
        hint,
        source_state: Some(deadline.from.clone()),
        transition_idx: None,
        block: Some(format!("deadline({})", deadline.name)),
        span,
        trace: DecisionTrace::default(),
        cause,
    }
}

fn configuration_is_terminal(
    machine: &CompiledMachine,
    tree: &Tree,
    configuration: &ActiveConfiguration,
) -> bool {
    tree.active_leaves(configuration).is_some_and(|active| {
        !active.is_empty()
            && active.iter().all(|(_, state)| {
                find_node(&machine.spec, &tree.names[*state as usize])
                    .is_some_and(|node| node.terminal)
            })
    })
}

fn eval_guard(
    tr: &TransitionSpec,
    ctx: &BTreeMap<String, Val>,
    evt: &BTreeMap<String, Val>,
    budget: &mut Budget,
    spec: &MachineSpec,
    event_name: &str,
    tidx: usize,
    compiled: &BTreeMap<ExprSlot, crate::machine::CompiledExpr>,
) -> Result<(bool, crate::expr::eval::TraceNode), Rejection> {
    match &tr.guard {
        None => {
            // Omitted guards have historically evaluated an implicit `true`.
            // Keep that one-tick accounting for replay compatibility; the
            // compiler includes the worst-case tick in `def/limit_eval`.
            let dummy = parser::parse("true").expect("static guard expression");
            let bindings = Bindings {
                ctx,
                evt: Some(evt),
            };
            match eval(&dummy, &bindings, budget, true) {
                (Ok(Val::Bool(value)), Some(trace)) => Ok((value, trace)),
                (Err(error), trace) => Err(Rejection {
                    code: "run/guard_error",
                    message: error.message,
                    hint: error.hint,
                    source_state: None,
                    transition_idx: Some(tidx as u32),
                    block: None,
                    span: Some((error.span.start, error.span.end)),
                    cause: Some(error.code),
                    trace: DecisionTrace {
                        candidates: vec![LevelTrace {
                            source_state: String::new(),
                            transitions: vec![CandidateTrace {
                                transition_idx: tidx as u32,
                                guard: GuardTrace::Evaluated(trace.unwrap_or(
                                    crate::expr::eval::TraceNode {
                                        span: error.span,
                                        outcome: crate::expr::eval::TraceOutcome::Error {
                                            code: error.code,
                                            inputs: Vec::new(),
                                        },
                                        children: vec![],
                                    },
                                )),
                            }],
                        }],
                        ..DecisionTrace::default()
                    },
                }),
                _ => Err(reject("run/guard_error", "guard not bool")),
            }
        }
        Some(src) => {
            let e = if let Some(c) = compiled.get(&ExprSlot::TransitionGuard(tidx)) {
                c.expr.clone()
            } else {
                let ctx_tys: BTreeMap<String, Ty> = spec
                    .context
                    .iter()
                    .map(|c| (c.name.clone(), c.ty.to_ty()))
                    .collect();
                let evt_tys: BTreeMap<String, Ty> = spec
                    .events
                    .iter()
                    .find(|e| e.name == event_name)
                    .map(|e| {
                        e.fields
                            .iter()
                            .map(|f| (f.name.clone(), f.ty.to_ty()))
                            .collect()
                    })
                    .unwrap_or_default();
                let mut e = parser::parse(src).map_err(|err| Rejection {
                    code: "run/guard_error",
                    message: err.message,
                    hint: err.hint,
                    source_state: None,
                    transition_idx: None,
                    block: None,
                    span: Some((err.span.start, err.span.end)),
                    trace: DecisionTrace::default(),
                    cause: Some(err.code),
                })?;
                annotate_if_widening(
                    &mut e,
                    &spec_scope(spec, ScopeKind::Guard, &ctx_tys, Some(&evt_tys)),
                );
                e
            };
            let b = Bindings {
                ctx,
                evt: Some(evt),
            };
            match eval(&e, &b, budget, true) {
                (Ok(Val::Bool(v)), t) => Ok((v, t.unwrap())),
                (Err(err), t) => Err(Rejection {
                    code: "run/guard_error",
                    message: err.message,
                    hint: err.hint,
                    source_state: None,
                    transition_idx: Some(tidx as u32),
                    block: None,
                    span: Some((err.span.start, err.span.end)),
                    cause: Some(err.code),
                    trace: DecisionTrace {
                        candidates: vec![LevelTrace {
                            source_state: String::new(),
                            transitions: vec![CandidateTrace {
                                transition_idx: tidx as u32,
                                guard: GuardTrace::Evaluated(t.unwrap_or(
                                    crate::expr::eval::TraceNode {
                                        span: err.span,
                                        outcome: crate::expr::eval::TraceOutcome::Error {
                                            code: err.code,
                                            inputs: Vec::new(),
                                        },
                                        children: vec![],
                                    },
                                )),
                            }],
                        }],
                        ..DecisionTrace::default()
                    },
                }),
                _ => Err(reject("run/guard_error", "guard not bool")),
            }
        }
    }
}

fn coerce_to_ty(v: Val, ty: &TySpec) -> Result<Val, &'static str> {
    match (v, ty) {
        (Val::Dec(d), TySpec::Dec { scale }) if d.scale == *scale => Ok(Val::Dec(d)),
        (Val::Dec(d), TySpec::Dec { scale }) if d.scale < *scale => d
            .rescale_up(*scale)
            .map(Val::Dec)
            .map_err(|_| "run/overflow"),
        (Val::Dec(_), TySpec::Dec { .. }) => Err("req/field_scale"),
        (v, ty) if val_matches(&v, ty) => Ok(v),
        _ => Err("req/field_type"),
    }
}

fn spec_scope<'a>(
    spec: &'a MachineSpec,
    kind: ScopeKind,
    ctx_tys: &'a BTreeMap<String, Ty>,
    evt_tys: Option<&'a BTreeMap<String, Ty>>,
) -> Scope<'a> {
    Scope {
        kind,
        ctx: ctx_tys,
        evt: evt_tys,
        enums: &spec.enums,
    }
}

fn compiled_or_annotate(
    src: &str,
    slot: &ExprSlot,
    compiled: &BTreeMap<ExprSlot, crate::machine::CompiledExpr>,
    spec: &MachineSpec,
    kind: ScopeKind,
    ctx_tys: &BTreeMap<String, Ty>,
    evt_tys: Option<&BTreeMap<String, Ty>>,
    block: &BlockKind,
) -> Result<crate::expr::ast::Expr, Rejection> {
    if let Some(c) = compiled.get(slot) {
        return Ok(c.expr.clone());
    }
    let mut e = parser::parse(src).map_err(|err| action_err(block, err.message, err.hint))?;
    annotate_if_widening(&mut e, &spec_scope(spec, kind, ctx_tys, evt_tys));
    Ok(e)
}

fn owner_set_slot(owner: &ExprSlotOwner, i: usize) -> ExprSlot {
    match owner {
        ExprSlotOwner::Transition(t) => ExprSlot::TransitionSet(*t, i),
        ExprSlotOwner::Deadline(deadline) => ExprSlot::DeadlineSet(*deadline, i),
        ExprSlotOwner::Entry(n) => ExprSlot::StateEntrySet(n.clone(), i),
        ExprSlotOwner::Exit(n) => ExprSlot::StateExitSet(n.clone(), i),
    }
}

fn owner_emit_slot(owner: &ExprSlotOwner, i: usize, arg: &str) -> ExprSlot {
    match owner {
        ExprSlotOwner::Transition(t) => ExprSlot::TransitionEmitArg(*t, i, arg.into()),
        ExprSlotOwner::Deadline(deadline) => ExprSlot::DeadlineEmitArg(*deadline, i, arg.into()),
        ExprSlotOwner::Entry(n) => ExprSlot::StateEntryEmitArg(n.clone(), i, arg.into()),
        ExprSlotOwner::Exit(n) => ExprSlot::StateExitEmitArg(n.clone(), i, arg.into()),
    }
}

fn apply_block(
    block: &Block,
    kind: BlockKind,
    ctx: &mut BTreeMap<String, Val>,
    effects: &mut Vec<EffectOut>,
    k: &mut u32,
    see_evt: bool,
    evt: &BTreeMap<String, Val>,
    budget: &mut Budget,
    spec: &crate::spec::MachineSpec,
    event_name: &str,
    compiled: &BTreeMap<ExprSlot, crate::machine::CompiledExpr>,
    owner: ExprSlotOwner,
) -> Result<BlockTrace, Rejection> {
    let snapshot = ctx.clone();
    let mut sets = Vec::new();
    let mut emits = Vec::new();
    let evt_ref = if see_evt { Some(evt) } else { None };
    let b = Bindings {
        ctx: &snapshot,
        evt: evt_ref,
    };
    let ctx_tys: BTreeMap<String, Ty> = spec
        .context
        .iter()
        .map(|c| (c.name.clone(), c.ty.to_ty()))
        .collect();
    let evt_tys: Option<BTreeMap<String, Ty>> =
        spec.events.iter().find(|e| e.name == event_name).map(|e| {
            e.fields
                .iter()
                .map(|f| (f.name.clone(), f.ty.to_ty()))
                .collect()
        });
    let scope_kind = if see_evt {
        ScopeKind::TransitionAction
    } else {
        ScopeKind::Block
    };
    for (i, set) in block.sets.iter().enumerate() {
        let e = compiled_or_annotate(
            &set.value,
            &owner_set_slot(&owner, i),
            compiled,
            spec,
            scope_kind,
            &ctx_tys,
            evt_tys.as_ref(),
            &kind,
        )?;
        match eval(&e, &b, budget, true) {
            (Ok(v), Some(tn)) => {
                let v = if let Some(decl) = spec.context.iter().find(|c| c.name == set.target) {
                    coerce_to_ty(v, &decl.ty).map_err(|c| action_err(&kind, c.into(), c.into()))?
                } else {
                    v
                };
                let before = ctx
                    .get(&set.target)
                    .map(Val::canonical_string)
                    .unwrap_or_default();
                sets.push(SetTrace {
                    target: set.target.clone(),
                    before,
                    after: v.canonical_string(),
                    expr: tn,
                });
                ctx.insert(set.target.clone(), v);
            }
            (Err(err), tn) => {
                if let Some(tn) = tn {
                    sets.push(SetTrace {
                        target: set.target.clone(),
                        before: ctx
                            .get(&set.target)
                            .map(Val::canonical_string)
                            .unwrap_or_default(),
                        after: String::new(),
                        expr: tn,
                    });
                }
                return Err(action_err_at(
                    &kind,
                    "run/action_error",
                    err.message,
                    err.hint,
                    Some((err.span.start, err.span.end)),
                    sets,
                    emits,
                    Some(err.code),
                ));
            }
            _ => {
                return Err(action_err(
                    &kind,
                    "eval failed".into(),
                    "check the expression".into(),
                ));
            }
        }
    }
    for (ei, em) in block.emits.iter().enumerate() {
        let mut args = BTreeMap::new();
        let fx = spec.effects.iter().find(|e| e.name == em.effect);
        for (name, src) in &em.args {
            let e = compiled_or_annotate(
                src,
                &owner_emit_slot(&owner, ei, name),
                compiled,
                spec,
                scope_kind,
                &ctx_tys,
                evt_tys.as_ref(),
                &kind,
            )?;
            match eval(&e, &b, budget, true) {
                (Ok(v), _) => {
                    let v = if let Some(f) =
                        fx.and_then(|e| e.fields.iter().find(|f| f.name == *name))
                    {
                        coerce_to_ty(v, &f.ty).map_err(|c| action_err(&kind, c.into(), c.into()))?
                    } else {
                        v
                    };
                    args.insert(name.clone(), v);
                }
                (Err(err), tn) => {
                    emits.push(EmitTrace {
                        effect: em.effect.clone(),
                        k: *k,
                        expr: tn,
                    });
                    return Err(action_err_at(
                        &kind,
                        "run/action_error",
                        err.message,
                        err.hint,
                        Some((err.span.start, err.span.end)),
                        sets,
                        emits,
                        Some(err.code),
                    ));
                }
            }
        }
        effects.push(EffectOut {
            name: em.effect.clone(),
            args,
            k: *k,
        });
        emits.push(EmitTrace {
            effect: em.effect.clone(),
            k: *k,
            expr: None,
        });
        *k += 1;
    }
    Ok(BlockTrace {
        block: kind,
        sets,
        emits,
        discarded: false,
    })
}

fn reject_pipeline(
    mut r: Rejection,
    mut done: Vec<BlockTrace>,
    trace: &DecisionTrace,
) -> Rejection {
    for p in &mut done {
        p.discarded = true;
    }
    for p in &mut r.trace.pipeline {
        p.discarded = true;
    }
    done.append(&mut r.trace.pipeline);
    r.trace.pipeline = done;
    r.trace.candidates = trace.candidates.clone();
    r
}

fn action_err(kind: &BlockKind, message: String, hint: String) -> Rejection {
    action_err_at(
        kind,
        "run/action_error",
        message,
        hint,
        None,
        Vec::new(),
        Vec::new(),
        None,
    )
}

fn action_err_at(
    kind: &BlockKind,
    code: &'static str,
    message: String,
    hint: String,
    span: Option<(u32, u32)>,
    sets: Vec<SetTrace>,
    emits: Vec<EmitTrace>,
    cause: Option<&'static str>,
) -> Rejection {
    Rejection {
        code,
        message,
        hint,
        source_state: None,
        transition_idx: None,
        block: Some(kind.as_label()),
        span,
        cause,
        trace: DecisionTrace {
            pipeline: vec![BlockTrace {
                block: kind.clone(),
                sets,
                emits,
                discarded: true,
            }],
            ..DecisionTrace::default()
        },
    }
}

fn eval_invariants(
    spec: &MachineSpec,
    compiled: &BTreeMap<ExprSlot, crate::machine::CompiledExpr>,
    ctx: &BTreeMap<String, Val>,
    budget: &mut Budget,
) -> (bool, Vec<String>, Vec<InvariantTrace>) {
    let mut ok = true;
    let mut flags = Vec::new();
    let mut traces = Vec::new();
    let b = Bindings { ctx, evt: None };
    let ctx_tys: BTreeMap<String, Ty> = spec
        .context
        .iter()
        .map(|c| (c.name.clone(), c.ty.to_ty()))
        .collect();
    for (i, inv) in spec.invariants.iter().enumerate() {
        let e = if let Some(c) = compiled.get(&ExprSlot::Invariant(i)) {
            c.expr.clone()
        } else {
            match parser::parse(&inv.expr) {
                Ok(mut e) => {
                    annotate_if_widening(
                        &mut e,
                        &spec_scope(spec, ScopeKind::Invariant, &ctx_tys, None),
                    );
                    e
                }
                Err(_) => {
                    ok = false;
                    traces.push(InvariantTrace {
                        name: inv.name.clone(),
                        passed: false,
                        expr: None,
                        error: None,
                    });
                    continue;
                }
            }
        };
        match eval(&e, &b, budget, true) {
            (Ok(Val::Bool(true)), tn) => {
                traces.push(InvariantTrace {
                    name: inv.name.clone(),
                    passed: true,
                    expr: tn,
                    error: None,
                });
            }
            (Ok(Val::Bool(false)), tn) => {
                traces.push(InvariantTrace {
                    name: inv.name.clone(),
                    passed: false,
                    expr: tn,
                    error: None,
                });
                match inv.mode {
                    EnforceMode::Enforce => ok = false,
                    EnforceMode::Monitor => flags.push(inv.name.clone()),
                }
            }
            (Err(err), tn) => {
                traces.push(InvariantTrace {
                    name: inv.name.clone(),
                    passed: false,
                    expr: tn.clone(),
                    error: Some(crate::trace::InvariantEvalError {
                        code: err.code,
                        message: err.message,
                        span: Some((err.span.start, err.span.end)),
                        expr: tn,
                    }),
                });
                ok = false;
            }
            (_, tn) => {
                traces.push(InvariantTrace {
                    name: inv.name.clone(),
                    passed: false,
                    expr: tn,
                    error: None,
                });
                ok = false;
            }
        }
    }
    (ok, flags, traces)
}

fn find_node<'a>(spec: &'a MachineSpec, name: &str) -> Option<&'a crate::spec::StateNode> {
    fn rec<'a>(
        nodes: &'a [crate::spec::StateNode],
        name: &str,
    ) -> Option<&'a crate::spec::StateNode> {
        for n in nodes {
            if n.name == name {
                return Some(n);
            }
            if let Some(f) = rec(&n.states, name) {
                return Some(f);
            }
        }
        None
    }
    for (_, states, _) in spec.state_groups() {
        if let Some(node) = rec(states, name) {
            return Some(node);
        }
    }
    None
}

/// Create an instance from a definition, context overrides, and caller time.
///
/// Every region enters its initial chain. Deadlines on those chains are
/// scheduled relative to `now_ms`. Creation is pure; durable hosts must not
/// journal a failed result or consume an instance id or sequence number.
pub fn create(
    m: &CompiledMachine,
    t: &Tree,
    overrides: &BTreeMap<String, Val>,
    now_ms: i64,
) -> Result<Applied, Rejection> {
    // validate overrides
    let ctx_map: BTreeMap<_, _> = m
        .spec
        .context
        .iter()
        .map(|c| (c.name.as_str(), c))
        .collect();
    for (k, v) in overrides {
        let Some(decl) = ctx_map.get(k.as_str()) else {
            return Err(reject("req/field_unknown", k));
        };
        if !val_matches(v, &decl.ty) {
            return Err(reject("req/field_type", k));
        }
        if let (Val::Dec(d), TySpec::Dec { scale }) = (v, &decl.ty) {
            if d.scale != *scale {
                return Err(reject("req/field_scale", k));
            }
        }
        if let (Val::Enum { ty, variant }, TySpec::Enum { of }) = (v, &decl.ty) {
            if ty != of {
                return Err(reject("req/field_type", k));
            }
            let allowed = m.spec.enums.get(of).cloned().unwrap_or_default();
            if !allowed.iter().any(|x| x == variant) {
                return Err(reject("req/field_type", k));
            }
        }
    }
    let mut ctx = BTreeMap::new();
    for c in &m.spec.context {
        let v = if let Some(ov) = overrides.get(&c.name) {
            ov.clone()
        } else {
            parse_init(&c.init, &c.ty).map_err(|code| reject(code, &c.name))?
        };
        if let TySpec::Enum { of } = &c.ty {
            if let Val::Enum { variant, .. } = &v {
                let allowed = m.spec.enums.get(of).cloned().unwrap_or_default();
                if !allowed.iter().any(|x| x == variant) {
                    return Err(reject("req/field_type", &c.name));
                }
            }
        }
        ctx.insert(c.name.clone(), v);
    }
    let mut entered = Vec::new();
    let mut parallel_leaves = BTreeMap::new();
    let mut sequential_leaf = None;
    for (region, root_initial) in &t.root_initials {
        let mut region_entry = vec![*root_initial];
        region_entry.extend(t.initial_descent(*root_initial));
        let leaf = region_entry
            .last()
            .map(|state| t.names[*state as usize].clone())
            .ok_or_else(|| reject("run/create_failed", "empty initial descent"))?;
        match region {
            Some(region) => {
                parallel_leaves.insert(region.clone(), leaf);
            }
            None => sequential_leaf = Some(leaf),
        }
        entered.extend(region_entry);
    }
    let configuration_after = match &m.spec.topology {
        crate::spec::Topology::Sequential { .. } => ActiveConfiguration::Sequential {
            leaf: sequential_leaf.ok_or_else(|| reject("run/create_failed", "bad initial"))?,
        },
        crate::spec::Topology::Parallel { regions } => {
            if parallel_leaves.len() != regions.len() {
                return Err(reject("run/create_failed", "bad region initial"));
            }
            ActiveConfiguration::Parallel {
                leaves: parallel_leaves,
            }
        }
    };
    let mut effects = Vec::new();
    let mut k = 0u32;
    let mut pipeline = Vec::new();
    let empty_evt = BTreeMap::new();
    let mut budget = Budget::new(crate::limits::MAX_EVAL_TICKS);
    for &id in &entered {
        let name = &t.names[id as usize];
        if let Some(node) = find_node(&m.spec, name) {
            if let Some(b) = &node.entry {
                match apply_block(
                    b,
                    BlockKind::Entry(name.clone()),
                    &mut ctx,
                    &mut effects,
                    &mut k,
                    false,
                    &empty_evt,
                    &mut budget,
                    &m.spec,
                    "",
                    &m.compiled_exprs,
                    ExprSlotOwner::Entry(name.clone()),
                ) {
                    Ok(bt) => pipeline.push(bt),
                    Err(inner) => {
                        let mut r = reject_pipeline(inner, pipeline, &DecisionTrace::default());
                        r.code = "run/create_failed";
                        return Err(r);
                    }
                }
            }
        }
    }
    let (ok_inv, flags, inv_trace) = eval_invariants(&m.spec, &m.compiled_exprs, &ctx, &mut budget);
    if !ok_inv {
        for p in &mut pipeline {
            p.discarded = true;
        }
        let eval_err = inv_trace
            .iter()
            .find_map(|i| i.error.as_ref().map(|e| (i.name.as_str(), e)));
        let failed_inv = eval_err.map(|(name, _)| name).or_else(|| {
            inv_trace
                .iter()
                .zip(&m.spec.invariants)
                .find(|(trace, spec)| !trace.passed && spec.mode == EnforceMode::Enforce)
                .map(|(trace, _)| trace.name.as_str())
        });
        return Err(Rejection {
            code: "run/create_failed",
            message: eval_err
                .map(|(n, e)| format!("invariant {n}: {}", e.message))
                .unwrap_or_else(|| "invariant failed at create".into()),
            hint: failed_inv
                .map(|n| format!("fix inits or invariant {n}"))
                .unwrap_or_else(|| "fix inits or the invariant".into()),
            source_state: None,
            transition_idx: None,
            block: eval_err.map(|(n, _)| format!("invariant({n})")),
            span: eval_err.and_then(|(_, e)| e.span),
            cause: eval_err.map(|(_, e)| e.code),
            trace: DecisionTrace {
                pipeline,
                invariants: inv_trace,
                ..DecisionTrace::default()
            },
        });
    }
    let mut deadlines_after = match update_deadline_schedules(
        m,
        &BTreeMap::new(),
        &[],
        &entered,
        t,
        &ctx,
        now_ms,
        &mut budget,
    ) {
        Ok(deadlines) => deadlines,
        Err(inner) => {
            let mut rejection = reject_pipeline(inner, pipeline, &DecisionTrace::default());
            rejection.trace.invariants = inv_trace;
            rejection.code = "run/create_failed";
            return Err(rejection);
        }
    };
    let status_after = if configuration_is_terminal(m, t, &configuration_after) {
        deadlines_after.clear();
        Status::Completed
    } else {
        Status::Running
    };
    Ok(Applied {
        configuration_after,
        ctx_after: ctx,
        history_after: BTreeMap::new(),
        deadlines_after,
        effects,
        monitor_flags: flags,
        status_after,
        internal: false,
        region: None,
        source_state: String::new(),
        transition_idx: 0,
        exited: Vec::new(),
        entered: entered
            .iter()
            .map(|&i| t.names[i as usize].clone())
            .collect(),
        trace: DecisionTrace {
            pipeline,
            invariants: inv_trace,
            ..DecisionTrace::default()
        },
    })
}

fn parse_init(s: &str, ty: &TySpec) -> Result<Val, &'static str> {
    match ty {
        TySpec::Int => s.parse::<i64>().map(Val::Int).map_err(|_| "req/field_type"),
        TySpec::Bool => match s {
            "true" => Ok(Val::Bool(true)),
            "false" => Ok(Val::Bool(false)),
            _ => Err("req/field_type"),
        },
        TySpec::Str => Ok(Val::Str(s.into())),
        TySpec::Ts => s.parse::<i64>().map(Val::Ts).map_err(|_| "req/field_type"),
        TySpec::Dur => s.parse::<i64>().map(Val::Dur).map_err(|_| "req/field_type"),
        TySpec::Dec { scale } => crate::decimal::Dec::parse(s, *scale)
            .map(Val::Dec)
            .map_err(|_| "req/field_type"),
        TySpec::Enum { of } => Ok(Val::Enum {
            ty: of.clone(),
            variant: s.into(),
        }),
    }
}

fn val_matches(v: &Val, ty: &TySpec) -> bool {
    match (v, ty) {
        (Val::Int(_), TySpec::Int)
        | (Val::Bool(_), TySpec::Bool)
        | (Val::Str(_), TySpec::Str)
        | (Val::Ts(_), TySpec::Ts)
        | (Val::Dur(_), TySpec::Dur) => true,
        (Val::Dec(_), TySpec::Dec { .. }) => true,
        (Val::Enum { ty: got, .. }, TySpec::Enum { of }) => got == of,
        _ => false,
    }
}

/// Helper used by tests to parse a payload object of string fields.
pub fn payload_from_pairs(pairs: &[(&str, &str)]) -> Value {
    let mut m = BTreeMap::new();
    for (k, v) in pairs {
        m.insert((*k).into(), Value::Str((*v).into()));
    }
    Value::Obj(m)
}
