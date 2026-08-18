//! The library embedding loop, as a downstream consumer sees it.
//!
//! This crate depends on `fsm-core` and nothing else. It exists so the
//! *library* consumption path is covered by the acceptance criteria alongside
//! the CLI and MCP hosts: an embedder that drives `parse → compile → step` in
//! process and keeps instance state in its own store.
//!
//! Everything here uses only the public API. If a step below ever requires an
//! item that is not exported, this crate fails to build, and that is the signal
//! that `fsm-core` is not actually embeddable for that step.
//!
//! See `docs/EMBEDDING.md` for the prose walkthrough.

use std::collections::BTreeMap;

use fsm_core::analyze::{analyze_all, completeness_matrix};
use fsm_core::expr::eval::{Budget, Val};
use fsm_core::hashes::state_hash;
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::machine::{CompiledMachine, InstanceState};
use fsm_core::replay::{ctx_val_string, parse_ctx_val};
use fsm_core::spec::{Finding, compile_accepted};
use fsm_core::step::{Outcome, create, step};
use fsm_core::tree::Tree;

/// Per-event evaluation budget. The engine is bounded by design; the embedder
/// chooses the ceiling.
pub const EVAL_BUDGET: u32 = 4096;

/// A machine ready to drive: the compiled spec plus its state tree.
///
/// Both are derived from the definition and are cheap to keep alongside it.
pub struct Machine {
    pub compiled: CompiledMachine,
    pub tree: Tree,
}

#[derive(Debug)]
pub enum EmbedError {
    /// The bytes were not JSON the engine accepts.
    Json(String),
    /// The definition compiled to findings — each carries a path and a hint.
    Definition(Vec<Finding>),
    /// Creation or a step was rejected; the code is a namespaced `fsm` code.
    Rejected(String),
    /// A persisted value could not be read back at its declared type.
    Persistence(String),
}

/// Stage 1: bytes to a machine you can step.
pub fn load(spec_json: &[u8]) -> Result<Machine, EmbedError> {
    let def = parse(spec_json, &JsonLimits::DEFAULT).map_err(|e| EmbedError::Json(e.message))?;
    let compiled = compile_accepted(&def).map_err(EmbedError::Definition)?;
    let tree = Tree::build(&compiled.spec.states);
    Ok(Machine { compiled, tree })
}

/// Static findings worth surfacing before anything runs: unreachable states,
/// shadowed transitions, guards that can never hold.
pub fn lint(m: &Machine) -> Vec<Finding> {
    analyze_all(&m.compiled, &m.tree)
}

/// Which (leaf, event) pairs are handled, and how. The embedder's own coverage
/// check hangs off this.
pub fn coverage(m: &Machine) -> BTreeMap<(String, String), String> {
    completeness_matrix(&m.compiled, &m.tree)
}

/// Stage 2: a fresh instance, with optional context overrides.
pub fn start(m: &Machine, overrides: &BTreeMap<String, Val>) -> Result<InstanceState, EmbedError> {
    let a = create(&m.compiled, &m.tree, overrides)
        .map_err(|r| EmbedError::Rejected(r.code.to_string()))?;
    Ok(InstanceState {
        status: a.status_after,
        leaf: a.leaf_after,
        ctx: a.ctx_after,
        history: a.history_after,
        pending: a.effects.iter().map(|e| e.k.to_string()).collect(),
    })
}

/// The outcome of one event, in the form an embedder commits.
pub struct Advance {
    pub next: InstanceState,
    pub exited: Vec<String>,
    pub entered: Vec<String>,
    /// Effects to execute *after* the new state is durable.
    pub effects: Vec<String>,
}

