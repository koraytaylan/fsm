use std::collections::BTreeMap;

use crate::expr::eval::{Budget, Val};
use crate::json::Value;
use crate::machine::{ActiveConfiguration, CompiledMachine, InstanceState, Status};
use crate::spec::TySpec;
use crate::trace::BlockKind;
use crate::tree::Tree;

use super::block::{apply_block, find_node, reject_pipeline};
use super::guard::val_matches;
use super::micro::{EngineSelector, ReactionSelector, invariant_failure, run_to_quiescence};
use super::transition::Transitioned;
use super::validate::reject;
use super::{Applied, ExprSlotOwner, Rejection};

/// Create an instance from a definition, context overrides, and caller time.
///
/// Every region enters its initial chain. Deadlines on those chains are
/// scheduled relative to `now_ms`. Creation is pure; durable hosts must not
/// journal a failed result or consume an instance id or sequence number.
/// Creation is a macrostep like any other: a machine whose initial state has
/// an eventless exit reacts before its first sealed state.
pub fn create(
    m: &CompiledMachine,
    t: &Tree,
    overrides: &BTreeMap<String, Val>,
    now_ms: i64,
) -> Result<Applied, Rejection> {
    create_with(m, t, overrides, now_ms, &mut EngineSelector)
}

/// [`create`] with an explicit reaction selector; tests script the reactions.
pub fn create_with(
    m: &CompiledMachine,
    t: &Tree,
    overrides: &BTreeMap<String, Val>,
    now_ms: i64,
    selector: &mut dyn ReactionSelector,
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
    let mut budget = Budget::new(crate::limits::MACROSTEP_EVAL_TICKS);
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
                        let mut r = reject_pipeline(inner, pipeline, &[]);
                        r.code = "run/create_failed";
                        return Err(r);
                    }
                }
            }
        }
    }
    // Creation's trigger microstep is the initial entry itself: no source, no
    // exits, and the whole entered chain. From here it is an ordinary
    // macrostep.
    let trigger = Transitioned {
        configuration_after,
        context: ctx,
        history_after: BTreeMap::new(),
        effects,
        pipeline,
        candidates: Vec::new(),
        exited: Vec::new(),
        entered,
        internal: false,
        region: None,
        source_state: String::new(),
        public_index: 0,
    };
    let nothing_yet = InstanceState {
        status: Status::Running,
        configuration: trigger.configuration_after.clone(),
        ctx: BTreeMap::new(),
        history: BTreeMap::new(),
        deadlines: BTreeMap::new(),
        pending: Vec::new(),
    };
    run_to_quiescence(m, t, &nothing_yet, trigger, now_ms, &mut budget, selector)
        .map_err(|rejection| create_failed(rejection, m))
}

/// Every creation failure is `run/create_failed`; an invariant failure keeps
/// its identity but tells the caller to fix the inits rather than an action.
fn create_failed(mut rejection: Rejection, m: &CompiledMachine) -> Rejection {
    if rejection.code == "run/invariant" {
        let failure = invariant_failure(&m.spec, &rejection.trace.invariants);
        rejection.message = failure
            .evaluation_error
            .as_ref()
            .map(|error| format!("invariant {}: {}", error.name, error.message))
            .unwrap_or_else(|| "invariant failed at create".into());
        rejection.hint = failure
            .failed_invariant
            .as_ref()
            .map(|name| format!("fix inits or invariant {name}"))
            .unwrap_or_else(|| "fix inits or the invariant".into());
        rejection.source_state = None;
        rejection.transition_idx = None;
    }
    rejection.code = "run/create_failed";
    rejection
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

/// Helper used by tests to parse a payload object of string fields.
pub fn payload_from_pairs(pairs: &[(&str, &str)]) -> Value {
    let mut m = BTreeMap::new();
    for (k, v) in pairs {
        m.insert((*k).into(), Value::Str((*v).into()));
    }
    Value::Obj(m)
}
