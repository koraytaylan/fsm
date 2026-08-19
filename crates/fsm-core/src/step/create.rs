use std::collections::BTreeMap;

use crate::expr::eval::{Budget, Val};
use crate::json::Value;
use crate::machine::{ActiveConfiguration, CompiledMachine, EnforceMode, Status};
use crate::spec::TySpec;
use crate::trace::{BlockKind, DecisionTrace};
use crate::tree::Tree;

use super::block::{apply_block, eval_invariants, find_node, reject_pipeline};
use super::guard::val_matches;
use super::validate::reject;
use super::{Applied, ExprSlotOwner, Rejection};

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
    let mut deadlines_after = match super::transition::update_deadline_schedules(
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
    let status_after = if super::transition::configuration_is_terminal(m, t, &configuration_after) {
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

/// Helper used by tests to parse a payload object of string fields.
pub fn payload_from_pairs(pairs: &[(&str, &str)]) -> Value {
    let mut m = BTreeMap::new();
    for (k, v) in pairs {
        m.insert((*k).into(), Value::Str((*v).into()));
    }
    Value::Obj(m)
}
