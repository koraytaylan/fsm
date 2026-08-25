//! Re-derivation of a pending effect's name and args from the journal.
//!
//! The store surfaces a pending effect as an opaque `{instance}/{seq}/{k}` id
//! and nothing else — no record body and no view carries the emitted name or
//! the evaluated args. A human host reads the machine to know what
//! `order-1/3/0` means; a mechanical executor replays the one record that
//! emitted it.
//!
//! The replay is exact rather than approximate because SPEC pins every
//! record's timestamp to the `now_ms` the pure call received: re-running the
//! same engine entry point against the state before that record, at the
//! record's own `ts`, reproduces the emit the store hashed into the chain.
//! `fsm-store` does this privately for history rendering; this module does it
//! against the public API only, which is why the engine needs no change.

use std::collections::BTreeMap;

use fsm_core::expr::eval::{Budget, Val};
use fsm_core::json::Value;
use fsm_core::limits::MAX_EVAL_TICKS;
use fsm_core::machine::InstanceState;
use fsm_core::record::{Record, RecordKind};
use fsm_core::replay::{NopSink, StoreState, StoredMachine, fold_with, parse_ctx_json};
use fsm_core::step::{DeadlineOutcome, EffectOut, Outcome, create, poll_deadline, step};
use fsm_store::store::Store;

use crate::error::ExecError;

/// One pending effect, resolved back to what the machine actually emitted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingEffect {
    /// The instance that emitted it.
    pub instance_id: String,
    /// The opaque `{instance}/{seq}/{k}` id the store hands out.
    pub effect_id: String,
    /// The declared effect name, re-derived by replay.
    pub effect_name: String,
    /// The evaluated arguments, re-derived by the same replay.
    pub args: BTreeMap<String, Val>,
    /// The `seq` component of the id, which is `0` for a creation-time emit.
    pub emitted_seq: u64,
    /// The emit's own index within its transition.
    pub k: u32,
}

/// Resolve one pending effect id against a store opened for reading.
///
/// Costs one prefix fold — the whole journal up to the emitting record, with
/// every `machine_defined` record on the way re-compiled. That is the price of
/// deriving the emit exactly rather than trusting a cache, and it is why
/// callers resolve an id once and carry the result: the watcher memoizes, so
/// the cost is paid on the scan that first sees an effect and never again. A
/// restart with many effects already pending pays it once per effect on its
/// first scan.
pub fn resolve(store: &Store, effect_id: &str) -> Result<PendingEffect, ExecError> {
    let id = EffectId::parse(effect_id)?;
    let record = emitting_record(store, &id)?;
    let before = fold_before(store, record, &id)?;
    let emitted = replay_emits(&before, record, &id)?;
    let emit = emitted
        .into_iter()
        .find(|emit| emit.k == id.k)
        .ok_or_else(|| unresolved(&id, format!("the emitting record emitted no k={}", id.k)))?;
    Ok(PendingEffect {
        instance_id: id.instance_id.to_string(),
        effect_id: effect_id.to_string(),
        effect_name: emit.name,
        args: emit.args,
        emitted_seq: id.seq,
        k: id.k,
    })
}

/// The three parts the store packs into one pending-effect id.
struct EffectId<'a> {
    raw: &'a str,
    instance_id: &'a str,
    seq: u64,
    k: u32,
}

impl<'a> EffectId<'a> {
    /// Split from the right: an instance id may not contain `/`, but the
    /// rightmost two separators are the ones that mean something here.
    fn parse(effect_id: &'a str) -> Result<Self, ExecError> {
        let shape = "an effect id is {instance_id}/{seq}/{k}";
        let malformed = || {
            ExecError::new(
                "exec/effect_unresolved",
                format!("cannot resolve {effect_id}: {shape}"),
            )
            .hint("pass an id exactly as it appears in the instance's effects_pending")
            .details(Value::Obj(BTreeMap::from([(
                "effect_id".into(),
                Value::Str(effect_id.into()),
            )])))
        };
        let (head, k) = effect_id.rsplit_once('/').ok_or_else(malformed)?;
        let (instance_id, seq) = head.rsplit_once('/').ok_or_else(malformed)?;
        if instance_id.is_empty() {
            return Err(malformed());
        }
        Ok(Self {
            raw: effect_id,
            instance_id,
            seq: seq.parse().map_err(|_| malformed())?,
            k: k.parse().map_err(|_| malformed())?,
        })
    }
}

fn emitting_record<'a>(store: &'a Store, id: &EffectId<'_>) -> Result<&'a Record, ExecError> {
    if id.seq == 0 {
        // A creation-time emit is `{instance}/0/{k}` — the store writes a
        // literal zero in the seq slot (`store/instance/create.rs`), because
        // the id is composed before the record's own seq is known. So `0`
        // names this instance's `instance_created` record wherever it landed,
        // never journal seq 0, which is genesis and emits nothing.
        //
        // Searched from the newest end because creation is not guarded against
        // re-use of an instance id: a journal can hold two `instance_created`
        // records for one id, both composing `{instance}/0/{k}`, and the
        // pending effect belongs to the creation that is current. Taking the
        // first would substitute the older creation's arguments into the argv
        // and run the handler against stale values.
        return store
            .records
            .iter()
            .rev()
            .find(|record| {
                record.kind == RecordKind::InstanceCreated
                    && instance_of(record) == Some(id.instance_id)
            })
            .ok_or_else(|| unresolved(id, "this instance has no instance_created record"));
    }
    let position = store
        .records
        .binary_search_by(|record| record.seq.cmp(&id.seq))
        .map_err(|_| unresolved(id, format!("the journal holds no record at seq {}", id.seq)))?;
    let record = &store.records[position];
    if instance_of(record) != Some(id.instance_id) {
        return Err(unresolved(
            id,
            format!("seq {} belongs to another instance", id.seq),
        ));
    }
    Ok(record)
}

