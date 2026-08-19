//! Pure journal fold through the engine.

#![allow(clippy::collapsible_if, unused_imports)]

use std::collections::BTreeMap;

use crate::expr::eval::{Budget, Val};
use crate::hashes::{
    STATE_FORMAT, configuration_value, domain_hash, legacy_state_hash, state_hash,
};
use crate::json::Value;
use crate::machine::{ActiveConfiguration, CompiledMachine, InstanceState, Status};
use crate::record::{Record, RecordKind};
use crate::spec::{TySpec, compile, parse_machine};
use crate::step::{
    DeadlineOutcome, Outcome, PendingDeadline, Rejection, create, poll_deadline, step,
};
use crate::tree::Tree;

/// Format discriminator for newly written logical store-state roots.
pub const STATE_ROOT_FORMAT: &str = "fsm.state-root/3";
/// Hash domain paired with [`STATE_ROOT_FORMAT`].
pub const STATE_ROOT_DOMAIN: &str = "fsm:state-root:3";

/// Hash the complete logical store state at `seq` without the journal hash.
///
/// Omitting `last_hash` avoids a cycle when this root is placed inside the
/// checkpoint record whose hash authenticates it. The checkpoint hash
/// separately binds a snapshot's `last_hash`. The current root commits each
/// instance's tagged active configuration and absolute deadline schedules.
pub fn state_root_at(st: &StoreState, seq: u64) -> String {
    let mut machines = BTreeMap::new();
    for (id, machine) in &st.machines {
        machines.insert(id.clone(), machine.def.clone());
    }

    let mut instances = BTreeMap::new();
    for (id, inst) in &st.instances {
        let mid = st.instance_machines.get(id).cloned().unwrap_or_default();
        let context = inst
            .ctx
            .iter()
            .map(|(name, value)| (name.clone(), Value::Str(ctx_val_string(value))))
            .collect();
        let history = inst
            .history
            .iter()
            .map(|(owner, leaf)| (owner.clone(), Value::Str(leaf.clone())))
            .collect();
        let deadlines = inst
            .deadlines
            .iter()
            .map(|(name, due_ms)| (name.clone(), Value::Num(due_ms.to_string())))
            .collect();
        let body = BTreeMap::from([
            (
                "configuration".into(),
                configuration_value(&inst.configuration),
            ),
            ("status".into(), Value::Str(inst.status.as_str().into())),
            ("machine_id".into(), Value::Str(mid.clone())),
            ("context".into(), Value::Obj(context)),
            ("history".into(), Value::Obj(history)),
            ("deadlines".into(), Value::Obj(deadlines)),
            (
                "pending".into(),
                Value::Arr(inst.pending.iter().cloned().map(Value::Str).collect()),
            ),
            (
                "state_hash".into(),
                Value::Str(state_hash(&mid, id, seq, inst)),
            ),
        ]);
        instances.insert(id.clone(), Value::Obj(body));
    }

    // Only the claiming seq enters the root, not the request fingerprint. The
    // fingerprint lives in the record body that claimed the key, so the hash
    // chain already authenticates it; including it here would change every
    // historical root for no added binding.
    let dedup = st
        .dedup
        .iter()
        .map(|(request_id, slot)| (request_id.clone(), Value::Num(slot.seq.to_string())))
        .collect();
    let material = Value::Obj(BTreeMap::from([
        ("seq".into(), Value::Num(seq.to_string())),
        ("machines".into(), Value::Obj(machines)),
        ("instances".into(), Value::Obj(instances)),
        ("dedup".into(), Value::Obj(dedup)),
    ]));
    format!(
        "sha256:{}",
        crate::sha256::to_hex(&domain_hash(STATE_ROOT_DOMAIN, &material))
    )
}

fn legacy_state_root_at(state: &StoreState, seq: u64) -> String {
    let machines = state
        .machines
        .iter()
        .map(|(identifier, machine)| (identifier.clone(), machine.def.clone()))
        .collect();
    let mut instances = BTreeMap::new();
    for (identifier, instance) in &state.instances {
        let machine_id = state
            .instance_machines
            .get(identifier)
            .cloned()
            .unwrap_or_default();
        let leaf = match &instance.configuration {
            ActiveConfiguration::Sequential { leaf } => leaf.clone(),
            ActiveConfiguration::Parallel { .. } => String::new(),
        };
        let context = instance
            .ctx
            .iter()
            .map(|(name, value)| (name.clone(), Value::Str(ctx_val_string(value))))
            .collect();
        let history = instance
            .history
            .iter()
            .map(|(owner, bound)| (owner.clone(), Value::Str(bound.clone())))
            .collect();
        instances.insert(
            identifier.clone(),
            Value::Obj(BTreeMap::from([
                ("leaf".into(), Value::Str(leaf)),
                ("status".into(), Value::Str(instance.status.as_str().into())),
                ("machine_id".into(), Value::Str(machine_id.clone())),
                ("context".into(), Value::Obj(context)),
                ("history".into(), Value::Obj(history)),
                (
                    "pending".into(),
                    Value::Arr(instance.pending.iter().cloned().map(Value::Str).collect()),
                ),
                (
                    "state_hash".into(),
                    Value::Str(
                        legacy_state_hash(&machine_id, identifier, seq, instance)
                            .unwrap_or_default(),
                    ),
                ),
            ])),
        );
    }
    let dedup = state
        .dedup
        .iter()
        .map(|(request_id, slot)| (request_id.clone(), Value::Num(slot.seq.to_string())))
        .collect();
    let material = Value::Obj(BTreeMap::from([
        ("seq".into(), Value::Num(seq.to_string())),
        ("machines".into(), Value::Obj(machines)),
        ("instances".into(), Value::Obj(instances)),
        ("dedup".into(), Value::Obj(dedup)),
    ]));
    format!(
        "sha256:{}",
        crate::sha256::to_hex(&domain_hash("fsm:state-root:2", &material))
    )
}

fn state_hash_for_record(
    record: &Record,
    machine_id: &str,
    instance_id: &str,
    state: &InstanceState,
) -> Option<String> {
    match record.body.get("state_format").and_then(Value::as_str) {
        Some(STATE_FORMAT) => Some(state_hash(machine_id, instance_id, record.seq, state)),
        None => legacy_state_hash(machine_id, instance_id, record.seq, state),
        Some(_) => None,
    }
}

#[derive(Debug, Clone)]
pub struct StoredMachine {
    pub def: Value,
    pub compiled: CompiledMachine,
    pub tree: Tree,
}

/// One claimed idempotency key.
///
/// `fp` is the [`crate::hashes::request_fp`] of the request that claimed the
/// key. Reusing the key for a request with a different fingerprint is a
/// conflict, not a replay. It is `None` for records written before
/// fingerprints existed (store format ≤ 6), where the original content is
/// unrecoverable and the key can only be replayed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestSlot {
    pub seq: u64,
    pub fp: Option<String>,
}

