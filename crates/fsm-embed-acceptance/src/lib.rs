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
use fsm_core::hashes::{configuration_value, state_hash};
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::machine::{ActiveConfiguration, CompiledMachine, InstanceState};
use fsm_core::replay::{ctx_val_string, parse_ctx_val};
use fsm_core::spec::{Finding, compile_accepted};
use fsm_core::step::{
    Applied, DeadlineOutcome, Outcome, PendingDeadline, create,
    poll_deadline as core_poll_deadline, step,
};
use fsm_core::tree::Tree;

/// Standard per-operation evaluation budget. Accepted definitions are
/// statically bounded so this fresh budget cannot be exhausted.
pub const EVAL_BUDGET: u32 = fsm_core::limits::MAX_EVAL_TICKS;

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
    let tree = Tree::for_machine(&compiled.spec);
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
///
/// `now_ms` is supplied by the caller and is used only to schedule deadlines
/// on the initial entry chains. The core never consults a clock.
pub fn start(
    m: &Machine,
    overrides: &BTreeMap<String, Val>,
    now_ms: i64,
) -> Result<InstanceState, EmbedError> {
    let a = create(&m.compiled, &m.tree, overrides, now_ms)
        .map_err(|r| EmbedError::Rejected(r.code.to_string()))?;
    Ok(InstanceState {
        status: a.status_after,
        configuration: a.configuration_after,
        ctx: a.ctx_after,
        history: a.history_after,
        deadlines: a.deadlines_after,
        pending: a.effects.iter().map(|e| e.k.to_string()).collect(),
        invocations: BTreeMap::new(),
        signals: BTreeMap::new(),
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

/// The outcome of a pure deadline poll in the form an embedder commits.
pub enum DeadlinePoll {
    /// One due schedule advanced exactly one region.
    Applied {
        /// Deadline selected by due timestamp and document index.
        deadline: PendingDeadline,
        /// State, effects, and entry/exit paths produced by the transition.
        advance: Box<Advance>,
    },
    /// No schedule was due.
    NotDue {
        /// Earliest active schedule, if the configuration has one.
        next: Option<PendingDeadline>,
    },
}

fn advance_from_applied(st: &InstanceState, a: Applied) -> Advance {
    let effects: Vec<String> = a
        .effects
        .iter()
        .map(|effect| effect.k.to_string())
        .collect();
    let mut pending = st.pending.clone();
    pending.extend(effects.iter().cloned());
    Advance {
        next: InstanceState {
            status: a.status_after,
            configuration: a.configuration_after,
            ctx: a.ctx_after,
            history: a.history_after,
            deadlines: a.deadlines_after,
            pending,
            invocations: BTreeMap::new(),
            signals: BTreeMap::new(),
        },
        exited: a.exited,
        entered: a.entered,
        effects,
    }
}

/// Stage 3: one event, one transition. Pure — `st` is untouched, and nothing is
/// committed until the caller persists `Advance::next`.
pub fn advance(
    m: &Machine,
    st: &InstanceState,
    event: &str,
    payload: &Value,
    now_ms: i64,
) -> Result<Option<Advance>, EmbedError> {
    let mut budget = Budget::new(EVAL_BUDGET);
    match step(
        &m.compiled,
        &m.tree,
        st,
        event,
        payload,
        now_ms,
        &mut budget,
    ) {
        Outcome::Applied(a) => Ok(Some(advance_from_applied(st, a))),
        // The machine declares this event ignorable at this state.
        Outcome::Ignored => Ok(None),
        Outcome::Rejected(r) => Err(EmbedError::Rejected(r.code.to_string())),
    }
}

/// Poll at a caller-supplied timestamp and apply at most one due deadline.
///
/// Pure — `st` is untouched. A `NotDue` result is also a pure observation; a
/// durable host decides whether and how to record the poll request.
pub fn poll_deadline(
    m: &Machine,
    st: &InstanceState,
    now_ms: i64,
) -> Result<DeadlinePoll, EmbedError> {
    let mut budget = Budget::new(EVAL_BUDGET);
    match core_poll_deadline(&m.compiled, &m.tree, st, now_ms, &mut budget) {
        DeadlineOutcome::Applied(applied) => Ok(DeadlinePoll::Applied {
            deadline: applied.deadline,
            advance: Box::new(advance_from_applied(st, applied.transition)),
        }),
        DeadlineOutcome::NotDue { next } => Ok(DeadlinePoll::NotDue { next }),
        DeadlineOutcome::Rejected(rejected) => {
            Err(EmbedError::Rejected(rejected.rejection.code.to_string()))
        }
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
    let deadlines = st
        .deadlines
        .iter()
        .map(|(name, due_ms)| (name.clone(), Value::Num(due_ms.to_string())))
        .collect();
    Value::Obj(BTreeMap::from([
        ("status".into(), Value::Str(st.status.as_str().into())),
        (
            "configuration".into(),
            configuration_value(&st.configuration),
        ),
        ("ctx".into(), Value::Obj(ctx)),
        ("history".into(), Value::Obj(history)),
        ("deadlines".into(), Value::Obj(deadlines)),
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
    reject_unknown_fields(
        obj,
        &[
            "status",
            "configuration",
            "ctx",
            "history",
            "deadlines",
            "pending",
        ],
        "row",
    )?;
    let configuration = parse_configuration(
        obj.get("configuration")
            .ok_or_else(|| miss("configuration"))?,
    )?;
    if m.tree.active_leaves(&configuration).is_none() {
        return Err(EmbedError::Persistence(
            "configuration does not match the machine topology".into(),
        ));
    }
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
    if cobj.len() != m.compiled.spec.context.len() {
        return Err(EmbedError::Persistence(
            "row context contains unknown fields".into(),
        ));
    }
    let hobj = obj
        .get("history")
        .and_then(Value::as_obj)
        .ok_or_else(|| miss("history"))?;
    let mut history = BTreeMap::new();
    for (owner, bound) in hobj {
        let bound = bound
            .as_str()
            .ok_or_else(|| EmbedError::Persistence(format!("history.{owner} is not a string")))?;
        history.insert(owner.clone(), bound.to_string());
    }
    let dobj = obj
        .get("deadlines")
        .and_then(Value::as_obj)
        .ok_or_else(|| miss("deadlines"))?;
    let mut deadlines = BTreeMap::new();
    for (name, due_ms) in dobj {
        if !m
            .compiled
            .spec
            .deadlines
            .iter()
            .any(|deadline| deadline.name == *name)
        {
            return Err(EmbedError::Persistence(format!(
                "unknown deadline schedule {name}"
            )));
        }
        let due_ms = due_ms
            .as_num()
            .and_then(|raw| raw.parse::<i64>().ok())
            .ok_or_else(|| {
                EmbedError::Persistence(format!("deadline {name} is not an integer timestamp"))
            })?;
        deadlines.insert(name.clone(), due_ms);
    }
    let parr = obj
        .get("pending")
        .and_then(Value::as_arr)
        .ok_or_else(|| miss("pending"))?;
    let mut pending = Vec::new();
    for (index, value) in parr.iter().enumerate() {
        let effect = value
            .as_str()
            .ok_or_else(|| EmbedError::Persistence(format!("pending[{index}] is not a string")))?;
        pending.push(effect.to_string());
    }
    let state = InstanceState {
        status,
        configuration,
        ctx,
        history,
        deadlines,
        pending,
        invocations: BTreeMap::new(),
        signals: BTreeMap::new(),
    };
    m.tree
        .validate_instance_state(&m.compiled, &state)
        .map_err(|error| EmbedError::Persistence(error.to_string()))?;
    Ok(state)
}

fn parse_configuration(value: &Value) -> Result<ActiveConfiguration, EmbedError> {
    let obj = value
        .as_obj()
        .ok_or_else(|| EmbedError::Persistence("configuration is not an object".to_string()))?;
    match obj.get("kind").and_then(Value::as_str) {
        Some("sequential") => {
            reject_unknown_fields(obj, &["kind", "leaf"], "configuration")?;
            let leaf = obj
                .get("leaf")
                .and_then(Value::as_str)
                .ok_or_else(|| EmbedError::Persistence("configuration missing leaf".to_string()))?
                .to_string();
            Ok(ActiveConfiguration::Sequential { leaf })
        }
        Some("parallel") => {
            reject_unknown_fields(obj, &["kind", "leaves"], "configuration")?;
            let raw_leaves = obj.get("leaves").and_then(Value::as_obj).ok_or_else(|| {
                EmbedError::Persistence("configuration missing leaves".to_string())
            })?;
            let mut leaves = BTreeMap::new();
            for (region, leaf) in raw_leaves {
                let leaf = leaf.as_str().ok_or_else(|| {
                    EmbedError::Persistence(format!(
                        "configuration leaf for region {region} is not a string"
                    ))
                })?;
                leaves.insert(region.clone(), leaf.to_string());
            }
            Ok(ActiveConfiguration::Parallel { leaves })
        }
        Some(kind) => Err(EmbedError::Persistence(format!(
            "unknown configuration kind {kind}"
        ))),
        None => Err(EmbedError::Persistence(
            "configuration missing kind".to_string(),
        )),
    }
}

fn reject_unknown_fields(
    object: &BTreeMap<String, Value>,
    allowed: &[&str],
    scope: &str,
) -> Result<(), EmbedError> {
    if let Some(field) = object
        .keys()
        .find(|field| !allowed.contains(&field.as_str()))
    {
        return Err(EmbedError::Persistence(format!(
            "{scope} contains unknown field {field}"
        )));
    }
    Ok(())
}