/// Stage 3: one event, one transition. Pure — `st` is untouched, and nothing is
/// committed until the caller persists `Advance::next`.
pub fn advance(
    m: &Machine,
    st: &InstanceState,
    event: &str,
    payload: &Value,
) -> Result<Option<Advance>, EmbedError> {
    let mut budget = Budget::new(EVAL_BUDGET);
    match step(&m.compiled, &m.tree, st, event, payload, &mut budget) {
        Outcome::Applied(a) => {
            let mut pending = st.pending.clone();
            pending.extend(a.effects.iter().map(|e| e.k.to_string()));
            Ok(Some(Advance {
                next: InstanceState {
                    status: a.status_after,
                    leaf: a.leaf_after,
                    ctx: a.ctx_after,
                    history: a.history_after,
                    pending: pending.clone(),
                },
                exited: a.exited,
                entered: a.entered,
                effects: pending,
            }))
        }
        // The machine declares this event ignorable at this state.
        Outcome::Ignored => Ok(None),
        Outcome::Rejected(r) => Err(EmbedError::Rejected(r.code.to_string())),
    }
}

/// A tamper-evident digest of one instance at one sequence number. An embedder
/// stores this next to its own row to detect drift.
pub fn digest(machine_id: &str, instance_id: &str, seq: u64, st: &InstanceState) -> String {
    state_hash(machine_id, instance_id, seq, st)
}

// --- Persistence -------------------------------------------------------------
//
// The embedder owns the store. `fsm-core` owns the *encoding*: `ctx_val_string`
// and `parse_ctx_val` are exact inverses for every declared type, so a
// round-trip through any byte-preserving medium is lossless. Reimplementing
// either half by hand is how stored state silently drifts from the engine's.

/// Encode an instance as plain JSON for the embedder's own store.
pub fn to_row(st: &InstanceState) -> Value {
    let ctx = st
        .ctx
        .iter()
        .map(|(k, v)| (k.clone(), Value::Str(ctx_val_string(v))))
        .collect();
    let history = st
        .history
        .iter()
        .map(|(k, v)| (k.clone(), Value::Str(v.clone())))
        .collect();
    Value::Obj(BTreeMap::from([
        ("status".into(), Value::Str(st.status.as_str().into())),
        ("leaf".into(), Value::Str(st.leaf.clone())),
        ("ctx".into(), Value::Obj(ctx)),
        ("history".into(), Value::Obj(history)),
        (
            "pending".into(),
            Value::Arr(st.pending.iter().cloned().map(Value::Str).collect()),
        ),
    ]))
}

/// Decode a row written by [`to_row`], typed against the machine's declared
/// context. Unknown or ill-typed values are an error, never a default.
pub fn from_row(m: &Machine, row: &Value) -> Result<InstanceState, EmbedError> {
    let miss = |f: &str| EmbedError::Persistence(format!("row missing {f}"));
    let obj = row.as_obj().ok_or_else(|| miss("object"))?;
    let leaf = obj
        .get("leaf")
        .and_then(Value::as_str)
        .ok_or_else(|| miss("leaf"))?
        .to_string();
    let status = match obj.get("status").and_then(Value::as_str) {
        Some("running") => fsm_core::machine::Status::Running,
        Some("completed") => fsm_core::machine::Status::Completed,
        Some("cancelled") => fsm_core::machine::Status::Cancelled,
        _ => return Err(miss("status")),
    };
    let cobj = obj
        .get("ctx")
        .and_then(Value::as_obj)
        .ok_or_else(|| miss("ctx"))?;
    let mut ctx = BTreeMap::new();
    for decl in &m.compiled.spec.context {
        let raw = cobj
            .get(&decl.name)
            .and_then(Value::as_str)
            .ok_or_else(|| EmbedError::Persistence(format!("row missing ctx.{}", decl.name)))?;
        let val = parse_ctx_val(&decl.ty, raw).ok_or_else(|| {
            EmbedError::Persistence(format!("ctx.{} is not a {:?}", decl.name, decl.ty))
        })?;
        ctx.insert(decl.name.clone(), val);
    }
    let history = obj
        .get("history")
        .and_then(Value::as_obj)
        .ok_or_else(|| miss("history"))?
        .iter()
        .filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string())))
        .collect();
    let pending = obj
        .get("pending")
        .and_then(Value::as_arr)
        .ok_or_else(|| miss("pending"))?
        .iter()
        .filter_map(|v| Some(v.as_str()?.to_string()))
        .collect();
    Ok(InstanceState {
        status,
        leaf,
        ctx,
        history,
        pending,
    })
}
