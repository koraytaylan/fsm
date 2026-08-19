//! Pure journal fold through the engine.

#![allow(clippy::collapsible_if, unused_imports)]

use std::collections::BTreeMap;

use crate::hashes::{
    STATE_FORMAT, configuration_value, domain_hash, legacy_state_hash, state_hash,
};
use crate::json::Value;
use crate::machine::{ActiveConfiguration, CompiledMachine, InstanceState};
use crate::record::{Record, RecordKind};
use crate::tree::Tree;

mod apply;
mod ctx;
mod deadline;
mod report;
#[cfg(test)]
mod tests;
mod verify;

use apply::apply;

pub use ctx::{ctx_val_json, ctx_val_string, parse_ctx_json, parse_ctx_val};

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
