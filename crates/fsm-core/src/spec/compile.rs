use std::collections::{BTreeMap, BTreeSet};

use crate::expr::parser;
use crate::expr::typeck::{Scope, ScopeKind, Ty, typecheck};
use crate::limits;
use crate::machine::{CompiledExpr, CompiledMachine, ExprSlot};

use super::compat::{accepted_identity, identity_document};
use super::validate::{DefinitionCompatibility, validate_with_compatibility};
use super::{Block, Finding, MachineSpec, Severity, StateNode, TransitionSpec};

pub fn compile(spec: MachineSpec) -> Result<CompiledMachine, Vec<Finding>> {
    compile_with_compatibility(spec, DefinitionCompatibility::Current)
}

pub(super) fn compile_with_compatibility(
    spec: MachineSpec,
    compatibility: DefinitionCompatibility,
) -> Result<CompiledMachine, Vec<Finding>> {
    validate_with_compatibility(&spec, compatibility)?;
    let mut errs = Vec::new();
    let mut compiled_exprs = BTreeMap::new();
    let ctx_tys: BTreeMap<String, Ty> = spec
        .context
        .iter()
        .map(|c| (c.name.clone(), c.ty.to_ty()))
        .collect();
    let state_names = spec.state_names();
    let enums = spec.enums.clone();
    let mut event_map: BTreeMap<String, BTreeMap<String, Ty>> = spec
        .events
        .iter()
        .map(|e| {
            (
                e.name.clone(),
                e.fields
                    .iter()
                    .map(|f| (f.name.clone(), f.ty.to_ty()))
                    .collect(),
            )
        })
        .collect();
    // A generated `$done.*` event is in scope for its handler's guard and
    // block exactly like a declared fieldless event: `evt` binds to an empty
    // object, so a field reference is `expr/unknown_field`, not "no event".
    // A join that needs data reads `ctx`, which the finishing sub-workflow
    // already wrote.
    for name in super::generated_event_names(&spec) {
        event_map.entry(name).or_default();
    }
    let effect_map: BTreeMap<String, BTreeMap<String, Ty>> = spec
        .effects
        .iter()
        .map(|e| {
            (
                e.name.clone(),
                e.fields
                    .iter()
                    .map(|f| (f.name.clone(), f.ty.to_ty()))
                    .collect(),
            )
        })
        .collect();

    let bind = |src: &str,
                scope: &Scope<'_>,
                path: &str,
                slot: ExprSlot,
                compiled_exprs: &mut BTreeMap<ExprSlot, CompiledExpr>,
                errs: &mut Vec<Finding>|
     -> Option<Ty> {
        match parser::parse(src) {
            Ok(e) => match typecheck(&e, scope) {
                Ok((ty, annotated, twarns)) => {
                    compiled_exprs.insert(
                        slot,
                        CompiledExpr {
                            source: src.to_string(),
                            ty: ty.clone(),
                            expr: annotated,
                        },
                    );
                    for w in twarns {
                        errs.push(Finding::warn(w.code, path, w.message, ""));
                    }
                    Some(ty)
                }
                Err(err) => {
                    let mut f = Finding::err(err.code, path, err.message, err.hint);
                    f.span = Some(err.span);
                    errs.push(f);
                    None
                }
            },
            Err(err) => {
                let mut f = Finding::err(err.code, path, err.message, err.hint);
                f.span = Some(err.span);
                errs.push(f);
                None
            }
        }
    };

    enum BlockOwner {
        Transition(usize),
        Deadline(usize),
        Entry(String),
        Exit(String),
    }
    let set_slot = |owner: &BlockOwner, i: usize| -> ExprSlot {
        match owner {
            BlockOwner::Transition(t) => ExprSlot::TransitionSet(*t, i),
            BlockOwner::Deadline(deadline) => ExprSlot::DeadlineSet(*deadline, i),
            BlockOwner::Entry(n) => ExprSlot::StateEntrySet(n.clone(), i),
            BlockOwner::Exit(n) => ExprSlot::StateExitSet(n.clone(), i),
        }
    };
    let emit_slot = |owner: &BlockOwner, i: usize, arg: &str| -> ExprSlot {
        match owner {
            BlockOwner::Transition(t) => ExprSlot::TransitionEmitArg(*t, i, arg.into()),
            BlockOwner::Deadline(deadline) => ExprSlot::DeadlineEmitArg(*deadline, i, arg.into()),
            BlockOwner::Entry(n) => ExprSlot::StateEntryEmitArg(n.clone(), i, arg.into()),
            BlockOwner::Exit(n) => ExprSlot::StateExitEmitArg(n.clone(), i, arg.into()),
        }
    };
    let raise_slot = |owner: &BlockOwner, i: usize, field: &str| -> ExprSlot {
        match owner {
            BlockOwner::Transition(t) => ExprSlot::TransitionRaiseArg(*t, i, field.into()),
            BlockOwner::Deadline(deadline) => {
                ExprSlot::DeadlineRaiseArg(*deadline, i, field.into())
            }
            BlockOwner::Entry(n) => ExprSlot::StateEntryRaiseArg(n.clone(), i, field.into()),
            BlockOwner::Exit(n) => ExprSlot::StateExitRaiseArg(n.clone(), i, field.into()),
        }
    };
    let check_block = |block: &Block,
                       scope: &Scope<'_>,
                       path: &str,
                       owner: &BlockOwner,
                       compiled_exprs: &mut BTreeMap<ExprSlot, CompiledExpr>,
                       errs: &mut Vec<Finding>| {
        let mut seen = BTreeSet::new();
        for (i, set) in block.sets.iter().enumerate() {
            if !seen.insert(&set.target) {
                errs.push(Finding::err(
                    "def/dup_set",
                    format!("{path}/do/{i}"),
                    format!("duplicate set {}", set.target),
                    "set each target at most once per block",
                ));
            }
            let Some(rhs) = bind(
                &set.value,
                scope,
                &format!("{path}/do/{i}/value"),
                set_slot(owner, i),
                compiled_exprs,
                errs,
            ) else {
                continue;
            };
            match ctx_tys.get(&set.target) {
                Some(want) if *want == rhs => {}
                Some(want) => {
                    errs.push(Finding::err(
                        "def/assign_type",
                        format!("{path}/do/{i}"),
                        format!("cannot assign {rhs} to {} ({want})", set.target),
                        "make the scale and class match exactly",
                    ));
                }
                None => {
                    errs.push(Finding::err(
                        "def/unknown_state",
                        format!("{path}/do/{i}/target"),
                        format!("unknown target {}", set.target),
                        "set a declared context variable",
                    ));
                }
            }
        }
        for (i, em) in block.emits.iter().enumerate() {
            let fields = effect_map.get(&em.effect);
            for (k, src) in &em.args {
                let Some(got) = bind(
                    src,
                    scope,
                    &format!("{path}/emit/{i}/args/{k}"),
                    emit_slot(owner, i, k),
                    compiled_exprs,
                    errs,
                ) else {
                    continue;
                };
                if let Some(fs) = fields {
                    if let Some(want) = fs.get(k) {
                        if *want != got {
                            errs.push(Finding::err(
                                "expr/type_mismatch",
                                format!("{path}/emit/{i}/args/{k}"),
                                format!("have {got}, want {want}"),
                                "match the effect field type",
                            ));
                        }
                    }
                }
            }
        }
        // A raise's payload is typed exactly like a context assignment:
        // class and decimal scale must match the declared field.
        for (i, raise) in block.raises.iter().enumerate() {
            let fields = event_map.get(&raise.event);
            for (field, src) in &raise.with {
                let Some(got) = bind(
                    src,
                    scope,
                    &format!("{path}/raise/{i}/with/{field}"),
                    raise_slot(owner, i, field),
                    compiled_exprs,
                    errs,
                ) else {
                    continue;
                };
                if let Some(want) = fields.and_then(|fs| fs.get(field)) {
                    if *want != got {
                        errs.push(Finding::err(
                            "def/assign_type",
                            format!("{path}/raise/{i}/with/{field}"),
                            format!("cannot raise {}.{field} with {got} ({want})", raise.event),
                            "make the scale and class match exactly",
                        ));
                    }
                }
            }
        }
    };

    // entry/exit blocks
    fn walk_blocks(
        nodes: &[StateNode],
        check_block: &dyn Fn(
            &Block,
            &Scope<'_>,
            &str,
            &BlockOwner,
            &mut BTreeMap<ExprSlot, CompiledExpr>,
            &mut Vec<Finding>,
        ),
        scope: &Scope<'_>,
        compiled_exprs: &mut BTreeMap<ExprSlot, CompiledExpr>,
        errs: &mut Vec<Finding>,
    ) {
        for n in nodes {
            if let Some(b) = &n.entry {
                check_block(
                    b,
                    scope,
                    &format!("/states/{}/entry", n.name),
                    &BlockOwner::Entry(n.name.clone()),
                    compiled_exprs,
                    errs,
                );
            }
            if let Some(b) = &n.exit {
                check_block(
                    b,
                    scope,
                    &format!("/states/{}/exit", n.name),
                    &BlockOwner::Exit(n.name.clone()),
                    compiled_exprs,
                    errs,
                );
            }
            walk_blocks(&n.states, check_block, scope, compiled_exprs, errs);
        }
    }
    let block_scope = Scope {
        kind: ScopeKind::Block,
        ctx: &ctx_tys,
        evt: None,
        enums: &enums,
        states: &state_names,
    };
    for (_, states, _) in spec.state_groups() {
        walk_blocks(
            states,
            &check_block,
            &block_scope,
            &mut compiled_exprs,
            &mut errs,
        );
    }

    for (index, deadline) in spec.deadlines.iter().enumerate() {
        if let Some(ty) = bind(
            &deadline.after,
            &block_scope,
            &format!("/deadlines/{index}/after"),
            ExprSlot::DeadlineAfter(index),
            &mut compiled_exprs,
            &mut errs,
        ) {
            if ty != Ty::Dur {
                errs.push(Finding::err(
                    "def/deadline_type",
                    format!("/deadlines/{index}/after"),
                    format!("deadline after has type {ty}, expected duration"),
                    "return a duration, for example dur(5, min)",
                ));
            }
        }
        check_block(
            &Block {
                sets: deadline.sets.clone(),
                emits: deadline.emits.clone(),
                raises: deadline.raises.clone(),
            },
            &block_scope,
            &format!("/deadlines/{index}"),
            &BlockOwner::Deadline(index),
            &mut compiled_exprs,
            &mut errs,
        );
    }

    let inv_scope = Scope {
        kind: ScopeKind::Invariant,
        ctx: &ctx_tys,
        evt: None,
        enums: &enums,
        states: &state_names,
    };
    for (i, inv) in spec.invariants.iter().enumerate() {
        if let Some(ty) = bind(
            &inv.expr,
            &inv_scope,
            &format!("/invariants/{}", inv.name),
            ExprSlot::Invariant(i),
            &mut compiled_exprs,
            &mut errs,
        ) {
            if ty != Ty::Bool {
                errs.push(Finding::err(
                    "expr/type_mismatch",
                    format!("/invariants/{}", inv.name),
                    format!("invariant has type {ty}, expected bool"),
                    "write a boolean expression",
                ));
            }
        }
    }

    let mut transitions_by: BTreeMap<(String, String), Vec<usize>> = BTreeMap::new();
    for (i, t) in spec.transitions.iter().enumerate() {
        transitions_by
            .entry((t.from.clone(), t.cell_key().to_string()))
            .or_default()
            .push(i);
        // An eventless transition has no `evt` in scope, in its guard or its
        // block; validation already refused any reference (`def/eventless_evt`).
        let evt_tys = t.on.as_ref().and_then(|on| event_map.get(on));
        let empty: BTreeMap<String, Ty> = BTreeMap::new();
        let guard_scope = Scope {
            kind: ScopeKind::Guard,
            ctx: &ctx_tys,
            evt: evt_tys,
            enums: &enums,
            states: &state_names,
        };
        if let Some(g) = &t.guard {
            if let Some(ty) = bind(
                g,
                &guard_scope,
                &format!("/transitions/{i}/if"),
                ExprSlot::TransitionGuard(i),
                &mut compiled_exprs,
                &mut errs,
            ) {
                if ty != Ty::Bool {
                    errs.push(Finding::err(
                        "expr/type_mismatch",
                        format!("/transitions/{i}/if"),
                        format!("guard has type {ty}, expected bool"),
                        "write a boolean expression",
                    ));
                }
            }
        }
        let action_scope = Scope {
            kind: ScopeKind::TransitionAction,
            ctx: &ctx_tys,
            evt: evt_tys,
            enums: &enums,
            states: &state_names,
        };
        let block = Block {
            sets: t.sets.clone(),
            emits: t.emits.clone(),
            raises: t.raises.clone(),
        };
        check_block(
            &block,
            &action_scope,
            &format!("/transitions/{i}"),
            &BlockOwner::Transition(i),
            &mut compiled_exprs,
            &mut errs,
        );
        let _ = empty;
    }

    // SPEC Evaluation: a runtime create, step, deadline poll, or enabled-event
    // scan visits each compiled slot at most once, while lazy operators can
    // only reduce the number of visited nodes. A step can additionally visit
    // at most one omitted guard: its implicit `true` immediately wins global
    // transition selection, so every later candidate is not considered. An
    // enabled-event scan repeats selection independently for every event and
    // can therefore visit one omitted guard per affected event.
    // Bounding that worst-case cost guarantees that a fresh standard budget
    // cannot be exhausted by an operation on a currently accepted definition.
    let compiled_ticks: u64 = compiled_exprs
        .values()
        .map(|compiled| u64::from(crate::expr::ast::node_count(&compiled.expr)))
        .sum();
    let implicit_guard_ticks = spec
        .transitions
        .iter()
        .filter(|transition| transition.guard.is_none())
        .map(TransitionSpec::cell_key)
        .collect::<BTreeSet<_>>()
        .len() as u64;
    let evaluation_ticks = compiled_ticks + implicit_guard_ticks;
    if compatibility == DefinitionCompatibility::Current
        && evaluation_ticks > u64::from(limits::MAX_EVAL_TICKS)
    {
        errs.push(Finding::err(
            "def/limit_eval",
            "/",
            format!(
                "expression evaluation requires {evaluation_ticks} ticks; limit is {}",
                limits::MAX_EVAL_TICKS
            ),
            format!(
                "shorten or remove expressions so compiled AST nodes plus the per-event omitted-guard reserve total at most {}",
                limits::MAX_EVAL_TICKS
            ),
        ));
    }

    if errs.iter().any(|f| f.severity == Severity::Error) {
        return Err(errs);
    }
    let accepted = identity_document(&spec);
    let (canonical, machine_id) = accepted_identity(&accepted);
    let compiled = CompiledMachine {
        machine_id,
        spec,
        canonical,
        transitions_by,
        compiled_exprs,
        compile_warnings: errs,
    };
    // An eventless cycle the machine provably cannot leave never reaches a
    // journal: refusing it here is strictly better than a live workflow
    // discovering it as `run/microstep_limit`. The graph needs the compiled
    // cells and the tree, so it runs after binding; only its refusals count
    // here — its warnings are `analyze_all`'s to report.
    let tree = crate::tree::Tree::for_machine(&compiled.spec);
    let refusals: Vec<Finding> = crate::analyze::eventless_cycle_findings(&compiled, &tree)
        .into_iter()
        .filter(|finding| finding.severity == Severity::Error)
        .collect();
    if !refusals.is_empty() {
        let mut errs = compiled.compile_warnings;
        errs.extend(refusals);
        return Err(errs);
    }
    Ok(compiled)
}