/// Fold the journal prefix the emitting record was applied against.
fn fold_before(store: &Store, record: &Record, id: &EffectId<'_>) -> Result<StoreState, ExecError> {
    let prefix: Vec<Record> = store
        .records
        .iter()
        .filter(|candidate| candidate.seq < record.seq)
        .cloned()
        .collect();
    fold_with(prefix, &mut NopSink)
        .map_err(|error| unresolved(id, format!("the journal prefix does not fold: {error:?}")))
}

/// Re-run the pure entry point that wrote this record, at the record's own
/// timestamp, and return what it emitted.
fn replay_emits(
    before: &StoreState,
    record: &Record,
    id: &EffectId<'_>,
) -> Result<Vec<EffectOut>, ExecError> {
    let mut budget = Budget::new(MAX_EVAL_TICKS);
    match record.kind {
        RecordKind::InstanceCreated => {
            let machine = created_machine(before, record, id)?;
            let overrides = created_overrides(machine, record, id)?;
            create(&machine.compiled, &machine.tree, &overrides, record.ts)
                .map(|applied| applied.effects)
                .map_err(|rejection| {
                    unresolved(
                        id,
                        format!(
                            "replaying instance_created was rejected: {}",
                            rejection.code
                        ),
                    )
                })
        }
        RecordKind::EventApplied => {
            let (machine, instance) = machine_and_instance(before, id)?;
            let event = record
                .body
                .get("event")
                .and_then(Value::as_str)
                .ok_or_else(|| unresolved(id, "the event_applied record names no event"))?;
            let payload = record
                .body
                .get("payload")
                .cloned()
                .unwrap_or_else(|| Value::Obj(BTreeMap::new()));
            match step(
                &machine.compiled,
                &machine.tree,
                instance,
                event,
                &payload,
                record.ts,
                &mut budget,
            ) {
                Outcome::Applied(applied) => Ok(applied.effects),
                _ => Err(unresolved(id, "replaying event_applied did not apply")),
            }
        }
        RecordKind::DeadlineApplied => {
            let (machine, instance) = machine_and_instance(before, id)?;
            match poll_deadline(
                &machine.compiled,
                &machine.tree,
                instance,
                record.ts,
                &mut budget,
            ) {
                DeadlineOutcome::Applied(applied) => Ok(applied.transition.effects),
                _ => Err(unresolved(id, "replaying deadline_applied did not apply")),
            }
        }
        other => Err(unresolved(
            id,
            format!("a {} record emits nothing", other.as_str()),
        )),
    }
}

/// The machine an `instance_created` record names, which the pre-state knows
/// but the pre-state's instance map does not: the instance is created *by*
/// this record.
fn created_machine<'a>(
    before: &'a StoreState,
    record: &Record,
    id: &EffectId<'_>,
) -> Result<&'a StoredMachine, ExecError> {
    let machine_id = record
        .body
        .get("machine_id")
        .and_then(Value::as_str)
        .ok_or_else(|| unresolved(id, "the instance_created record names no machine"))?;
    before
        .machines
        .get(machine_id)
        .ok_or_else(|| unresolved(id, format!("machine {machine_id} is not in the journal")))
}

/// Read the creation overrides back out of the record, against the machine's
/// declared context types — the record persists them in the canonical string
/// form, which is exactly what [`parse_ctx_json`] reads.
fn created_overrides(
    machine: &StoredMachine,
    record: &Record,
    id: &EffectId<'_>,
) -> Result<BTreeMap<String, Val>, ExecError> {
    let Some(raw) = record.body.get("overrides") else {
        return Ok(BTreeMap::new());
    };
    let fields = raw
        .as_obj()
        .ok_or_else(|| unresolved(id, "the instance_created overrides are not an object"))?;
    let mut overrides = BTreeMap::new();
    for (name, value) in fields {
        let declared = machine
            .compiled
            .spec
            .context
            .iter()
            .find(|variable| variable.name == *name)
            .ok_or_else(|| unresolved(id, format!("override {name} is not a context variable")))?;
        let parsed = parse_ctx_json(&declared.ty, value)
            .ok_or_else(|| unresolved(id, format!("override {name} is not a {:?}", declared.ty)))?;
        overrides.insert(name.clone(), parsed);
    }
    Ok(overrides)
}

fn machine_and_instance<'a>(
    before: &'a StoreState,
    id: &EffectId<'_>,
) -> Result<(&'a StoredMachine, &'a InstanceState), ExecError> {
    let machine_id = before
        .instance_machines
        .get(id.instance_id)
        .ok_or_else(|| unresolved(id, "the instance did not exist before this record"))?;
    let machine = before
        .machines
        .get(machine_id)
        .ok_or_else(|| unresolved(id, format!("machine {machine_id} is not in the journal")))?;
    let instance = before
        .instances
        .get(id.instance_id)
        .ok_or_else(|| unresolved(id, "the instance did not exist before this record"))?;
    Ok((machine, instance))
}

fn instance_of(record: &Record) -> Option<&str> {
    record.body.get("instance_id").and_then(Value::as_str)
}

fn unresolved(id: &EffectId<'_>, reason: impl Into<String>) -> ExecError {
    let reason = reason.into();
    ExecError::new(
        "exec/effect_unresolved",
        format!("cannot resolve {}: {reason}", id.raw),
    )
    .hint("check that the id came from this store's effects_pending and that the journal is intact")
    .details(Value::Obj(BTreeMap::from([
        ("effect_id".into(), Value::Str(id.raw.into())),
        ("instance_id".into(), Value::Str(id.instance_id.into())),
        ("reason".into(), Value::Str(reason)),
    ])))
}