#[derive(Debug, Clone)]
pub struct StoreState {
    pub machines: BTreeMap<String, StoredMachine>,
    pub instances: BTreeMap<String, InstanceState>,
    pub instance_machines: BTreeMap<String, String>,
    pub dedup: BTreeMap<String, RequestSlot>,
    pub last_seq: u64,
    pub last_hash: String,
}

impl Default for StoreState {
    fn default() -> Self {
        Self {
            machines: BTreeMap::new(),
            instances: BTreeMap::new(),
            instance_machines: BTreeMap::new(),
            dedup: BTreeMap::new(),
            last_seq: 0,
            last_hash: crate::record::zeros(),
        }
    }
}

pub trait RecordSink {
    fn on_record(&mut self, record: &Record, state: &StoreState);
}

pub struct NopSink;

impl RecordSink for NopSink {
    fn on_record(&mut self, _record: &Record, _state: &StoreState) {}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplayError {
    StateHashMismatch {
        seq: u64,
        expected: String,
        found: String,
    },
    FieldMismatch {
        seq: u64,
        field: &'static str,
    },
    UnknownMachine {
        seq: u64,
    },
    UnknownInstance {
        seq: u64,
    },
}

/// Fold a complete journal from genesis through its final record.
///
/// Only this complete-fold path recognizes the exact historical genesis and
/// applies its journal-level compatibility compiler to `machine_defined`
/// records above the current aggregate expression ceiling and to the narrowly
/// preserved legacy malformed history shapes.
pub fn fold_with(
    records: impl IntoIterator<Item = Record>,
    sink: &mut impl RecordSink,
) -> Result<StoreState, ReplayError> {
    fold_records(
        StoreState::default(),
        records,
        sink,
        GenesisDefinitionLimits::DetectHistorical,
    )
}

/// Fold a current-format journal tail onto an already-authenticated state.
///
/// Tail records never gain historical definition authority: a new
/// `machine_defined` record is compiled under the current aggregate ceiling.
pub fn fold_from(
    st: StoreState,
    records: impl IntoIterator<Item = Record>,
    sink: &mut impl RecordSink,
) -> Result<StoreState, ReplayError> {
    // A tail fold starts from an already-validated state or snapshot. Any new
    // MachineDefined record in that tail was admitted by the current writer.
    fold_records(st, records, sink, GenesisDefinitionLimits::CurrentOnly)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GenesisDefinitionLimits {
    DetectHistorical,
    CurrentOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DefinitionCompileMode {
    Current,
    HistoricalPersistence,
}

fn fold_records(
    mut state: StoreState,
    records: impl IntoIterator<Item = Record>,
    sink: &mut impl RecordSink,
    genesis_definition_limits: GenesisDefinitionLimits,
) -> Result<StoreState, ReplayError> {
    let mut compile_mode = DefinitionCompileMode::Current;
    for rec in records {
        if genesis_definition_limits == GenesisDefinitionLimits::DetectHistorical
            && rec.seq == 0
            && rec.kind == RecordKind::Genesis
            && crate::record::genesis_uses_historical_definition_limits(&rec.body)
        {
            compile_mode = DefinitionCompileMode::HistoricalPersistence;
        }
        apply(&mut state, &rec, compile_mode)?;
        state.last_seq = rec.seq;
        state.last_hash = rec.hash.clone();
        sink.on_record(&rec, &state);
    }
    Ok(state)
}

/// Serialize a context value in the **persistence form**: always a string.
///
/// This is the exact inverse of [`parse_ctx_val`] for every declared type, and
/// it is the form the engine's own snapshots and `state_root` hashing use. An
/// embedder persisting [`InstanceState`] in its own store wants this pair.
pub fn ctx_val_string(v: &Val) -> String {
    v.canonical_string()
}

/// Read a context value back from its [`ctx_val_string`] form.
///
/// Returns `None` when `raw` does not denote a value of the declared type.
pub fn parse_ctx_val(ty: &TySpec, raw: &str) -> Option<Val> {
    parse_override(ty, raw)
}

/// Serialize a context value in the **API form**: booleans become JSON
/// booleans, every other type becomes its canonical string.
///
/// This is the shape that appears in tool responses and CLI output. It is
/// deliberately *not* the inverse of [`parse_ctx_val`], which reads strings
/// only — read this form back with [`parse_ctx_json`], or persist with the
/// [`ctx_val_string`]/[`parse_ctx_val`] pair instead.
pub fn ctx_val_json(v: &Val) -> Value {
    match v {
        Val::Bool(b) => Value::Bool(*b),
        other => Value::Str(other.canonical_string()),
    }
}

/// Read a context value back from its [`ctx_val_json`] form.
///
/// Accepts the JSON boolean that [`ctx_val_json`] emits for [`TySpec::Bool`],
/// and otherwise defers to [`parse_ctx_val`]. Returns `None` when `v` does not
/// denote a value of the declared type.
pub fn parse_ctx_json(ty: &TySpec, v: &Value) -> Option<Val> {
    match (ty, v) {
        (TySpec::Bool, Value::Bool(b)) => Some(Val::Bool(*b)),
        (_, Value::Str(s)) => parse_ctx_val(ty, s),
        _ => None,
    }
}

fn parse_override(ty: &TySpec, raw: &str) -> Option<Val> {
    match ty {
        TySpec::Int => raw.parse().ok().map(Val::Int),
        TySpec::Bool => match raw {
            "true" => Some(Val::Bool(true)),
            "false" => Some(Val::Bool(false)),
            _ => None,
        },
        TySpec::Str => Some(Val::Str(raw.into())),
        TySpec::Ts => raw.parse().ok().map(Val::Ts),
        TySpec::Dur => raw.parse().ok().map(Val::Dur),
        TySpec::Dec { scale } => crate::decimal::Dec::parse(raw, *scale).ok().map(Val::Dec),
        // `canonical_string` writes enums qualified (`tier.premium`), so the
        // inverse must strip the type prefix — parsing the whole string as the
        // variant re-qualifies it on every round-trip and silently drifts the
        // value. The bare form is accepted too, since that is what a caller
        // supplying an override by hand writes. Identifiers cannot contain a
        // dot, so the split is unambiguous.
        TySpec::Enum { of } => {
            let variant = match raw.split_once('.') {
                Some((ty, v)) if ty == of => v,
                Some(_) => return None,
                None => raw,
            };
            Some(Val::Enum {
                ty: of.clone(),
                variant: variant.into(),
            })
        }
    }
}

fn overrides_from(
    ctx: &[crate::spec::CtxVar],
    raw: Option<&Value>,
) -> Option<BTreeMap<String, Val>> {
    let Some(v) = raw else {
        return Some(BTreeMap::new());
    };
    let obj = v.as_obj()?;
    let mut out = BTreeMap::new();
    for (k, val) in obj {
        let decl = ctx.iter().find(|c| c.name == *k)?;
        let s = val.as_str()?;
        out.insert(k.clone(), parse_override(&decl.ty, s)?);
    }
    Some(out)
}

fn claim_request_id(st: &mut StoreState, rec: &Record) -> Result<(), ReplayError> {
    if let Some(rid) = rec.body.get("request_id").and_then(Value::as_str) {
        if st.dedup.contains_key(rid) {
            return Err(ReplayError::FieldMismatch {
                seq: rec.seq,
                field: "request_id",
            });
        }
        let fp = rec
            .body
            .get("request_fp")
            .and_then(Value::as_str)
            .map(str::to_string);
        st.dedup
            .insert(rid.into(), RequestSlot { seq: rec.seq, fp });
    }
    Ok(())
}

fn apply(
    st: &mut StoreState,
    rec: &Record,
    compile_mode: DefinitionCompileMode,
) -> Result<(), ReplayError> {
    let applied = match rec.kind {
        RecordKind::Genesis => Ok(()),
        RecordKind::MachineDefined => {
            let def = rec
                .body
                .get("def")
                .cloned()
                .ok_or(ReplayError::UnknownMachine { seq: rec.seq })?;
            let compiled = match compile_mode {
                DefinitionCompileMode::Current => crate::spec::compile_accepted(&def),
                DefinitionCompileMode::HistoricalPersistence => {
                    crate::spec::compile_accepted_historical_unchecked(&def)
                }
            }
            .map_err(|_| ReplayError::UnknownMachine { seq: rec.seq })?;
            let tree = Tree::for_machine(&compiled.spec);
            let id = rec
                .body
                .get("machine_id")
                .and_then(Value::as_str)
                .unwrap_or(&compiled.machine_id)
                .to_string();
            if id != compiled.machine_id {
                return Err(ReplayError::FieldMismatch {
                    seq: rec.seq,
                    field: "machine_id",
                });
            }
            st.machines.insert(
                id,
                StoredMachine {
                    def,
                    compiled,
                    tree,
                },
            );
            Ok(())
        }
        RecordKind::InstanceCreated => {
            let mid = rec
                .body
                .get("machine_id")
                .and_then(Value::as_str)
                .ok_or(ReplayError::UnknownMachine { seq: rec.seq })?;
            let iid = rec
                .body
                .get("instance_id")
                .and_then(Value::as_str)
                .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
            let m = st
                .machines
                .get(mid)
                .ok_or(ReplayError::UnknownMachine { seq: rec.seq })?;
            let overrides =
                match overrides_from(&m.compiled.spec.context, rec.body.get("overrides")) {
                    Some(o) => o,
                    None => {
                        return Err(ReplayError::FieldMismatch {
                            seq: rec.seq,
                            field: "overrides",
                        });
                    }
                };
            let a = create(&m.compiled, &m.tree, &overrides, rec.ts)
                .map_err(|_| ReplayError::UnknownInstance { seq: rec.seq })?;
            let inst = InstanceState {
                status: a.status_after,
                configuration: a.configuration_after,
                ctx: a.ctx_after,
                history: a.history_after,
                deadlines: a.deadlines_after,
                pending: a
                    .effects
                    .iter()
                    .map(|e| format!("{iid}/0/{}", e.k))
                    .collect(),
            };
            if let Some(want) = rec.body.get("state_hash").and_then(Value::as_str) {
                let got = state_hash_for_record(rec, mid, iid, &inst).ok_or(
                    ReplayError::FieldMismatch {
                        seq: rec.seq,
                        field: "state_format",
                    },
                )?;
                if got != want {
                    return Err(ReplayError::StateHashMismatch {
                        seq: rec.seq,
                        expected: want.into(),
                        found: got,
                    });
                }
            }
            if let Some(want) = rec.body.get("leaf").and_then(Value::as_str) {
                if inst.configuration.leaf(None) != Some(want) {
                    return Err(ReplayError::FieldMismatch {
                        seq: rec.seq,
                        field: "leaf",
                    });
                }
            }
            if let Some(want) = rec.body.get("configuration") {
                if want != &configuration_value(&inst.configuration) {
                    return Err(ReplayError::FieldMismatch {
                        seq: rec.seq,
                        field: "configuration",
                    });
                }
            }
            st.instances.insert(iid.into(), inst);
            st.instance_machines.insert(iid.into(), mid.into());
            claim_request_id(st, rec)?;
            Ok(())
        }
        RecordKind::EventApplied => {
            let iid = rec
                .body
                .get("instance_id")
                .and_then(Value::as_str)
                .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
            let ev = rec.body.get("event").and_then(Value::as_str).unwrap_or("");
            let payload = rec
                .body
                .get("payload")
                .cloned()
                .unwrap_or(Value::Obj(BTreeMap::new()));
            let mid = st
                .instance_machines
                .get(iid)
                .cloned()
                .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
            let m = st
                .machines
                .get(&mid)
                .ok_or(ReplayError::UnknownMachine { seq: rec.seq })?;
            let inst = st
                .instances
                .get(iid)
                .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?
                .clone();
            let mut bud = Budget::new(crate::limits::MAX_EVAL_TICKS);
            match step(&m.compiled, &m.tree, &inst, ev, &payload, rec.ts, &mut bud) {
                Outcome::Applied(a) => {
                    let want = rec.body.get("exited").and_then(Value::as_arr).ok_or(
                        ReplayError::FieldMismatch {
                            seq: rec.seq,
                            field: "exited",
                        },
                    )?;
                    let got: Vec<_> = a.exited.iter().map(|s| Value::Str(s.clone())).collect();
                    if got != *want {
                        return Err(ReplayError::FieldMismatch {
                            seq: rec.seq,
                            field: "exited",
                        });
                    }
                    let want = rec.body.get("entered").and_then(Value::as_arr).ok_or(
                        ReplayError::FieldMismatch {
                            seq: rec.seq,
                            field: "entered",
                        },
                    )?;
                    let got: Vec<_> = a.entered.iter().map(|s| Value::Str(s.clone())).collect();
                    if got != *want {
                        return Err(ReplayError::FieldMismatch {
                            seq: rec.seq,
                            field: "entered",
                        });
                    }
                    let want = rec.body.get("source_state").and_then(Value::as_str).ok_or(
                        ReplayError::FieldMismatch {
                            seq: rec.seq,
                            field: "source_state",
                        },
                    )?;
                    if want != a.source_state {
                        return Err(ReplayError::FieldMismatch {
                            seq: rec.seq,
                            field: "source_state",
                        });
                    }
                    let mut pending = inst.pending.clone();
                    pending.extend(
                        a.effects
                            .iter()
                            .map(|e| format!("{iid}/{}/{}", rec.seq, e.k)),
                    );
                    let new = InstanceState {
                        status: a.status_after,
                        configuration: a.configuration_after,
                        ctx: a.ctx_after,
                        history: a.history_after,
                        deadlines: a.deadlines_after,
                        pending,
                    };
                    let want = rec.body.get("state_hash").and_then(Value::as_str).ok_or(
                        ReplayError::FieldMismatch {
                            seq: rec.seq,
                            field: "state_hash",
                        },
                    )?;
                    let got = state_hash_for_record(rec, &mid, iid, &new).ok_or(
                        ReplayError::FieldMismatch {
                            seq: rec.seq,
                            field: "state_format",
                        },
                    )?;
                    if got != want {
                        return Err(ReplayError::StateHashMismatch {
                            seq: rec.seq,
                            expected: want.into(),
                            found: got,
                        });
                    }
                    st.instances.insert(iid.into(), new);
                }
                _ => {
                    return Err(ReplayError::FieldMismatch {
                        seq: rec.seq,
                        field: "outcome",
                    });
                }
            }
            claim_request_id(st, rec)?;
            Ok(())
        }
        RecordKind::EventRejected | RecordKind::EventIgnored => {
            let iid = rec
                .body
                .get("instance_id")
                .and_then(Value::as_str)
                .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
            let ev = rec.body.get("event").and_then(Value::as_str).unwrap_or("");
            let payload = rec
                .body
                .get("payload")
                .cloned()
                .unwrap_or(Value::Obj(BTreeMap::new()));
            let mid = st
                .instance_machines
                .get(iid)
                .cloned()
                .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
            let m = st
                .machines
                .get(&mid)
                .ok_or(ReplayError::UnknownMachine { seq: rec.seq })?;
            let inst = st
                .instances
                .get(iid)
                .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
            let want = rec.body.get("state_hash").and_then(Value::as_str).ok_or(
                ReplayError::FieldMismatch {
                    seq: rec.seq,
                    field: "state_hash",
                },
            )?;
            let got =
                state_hash_for_record(rec, &mid, iid, inst).ok_or(ReplayError::FieldMismatch {
                    seq: rec.seq,
                    field: "state_format",
                })?;
            if got != want {
                return Err(ReplayError::StateHashMismatch {
                    seq: rec.seq,
                    expected: want.into(),
                    found: got,
                });
            }
            let mut bud = Budget::new(crate::limits::MAX_EVAL_TICKS);
            let out = step(&m.compiled, &m.tree, inst, ev, &payload, rec.ts, &mut bud);
            match (rec.kind, &out) {
                (RecordKind::EventRejected, Outcome::Rejected(r)) => {
                    let code = rec.body.get("code").and_then(Value::as_str).ok_or(
                        ReplayError::FieldMismatch {
                            seq: rec.seq,
                            field: "code",
                        },
                    )?;
                    if code != r.code {
                        return Err(ReplayError::FieldMismatch {
                            seq: rec.seq,
                            field: "code",
                        });
                    }
                    let msg = rec.body.get("message").and_then(Value::as_str).ok_or(
                        ReplayError::FieldMismatch {
                            seq: rec.seq,
                            field: "message",
                        },
                    )?;
                    if msg != r.message {
                        return Err(ReplayError::FieldMismatch {
                            seq: rec.seq,
                            field: "message",
                        });
                    }
                    let hint = rec.body.get("hint").and_then(Value::as_str).ok_or(
                        ReplayError::FieldMismatch {
                            seq: rec.seq,
                            field: "hint",
                        },
                    )?;
                    if hint != r.hint {
                        return Err(ReplayError::FieldMismatch {
                            seq: rec.seq,
                            field: "hint",
                        });
                    }
                    let details = rec.body.get("details").and_then(Value::as_obj).ok_or(
                        ReplayError::FieldMismatch {
                            seq: rec.seq,
                            field: "details",
                        },
                    )?;
                    let mut bud2 = Budget::new(crate::limits::MAX_EVAL_TICKS);
                    let evs = crate::analyze::enabled_events(&m.compiled, &m.tree, inst, &mut bud2);
                    let rid = rec.body.get("request_id").and_then(Value::as_str);
                    let want = expected_event_rejected_details(r, rid, enabled_reports_value(&evs));
                    let historical_match = if details != &want
                        && compile_mode == DefinitionCompileMode::HistoricalPersistence
                    {
                        let mut historical_budget = Budget::new(crate::limits::MAX_EVAL_TICKS);
                        let historical_events = crate::analyze::enabled_events_historical(
                            &m.compiled,
                            &m.tree,
                            inst,
                            &mut historical_budget,
                        );
                        let historical = expected_event_rejected_details(
                            r,
                            rid,
                            enabled_reports_value(&historical_events),
                        );
                        details == &historical
                    } else {
                        false
                    };
                    if details != &want && !historical_match {
                        return Err(ReplayError::FieldMismatch {
                            seq: rec.seq,
                            field: "details",
                        });
                    }
                    match (rec.body.get("span"), r.span) {
                        (None, None) => {}
                        (Some(Value::Obj(o)), Some((s, e))) => {
                            if o.get("start").and_then(Value::as_num) != Some(&s.to_string())
                                || o.get("end").and_then(Value::as_num) != Some(&e.to_string())
                            {
                                return Err(ReplayError::FieldMismatch {
                                    seq: rec.seq,
                                    field: "span",
                                });
                            }
                        }
                        _ => {
                            return Err(ReplayError::FieldMismatch {
                                seq: rec.seq,
                                field: "span",
                            });
                        }
                    }
                }
                (RecordKind::EventIgnored, Outcome::Ignored) => {}
                _ => {
                    return Err(ReplayError::FieldMismatch {
                        seq: rec.seq,
                        field: "outcome",
                    });
                }
            }
            claim_request_id(st, rec)?;
            Ok(())
        }
        RecordKind::DeadlineApplied => {
            let iid = rec
                .body
                .get("instance_id")
                .and_then(Value::as_str)
                .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
            let mid = st
                .instance_machines
                .get(iid)
                .cloned()
                .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
            let machine = st
                .machines
                .get(&mid)
                .ok_or(ReplayError::UnknownMachine { seq: rec.seq })?;
            let instance = st
                .instances
                .get(iid)
                .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?
                .clone();
            let expected_deadline = record_deadline(rec)?;
            let mut budget = Budget::new(crate::limits::MAX_EVAL_TICKS);
            match poll_deadline(
                &machine.compiled,
                &machine.tree,
                &instance,
                rec.ts,
                &mut budget,
            ) {
                DeadlineOutcome::Applied(applied) => {
                    verify_deadline(rec, &expected_deadline, &applied.deadline, false)?;
                    verify_deadline_transition(rec, &applied.transition)?;
                    let mut pending = instance.pending.clone();
                    pending.extend(
                        applied
                            .transition
                            .effects
                            .iter()
                            .map(|effect| format!("{iid}/{}/{}", rec.seq, effect.k)),
                    );
                    let new = InstanceState {
                        status: applied.transition.status_after,
                        configuration: applied.transition.configuration_after,
                        ctx: applied.transition.ctx_after,
                        history: applied.transition.history_after,
                        deadlines: applied.transition.deadlines_after,
                        pending,
                    };
                    verify_record_state_hash(rec, &mid, iid, &new)?;
                    st.instances.insert(iid.into(), new);
                }
                _ => {
                    return Err(ReplayError::FieldMismatch {
                        seq: rec.seq,
                        field: "outcome",
                    });
                }
            }
            claim_request_id(st, rec)?;
            Ok(())
        }
        RecordKind::DeadlineRejected => {
            let iid = rec
                .body
                .get("instance_id")
                .and_then(Value::as_str)
                .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
            let mid = st
                .instance_machines
                .get(iid)
                .cloned()
                .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
            let machine = st
                .machines
                .get(&mid)
                .ok_or(ReplayError::UnknownMachine { seq: rec.seq })?;
            let instance = st
                .instances
                .get(iid)
                .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
            verify_record_state_hash(rec, &mid, iid, instance)?;
            let expected_deadline = record_deadline(rec)?;
            let mut budget = Budget::new(crate::limits::MAX_EVAL_TICKS);
            match poll_deadline(
                &machine.compiled,
                &machine.tree,
                instance,
                rec.ts,
                &mut budget,
            ) {
                DeadlineOutcome::Rejected(rejected) => {
                    let selected =
                        rejected
                            .deadline
                            .as_ref()
                            .ok_or(ReplayError::FieldMismatch {
                                seq: rec.seq,
                                field: "deadline",
                            })?;
                    verify_deadline(rec, &expected_deadline, selected, false)?;
                    let request_id = rec.body.get("request_id").and_then(Value::as_str);
                    let details =
                        expected_deadline_rejected_details(&rejected.rejection, request_id);
                    verify_rejection(rec, &rejected.rejection, &details)?;
                }
                _ => {
                    return Err(ReplayError::FieldMismatch {
                        seq: rec.seq,
                        field: "outcome",
                    });
                }
            }
            claim_request_id(st, rec)?;
            Ok(())
        }
        RecordKind::DeadlineNotDue => {
            let iid = rec
                .body
                .get("instance_id")
                .and_then(Value::as_str)
                .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
            let mid = st
                .instance_machines
                .get(iid)
                .cloned()
                .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
            let machine = st
                .machines
                .get(&mid)
                .ok_or(ReplayError::UnknownMachine { seq: rec.seq })?;
            let instance = st
                .instances
                .get(iid)
                .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
            verify_record_state_hash(rec, &mid, iid, instance)?;
            let expected_next = record_next_deadline(rec)?;
            let mut budget = Budget::new(crate::limits::MAX_EVAL_TICKS);
            match poll_deadline(
                &machine.compiled,
                &machine.tree,
                instance,
                rec.ts,
                &mut budget,
            ) {
                DeadlineOutcome::NotDue { next } => match (&expected_next, &next) {
                    (None, None) => {}
                    (Some(expected), Some(actual)) => {
                        verify_deadline(rec, expected, actual, true)?;
                    }
                    _ => {
                        return Err(ReplayError::FieldMismatch {
                            seq: rec.seq,
                            field: "next_deadline",
                        });
                    }
                },
                _ => {
                    return Err(ReplayError::FieldMismatch {
                        seq: rec.seq,
                        field: "outcome",
                    });
                }
            }
            claim_request_id(st, rec)?;
            Ok(())
        }
        RecordKind::EffectAcked => {
            let iid = rec
                .body
                .get("instance_id")
                .and_then(Value::as_str)
                .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
            let eid = rec.body.get("effect_id").and_then(Value::as_str).ok_or(
                ReplayError::FieldMismatch {
                    seq: rec.seq,
                    field: "effect_id",
                },
            )?;
            let inst = st
                .instances
                .get_mut(iid)
                .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
            if !inst.pending.iter().any(|p| p == eid) {
                return Err(ReplayError::FieldMismatch {
                    seq: rec.seq,
                    field: "effect_id",
                });
            }
            inst.pending.retain(|p| p != eid);
            let mid = st
                .instance_machines
                .get(iid)
                .cloned()
                .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
            let inst = st
                .instances
                .get(iid)
                .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
            let want = rec.body.get("state_hash").and_then(Value::as_str).ok_or(
                ReplayError::FieldMismatch {
                    seq: rec.seq,
                    field: "state_hash",
                },
            )?;
            let got =
                state_hash_for_record(rec, &mid, iid, inst).ok_or(ReplayError::FieldMismatch {
                    seq: rec.seq,
                    field: "state_format",
                })?;
            if got != want {
                return Err(ReplayError::StateHashMismatch {
                    seq: rec.seq,
                    expected: want.into(),
                    found: got,
                });
            }
            claim_request_id(st, rec)?;
            Ok(())
        }
        RecordKind::RequestRejected => {
            let iid = rec
                .body
                .get("instance_id")
                .and_then(Value::as_str)
                .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
            let inst = st
                .instances
                .get(iid)
                .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
            let mid = st
                .instance_machines
                .get(iid)
                .cloned()
                .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
            let want = rec.body.get("state_hash").and_then(Value::as_str).ok_or(
                ReplayError::FieldMismatch {
                    seq: rec.seq,
                    field: "state_hash",
                },
            )?;
            let got =
                state_hash_for_record(rec, &mid, iid, inst).ok_or(ReplayError::FieldMismatch {
                    seq: rec.seq,
                    field: "state_format",
                })?;
            if got != want {
                return Err(ReplayError::StateHashMismatch {
                    seq: rec.seq,
                    expected: want.into(),
                    found: got,
                });
            }
            match rec.body.get("operation").and_then(Value::as_str) {
                Some("ack") => {
                    let eid = rec.body.get("effect_id").and_then(Value::as_str).ok_or(
                        ReplayError::FieldMismatch {
                            seq: rec.seq,
                            field: "effect_id",
                        },
                    )?;
                    if inst.pending.iter().any(|p| p == eid) {
                        return Err(ReplayError::FieldMismatch {
                            seq: rec.seq,
                            field: "effect_id",
                        });
                    }
                    if rec.body.get("code").and_then(Value::as_str) != Some("req/field_unknown") {
                        return Err(ReplayError::FieldMismatch {
                            seq: rec.seq,
                            field: "code",
                        });
                    }
                    if rec.body.get("message").and_then(Value::as_str) != Some("unknown effect id")
                    {
                        return Err(ReplayError::FieldMismatch {
                            seq: rec.seq,
                            field: "message",
                        });
                    }
                    if rec.body.get("hint").and_then(Value::as_str)
                        != Some("use an id from effects_pending")
                    {
                        return Err(ReplayError::FieldMismatch {
                            seq: rec.seq,
                            field: "hint",
                        });
                    }
                    if rec.body.get("span").is_some() {
                        return Err(ReplayError::FieldMismatch {
                            seq: rec.seq,
                            field: "span",
                        });
                    }
                    let details = rec.body.get("details").and_then(Value::as_obj).ok_or(
                        ReplayError::FieldMismatch {
                            seq: rec.seq,
                            field: "details",
                        },
                    )?;
                    let rid = rec.body.get("request_id").and_then(Value::as_str);
                    let want = expected_request_rejected_details(rid, &inst.pending);
                    if details != &want {
                        return Err(ReplayError::FieldMismatch {
                            seq: rec.seq,
                            field: "details",
                        });
                    }
                }
                Some("poll_deadline") => {
                    let machine = st
                        .machines
                        .get(&mid)
                        .ok_or(ReplayError::UnknownMachine { seq: rec.seq })?;
                    let mut budget = Budget::new(crate::limits::MAX_EVAL_TICKS);
                    match poll_deadline(&machine.compiled, &machine.tree, inst, rec.ts, &mut budget)
                    {
                        DeadlineOutcome::Rejected(rejected) if rejected.deadline.is_none() => {
                            let request_id = rec.body.get("request_id").and_then(Value::as_str);
                            let details =
                                expected_deadline_rejected_details(&rejected.rejection, request_id);
                            verify_rejection(rec, &rejected.rejection, &details)?;
                        }
                        _ => {
                            return Err(ReplayError::FieldMismatch {
                                seq: rec.seq,
                                field: "outcome",
                            });
                        }
                    }
                }
                _ => {
                    return Err(ReplayError::FieldMismatch {
                        seq: rec.seq,
                        field: "operation",
                    });
                }
            }
            claim_request_id(st, rec)?;
            Ok(())
        }
        RecordKind::InstanceCancelled => {
            let iid = rec
                .body
                .get("instance_id")
                .and_then(Value::as_str)
                .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
            let inst = st
                .instances
                .get_mut(iid)
                .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
            inst.status = Status::Cancelled;
            inst.deadlines.clear();
            let mid = st
                .instance_machines
                .get(iid)
                .cloned()
                .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
            let inst = st
                .instances
                .get(iid)
                .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
            let want = rec.body.get("state_hash").and_then(Value::as_str).ok_or(
                ReplayError::FieldMismatch {
                    seq: rec.seq,
                    field: "state_hash",
                },
            )?;
            let got =
                state_hash_for_record(rec, &mid, iid, inst).ok_or(ReplayError::FieldMismatch {
                    seq: rec.seq,
                    field: "state_format",
                })?;
            if got != want {
                return Err(ReplayError::StateHashMismatch {
                    seq: rec.seq,
                    expected: want.into(),
                    found: got,
                });
            }
            claim_request_id(st, rec)?;
            Ok(())
        }
        RecordKind::Annotated => {
            let iid = rec.body.get("instance_id").and_then(Value::as_str).ok_or(
                ReplayError::FieldMismatch {
                    seq: rec.seq,
                    field: "instance_id",
                },
            )?;
            if !st.instances.contains_key(iid) {
                return Err(ReplayError::UnknownInstance { seq: rec.seq });
            }
            if rec.body.get("note").and_then(Value::as_str).is_none() {
                return Err(ReplayError::FieldMismatch {
                    seq: rec.seq,
                    field: "note",
                });
            }
            claim_request_id(st, rec)?;
            Ok(())
        }
        RecordKind::StateCheckpoint => Ok(()),
    };
    applied?;
    if let Some(root) = rec.body.get("state_root") {
        let want = root.as_str().ok_or(ReplayError::FieldMismatch {
            seq: rec.seq,
            field: "state_root",
        })?;
        let found = match rec.body.get("state_root_format").and_then(Value::as_str) {
            Some(STATE_ROOT_FORMAT) => state_root_at(st, rec.seq),
            None => legacy_state_root_at(st, rec.seq),
            Some(_) => {
                return Err(ReplayError::FieldMismatch {
                    seq: rec.seq,
                    field: "state_root_format",
                });
            }
        };
        if want != found {
            return Err(ReplayError::FieldMismatch {
                seq: rec.seq,
                field: "state_root",
            });
        }
    }
    Ok(())
}

fn record_deadline(record: &Record) -> Result<PendingDeadline, ReplayError> {
    let name =
        record
            .body
            .get("deadline")
            .and_then(Value::as_str)
            .ok_or(ReplayError::FieldMismatch {
                seq: record.seq,
                field: "deadline",
            })?;
    let deadline_idx = record
        .body
        .get("deadline_idx")
        .and_then(Value::as_num)
        .and_then(|raw| raw.parse().ok())
        .ok_or(ReplayError::FieldMismatch {
            seq: record.seq,
            field: "deadline_idx",
        })?;
    let due_ms = record
        .body
        .get("due_ms")
        .and_then(Value::as_num)
        .and_then(|raw| raw.parse().ok())
        .ok_or(ReplayError::FieldMismatch {
            seq: record.seq,
            field: "due_ms",
        })?;
    Ok(PendingDeadline {
        name: name.into(),
        deadline_idx,
        due_ms,
    })
}

fn record_next_deadline(record: &Record) -> Result<Option<PendingDeadline>, ReplayError> {
    let Some(name) = record.body.get("next_deadline") else {
        if record.body.get("next_deadline_idx").is_some()
            || record.body.get("next_due_ms").is_some()
        {
            return Err(ReplayError::FieldMismatch {
                seq: record.seq,
                field: "next_deadline",
            });
        }
        return Ok(None);
    };
    let name = name.as_str().ok_or(ReplayError::FieldMismatch {
        seq: record.seq,
        field: "next_deadline",
    })?;
    let deadline_idx = record
        .body
        .get("next_deadline_idx")
        .and_then(Value::as_num)
        .and_then(|raw| raw.parse().ok())
        .ok_or(ReplayError::FieldMismatch {
            seq: record.seq,
            field: "next_deadline_idx",
        })?;
    let due_ms = record
        .body
        .get("next_due_ms")
        .and_then(Value::as_num)
        .and_then(|raw| raw.parse().ok())
        .ok_or(ReplayError::FieldMismatch {
            seq: record.seq,
            field: "next_due_ms",
        })?;
    Ok(Some(PendingDeadline {
        name: name.into(),
        deadline_idx,
        due_ms,
    }))
}

fn verify_deadline(
    record: &Record,
    expected: &PendingDeadline,
    actual: &PendingDeadline,
    next: bool,
) -> Result<(), ReplayError> {
    if expected.name != actual.name {
        return Err(ReplayError::FieldMismatch {
            seq: record.seq,
            field: if next { "next_deadline" } else { "deadline" },
        });
    }
    if expected.deadline_idx != actual.deadline_idx {
        return Err(ReplayError::FieldMismatch {
            seq: record.seq,
            field: if next {
                "next_deadline_idx"
            } else {
                "deadline_idx"
            },
        });
    }
    if expected.due_ms != actual.due_ms {
        return Err(ReplayError::FieldMismatch {
            seq: record.seq,
            field: if next { "next_due_ms" } else { "due_ms" },
        });
    }
    Ok(())
}

fn verify_deadline_transition(
    record: &Record,
    applied: &crate::step::Applied,
) -> Result<(), ReplayError> {
    let exited =
        record
            .body
            .get("exited")
            .and_then(Value::as_arr)
            .ok_or(ReplayError::FieldMismatch {
                seq: record.seq,
                field: "exited",
            })?;
    let actual_exited: Vec<_> = applied.exited.iter().cloned().map(Value::Str).collect();
    if exited != actual_exited {
        return Err(ReplayError::FieldMismatch {
            seq: record.seq,
            field: "exited",
        });
    }

    let entered =
        record
            .body
            .get("entered")
            .and_then(Value::as_arr)
            .ok_or(ReplayError::FieldMismatch {
                seq: record.seq,
                field: "entered",
            })?;
    let actual_entered: Vec<_> = applied.entered.iter().cloned().map(Value::Str).collect();
    if entered != actual_entered {
        return Err(ReplayError::FieldMismatch {
            seq: record.seq,
            field: "entered",
        });
    }

    if record.body.get("source_state").and_then(Value::as_str)
        != Some(applied.source_state.as_str())
    {
        return Err(ReplayError::FieldMismatch {
            seq: record.seq,
            field: "source_state",
        });
    }
    Ok(())
}

fn verify_record_state_hash(
    record: &Record,
    machine_id: &str,
    instance_id: &str,
    state: &InstanceState,
) -> Result<(), ReplayError> {
    let expected = record
        .body
        .get("state_hash")
        .and_then(Value::as_str)
        .ok_or(ReplayError::FieldMismatch {
            seq: record.seq,
            field: "state_hash",
        })?;
    let actual = state_hash_for_record(record, machine_id, instance_id, state).ok_or(
        ReplayError::FieldMismatch {
            seq: record.seq,
            field: "state_format",
        },
    )?;
    if actual != expected {
        return Err(ReplayError::StateHashMismatch {
            seq: record.seq,
            expected: expected.into(),
            found: actual,
        });
    }
    Ok(())
}

fn verify_rejection(
    record: &Record,
    rejection: &Rejection,
    expected_details: &BTreeMap<String, Value>,
) -> Result<(), ReplayError> {
    for (field, expected) in [
        ("code", rejection.code),
        ("message", rejection.message.as_str()),
        ("hint", rejection.hint.as_str()),
    ] {
        if record.body.get(field).and_then(Value::as_str) != Some(expected) {
            return Err(ReplayError::FieldMismatch {
                seq: record.seq,
                field,
            });
        }
    }
    if record.body.get("details").and_then(Value::as_obj) != Some(expected_details) {
        return Err(ReplayError::FieldMismatch {
            seq: record.seq,
            field: "details",
        });
    }
    match (record.body.get("span"), rejection.span) {
        (None, None) => {}
        (Some(Value::Obj(span)), Some((start, end))) => {
            if span.get("start").and_then(Value::as_num) != Some(&start.to_string())
                || span.get("end").and_then(Value::as_num) != Some(&end.to_string())
            {
                return Err(ReplayError::FieldMismatch {
                    seq: record.seq,
                    field: "span",
                });
            }
        }
        _ => {
            return Err(ReplayError::FieldMismatch {
                seq: record.seq,
                field: "span",
            });
        }
    }
    Ok(())
}

fn expected_deadline_rejected_details(
    rejection: &Rejection,
    request_id: Option<&str>,
) -> BTreeMap<String, Value> {
    let mut details = BTreeMap::new();
    if let Some(block) = &rejection.block {
        details.insert("block".into(), Value::Str(block.clone()));
    }
    if let Some(cause) = rejection.cause {
        details.insert("cause".into(), Value::Str(cause.into()));
    }
    if let Some(source_state) = &rejection.source_state {
        details.insert("source_state".into(), Value::Str(source_state.clone()));
    }
    if let Some(transition_idx) = rejection.transition_idx {
        details.insert(
            "transition_idx".into(),
            Value::Num(transition_idx.to_string()),
        );
    }
    details.insert("trace".into(), rejection.trace.to_value());
    if let Some(request_id) = request_id {
        details.insert("request_id".into(), Value::Str(request_id.into()));
    }
    details
}

fn expected_event_rejected_details(
    r: &crate::step::Rejection,
    rid: Option<&str>,
    enabled: Value,
) -> BTreeMap<String, Value> {
    let mut d = BTreeMap::new();
    if let Some(b) = &r.block {
        d.insert("block".into(), Value::Str(b.clone()));
    }
    if let Some(c) = r.cause {
        d.insert("cause".into(), Value::Str(c.into()));
    }
    if let Some(s) = &r.source_state {
        d.insert("source_state".into(), Value::Str(s.clone()));
    }
    if let Some(idx) = r.transition_idx {
        d.insert("transition_idx".into(), Value::Num(idx.to_string()));
    }
    d.insert("trace".into(), r.trace.to_value());
    if let Some(rid) = rid {
        d.insert("request_id".into(), Value::Str(rid.into()));
    }
    d.insert("enabled_events".into(), enabled);
    d
}

fn expected_request_rejected_details(
    rid: Option<&str>,
    pending: &[String],
) -> BTreeMap<String, Value> {
    let mut d = BTreeMap::new();
    d.insert(
        "pending".into(),
        Value::Arr(pending.iter().cloned().map(Value::Str).collect()),
    );
    if let Some(rid) = rid {
        d.insert("request_id".into(), Value::Str(rid.into()));
    }
    d
}

fn enabled_reports_value(evs: &[crate::analyze::EventReport]) -> Value {
    Value::Arr(
        evs.iter()
            .map(|e| {
                let mut m = BTreeMap::new();
                m.insert("event".into(), Value::Str(e.event.clone()));
                m.insert(
                    "status".into(),
                    Value::Str(
                        match e.status {
                            crate::analyze::EventStatus::Enabled => "enabled",
                            crate::analyze::EventStatus::Disabled => "disabled",
                            crate::analyze::EventStatus::DependsOnPayload => "depends_on_payload",
                            crate::analyze::EventStatus::Preempted => "preempted",
                            crate::analyze::EventStatus::PreemptedMaybe => "preempted_maybe",
                        }
                        .into(),
                    ),
                );
                if !e.payload_fields.is_empty() {
                    m.insert(
                        "payload_fields".into(),
                        Value::Arr(e.payload_fields.iter().cloned().map(Value::Str).collect()),
                    );
                }
                Value::Obj(m)
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expr::ast::node_count;
    use crate::expr::eval::Budget;
    use crate::expr::parser;
    use crate::hashes::legacy_state_hash;
    use crate::json::{JsonLimits, parse};
    use crate::record::{RecordKind, seal};
    use crate::spec::{compile_accepted, compile_accepted_historical_unchecked};
    use crate::step::{Outcome, create, step};

    struct Collect(Vec<u64>);
    impl RecordSink for Collect {
        fn on_record(&mut self, record: &Record, _state: &StoreState) {
            self.0.push(record.seq);
        }
    }

    #[test]
    fn empty_fold() {
        let st = fold_with(Vec::new(), &mut NopSink).unwrap();
        assert_eq!(st.last_seq, 0);
        assert!(st.machines.is_empty());
    }

    #[test]
    fn sink_sees_seq_order() {
        let r0 = seal(
            0,
            0,
            RecordKind::Genesis,
            {
                let mut b = BTreeMap::new();
                b.insert("format".into(), Value::Str("fsm.journal/1".into()));
                b.insert("limits".into(), crate::record::limits_value());
                Value::Obj(b)
            },
            &crate::record::zeros(),
        );
        let mut c = Collect(Vec::new());
        fold_with(vec![r0], &mut c).unwrap();
        assert_eq!(c.0, [0]);
    }

    #[test]
    fn historical_guardless_budget_rejection_still_full_folds() {
        fn balanced_sum(terms: usize) -> String {
            if terms == 1 {
                return "1".into();
            }
            let left = terms / 2;
            format!("({} + {})", balanced_sum(left), balanced_sum(terms - left))
        }

        let context = (0..32)
            .map(|index| format!(r#"{{"name":"x{index}","ty":"int","init":"0"}}"#))
            .collect::<Vec<_>>()
            .join(",");
        let expression = format!("-({})", balanced_sum(64));
        assert_eq!(node_count(&parser::parse(&expression).unwrap()), 128);
        let sets = (0..32)
            .map(|index| format!(r#"{{"target":"x{index}","value":"{expression}"}}"#))
            .collect::<Vec<_>>()
            .join(",");
        let conjunction = (0..16).map(|_| "true").collect::<Vec<_>>().join(" and ");
        let diagnostic_guard = format!("not (not ({conjunction}))");
        assert_eq!(node_count(&parser::parse(&diagnostic_guard).unwrap()), 33);
        let mut events = (0..127)
            .map(|index| format!(r#"{{"name":"e{index}","fields":[]}}"#))
            .collect::<Vec<_>>();
        events.push(r#"{"name":"go","fields":[]}"#.into());
        let mut transitions = (0..127)
            .map(|index| {
                format!(r#"{{"from":"waiting","on":"e{index}","if":"{diagnostic_guard}"}}"#)
            })
            .collect::<Vec<_>>();
        transitions.push(format!(r#"{{"from":"waiting","on":"go","do":[{sets}]}}"#));
        let definition = parse(
            format!(
                r#"{{"format":"fsm.machine/1","name":"legacy_guard_tick","states":[{{"name":"waiting"}}],"initial":"waiting","context":[{context}],"events":[{}],"transitions":[{}]}}"#,
                events.join(","),
                transitions.join(",")
            )
            .as_bytes(),
            &JsonLimits::DEFAULT,
        )
        .unwrap();
        assert!(
            compile_accepted(&definition)
                .unwrap_err()
                .iter()
                .any(|finding| finding.code == "def/limit_eval")
        );
        let machine = compile_accepted_historical_unchecked(&definition).unwrap();
        let machine_id = machine.machine_id.clone();
        let tree = Tree::for_machine(&machine.spec);

        let Value::Obj(mut historical_limits) = crate::record::limits_value() else {
            unreachable!("limits are an object")
        };
        historical_limits.remove("max_regions");
        historical_limits.remove("max_deadlines");
        historical_limits.remove("max_eval_ticks");
        let genesis = seal(
            0,
            0,
            RecordKind::Genesis,
            Value::Obj(BTreeMap::from([
                ("format".into(), Value::Str("fsm.journal/1".into())),
                ("created_ts".into(), Value::Num("0".into())),
                ("limits".into(), Value::Obj(historical_limits)),
            ])),
            &crate::record::zeros(),
        );
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

        let created = create(&machine, &tree, &BTreeMap::new(), 2).unwrap();
        let state = InstanceState {
            status: created.status_after,
            configuration: created.configuration_after,
            ctx: created.ctx_after,
            history: created.history_after,
            deadlines: created.deadlines_after,
            pending: Vec::new(),
        };
        let instance_id = "legacy-instance";
        let created_record = seal(
            2,
            2,
            RecordKind::InstanceCreated,
            Value::Obj(BTreeMap::from([
                ("instance_id".into(), Value::Str(instance_id.into())),
                ("machine_id".into(), Value::Str(machine_id.clone())),
                ("request_id".into(), Value::Str("create".into())),
                (
                    "state_hash".into(),
                    Value::Str(legacy_state_hash(&machine_id, instance_id, 2, &state).unwrap()),
                ),
                ("leaf".into(), Value::Str("waiting".into())),
                ("overrides".into(), Value::Obj(BTreeMap::new())),
            ])),
            &defined.hash,
        );

        let payload = Value::Obj(BTreeMap::new());
        let mut budget = Budget::new(crate::limits::MAX_EVAL_TICKS);
        let Outcome::Rejected(rejection) =
            step(&machine, &tree, &state, "go", &payload, 3, &mut budget)
        else {
            panic!("the historical omitted-guard tick must exhaust the budget");
        };
        assert_eq!(rejection.code, "run/action_error");
        assert_eq!(rejection.cause, Some("internal/budget"));
        let mut current_analysis_budget = Budget::new(crate::limits::MAX_EVAL_TICKS);
        let current_enabled =
            crate::analyze::enabled_events(&machine, &tree, &state, &mut current_analysis_budget);
        let mut historical_analysis_budget = Budget::new(crate::limits::MAX_EVAL_TICKS);
        let enabled = crate::analyze::enabled_events_historical(
            &machine,
            &tree,
            &state,
            &mut historical_analysis_budget,
        );
        assert_eq!(
            current_enabled.last().map(|event| event.status),
            Some(crate::analyze::EventStatus::DependsOnPayload)
        );
        assert_eq!(
            enabled.last().map(|event| event.status),
            Some(crate::analyze::EventStatus::Enabled)
        );
        let mut rejected_body = BTreeMap::from([
            ("instance_id".into(), Value::Str(instance_id.into())),
            ("request_id".into(), Value::Str("send".into())),
            ("event".into(), Value::Str("go".into())),
            ("payload".into(), payload),
            (
                "state_hash".into(),
                Value::Str(legacy_state_hash(&machine_id, instance_id, 3, &state).unwrap()),
            ),
            ("code".into(), Value::Str(rejection.code.into())),
            ("message".into(), Value::Str(rejection.message.clone())),
            ("hint".into(), Value::Str(rejection.hint.clone())),
            (
                "details".into(),
                Value::Obj(expected_event_rejected_details(
                    &rejection,
                    Some("send"),
                    enabled_reports_value(&enabled),
                )),
            ),
        ]);
        if let Some((start, end)) = rejection.span {
            rejected_body.insert(
                "span".into(),
                Value::Obj(BTreeMap::from([
                    ("start".into(), Value::Num(start.to_string())),
                    ("end".into(), Value::Num(end.to_string())),
                ])),
            );
        }
        let rejected = seal(
            3,
            3,
            RecordKind::EventRejected,
            Value::Obj(rejected_body),
            &created_record.hash,
        );

        let replayed = fold_with(
            vec![genesis, defined, created_record, rejected],
            &mut NopSink,
        )
        .expect("the exact historical rejection must remain replayable");
        assert_eq!(replayed.last_seq, 3);
        assert_eq!(replayed.instances.get(instance_id), Some(&state));
    }
}
