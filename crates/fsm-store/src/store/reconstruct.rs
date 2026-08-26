use std::collections::BTreeMap;

use fsm_core::analyze::enabled_events;
use fsm_core::expr::eval::Budget;
use fsm_core::hashes::{STATE_FORMAT, configuration_value, state_hash};
use fsm_core::json::Value;
use fsm_core::machine::{ActiveConfiguration, InstanceState};
use fsm_core::record::{Record, RecordKind};
use fsm_core::replay::{
    NopSink, StoreState, StoredMachine, ctx_val_json, fold_with, replay_sealed_step,
};
use fsm_core::step::{DeadlineOutcome, Outcome, poll_deadline, step};
use fsm_core::tree::Tree;

use crate::journal_io::JournalHealth;

use super::json_helpers::enabled_json;
use super::{ErrorObj, Store};

pub(super) fn insert_configuration_fields(
    output: &mut BTreeMap<String, Value>,
    tree: &Tree,
    configuration: &ActiveConfiguration,
) {
    output.insert("configuration".into(), configuration_value(configuration));
    match configuration {
        ActiveConfiguration::Sequential { leaf } => {
            output.insert("leaf".into(), Value::Str(leaf.clone()));
            output.insert("state".into(), Value::Str(tree.dotted_path(leaf)));
            output.insert(
                "state_path".into(),
                Value::Arr(
                    tree.configuration(leaf)
                        .into_iter()
                        .map(Value::Str)
                        .collect(),
                ),
            );
        }
        ActiveConfiguration::Parallel { leaves } => {
            let mut regions = BTreeMap::new();
            for (region, _) in &tree.root_initials {
                let Some(region) = region.as_ref() else {
                    continue;
                };
                let Some(leaf) = leaves.get(region) else {
                    continue;
                };
                regions.insert(
                    region.clone(),
                    Value::Obj(BTreeMap::from([
                        ("leaf".into(), Value::Str(leaf.clone())),
                        ("state".into(), Value::Str(tree.dotted_path(leaf))),
                        (
                            "state_path".into(),
                            Value::Arr(
                                tree.configuration(leaf)
                                    .into_iter()
                                    .map(Value::Str)
                                    .collect(),
                            ),
                        ),
                    ])),
                );
            }
            output.insert("regions".into(), Value::Obj(regions));
        }
    }
}

pub(super) fn insert_transition_configuration_fields(
    output: &mut BTreeMap<String, Value>,
    before: &ActiveConfiguration,
    after: &ActiveConfiguration,
) {
    output.insert("from_configuration".into(), configuration_value(before));
    output.insert("to_configuration".into(), configuration_value(after));
    if let (
        ActiveConfiguration::Sequential { leaf: from },
        ActiveConfiguration::Sequential { leaf: to },
    ) = (before, after)
    {
        output.insert("from_leaf".into(), Value::Str(from.clone()));
        output.insert("to_leaf".into(), Value::Str(to.clone()));
    }
}

pub(super) fn pending_deadlines_value(machine: &StoredMachine, state: &InstanceState) -> Value {
    let mut pending: Vec<_> = state
        .deadlines
        .iter()
        .filter_map(|(name, due_ms)| {
            machine
                .compiled
                .spec
                .deadlines
                .iter()
                .position(|deadline| deadline.name == *name)
                .map(|index| (due_ms, index, name))
        })
        .collect();
    pending.sort_by(|left, right| (left.0, left.1).cmp(&(right.0, right.1)));
    Value::Arr(
        pending
            .into_iter()
            .map(|(due_ms, index, name)| {
                Value::Obj(BTreeMap::from([
                    ("name".into(), Value::Str(name.clone())),
                    ("deadline_idx".into(), Value::Num(index.to_string())),
                    ("due_ms".into(), Value::Str(due_ms.to_string())),
                ]))
            })
            .collect(),
    )
}

pub(super) fn reconstruct_applied(
    pre: &StoreState,
    rec: &Record,
    iid: &str,
    request_id: &str,
    created_seq: u64,
    machine_history: Vec<Value>,
) -> Option<Value> {
    let ev = rec.body.get("event").and_then(Value::as_str)?;
    let payload = rec
        .body
        .get("payload")
        .cloned()
        .unwrap_or(Value::Obj(BTreeMap::new()));
    let mid = pre.instance_machines.get(iid)?;
    let m = pre.machines.get(mid)?;
    let inst = pre.instances.get(iid)?;
    // History and explain re-apply a record as the macrostep it was; the
    // standard budget would fail a legitimately deep cascade the live write
    // accepted and silently drop its trace.
    let mut bud = Budget::new(fsm_core::limits::MACROSTEP_EVAL_TICKS);
    match step(&m.compiled, &m.tree, inst, ev, &payload, rec.ts, &mut bud) {
        Outcome::Applied(a) => {
            let mut post_inst = inst.clone();
            post_inst.status = a.status_after;
            post_inst.configuration = a.configuration_after.clone();
            post_inst.ctx = a.ctx_after.clone();
            post_inst.history = a.history_after.clone();
            post_inst.deadlines = a.deadlines_after.clone();
            post_inst.pending.extend(
                a.effects
                    .iter()
                    .map(|e| format!("{iid}/{}/{}", rec.seq, e.k)),
            );
            let mut post = pre.clone();
            post.instances.insert(iid.into(), post_inst);
            post.last_seq = rec.seq;
            post.last_hash = rec.hash.clone();
            let mut v = view_at(
                &post,
                iid,
                Some(request_id),
                Some(true),
                rec.seq,
                created_seq,
                machine_history,
            )
            .ok()?;
            if let Value::Obj(o) = &mut v {
                o.insert("applied".into(), Value::Bool(true));
                o.insert("ok".into(), Value::Str("true".into()));
                insert_configuration_fields(o, &m.tree, &a.configuration_after);
                let mut tr = BTreeMap::new();
                tr.insert("source_state".into(), Value::Str(a.source_state.clone()));
                tr.insert(
                    "transition_idx".into(),
                    Value::Num(a.transition_idx.to_string()),
                );
                tr.insert("internal".into(), Value::Bool(a.internal));
                if let Some(region) = &a.region {
                    tr.insert("region".into(), Value::Str(region.clone()));
                }
                insert_transition_configuration_fields(
                    &mut tr,
                    &inst.configuration,
                    &a.configuration_after,
                );
                tr.insert(
                    "exited".into(),
                    Value::Arr(a.exited.iter().cloned().map(Value::Str).collect()),
                );
                tr.insert(
                    "entered".into(),
                    Value::Arr(a.entered.iter().cloned().map(Value::Str).collect()),
                );
                o.insert("transition".into(), Value::Obj(tr));
                o.insert("trace".into(), a.trace.to_value());
                o.insert(
                    "monitor_flags".into(),
                    Value::Arr(a.monitor_flags.iter().cloned().map(Value::Str).collect()),
                );
            }
            Some(v)
        }
        _ => None,
    }
}

pub(super) fn reconstruct_deadline_applied(
    pre: &StoreState,
    record: &Record,
    instance_id: &str,
    request_id: &str,
    created_seq: u64,
    machine_history: Vec<Value>,
) -> Option<Value> {
    let machine_id = pre.instance_machines.get(instance_id)?;
    let machine = pre.machines.get(machine_id)?;
    let instance = pre.instances.get(instance_id)?;
    let mut budget = Budget::new(fsm_core::limits::MACROSTEP_EVAL_TICKS);
    let DeadlineOutcome::Applied(applied) = poll_deadline(
        &machine.compiled,
        &machine.tree,
        instance,
        record.ts,
        &mut budget,
    ) else {
        return None;
    };
    let transition = applied.transition;
    let mut post_instance = instance.clone();
    post_instance.status = transition.status_after;
    post_instance.configuration = transition.configuration_after.clone();
    post_instance.ctx = transition.ctx_after.clone();
    post_instance.history = transition.history_after.clone();
    post_instance.deadlines = transition.deadlines_after.clone();
    post_instance.pending.extend(
        transition
            .effects
            .iter()
            .map(|effect| format!("{instance_id}/{}/{}", record.seq, effect.k)),
    );
    let mut post = pre.clone();
    post.instances.insert(instance_id.into(), post_instance);
    post.last_seq = record.seq;
    post.last_hash = record.hash.clone();
    let mut response = view_at(
        &post,
        instance_id,
        Some(request_id),
        Some(true),
        record.seq,
        created_seq,
        machine_history,
    )
    .ok()?;
    if let Value::Obj(output) = &mut response {
        output.insert("deadline_applied".into(), Value::Bool(true));
        output.insert("deadline_not_due".into(), Value::Bool(false));
        output.insert("deadline".into(), Value::Str(applied.deadline.name));
        output.insert(
            "deadline_idx".into(),
            Value::Num(applied.deadline.deadline_idx.to_string()),
        );
        output.insert(
            "due_ms".into(),
            Value::Str(applied.deadline.due_ms.to_string()),
        );
        let mut transition_value = BTreeMap::from([
            (
                "source_state".into(),
                Value::Str(transition.source_state.clone()),
            ),
            (
                "deadline_idx".into(),
                Value::Num(transition.transition_idx.to_string()),
            ),
            ("internal".into(), Value::Bool(false)),
            (
                "exited".into(),
                Value::Arr(transition.exited.iter().cloned().map(Value::Str).collect()),
            ),
            (
                "entered".into(),
                Value::Arr(transition.entered.iter().cloned().map(Value::Str).collect()),
            ),
        ]);
        if let Some(region) = &transition.region {
            transition_value.insert("region".into(), Value::Str(region.clone()));
        }
        insert_transition_configuration_fields(
            &mut transition_value,
            &instance.configuration,
            &transition.configuration_after,
        );
        output.insert("transition".into(), Value::Obj(transition_value));
        output.insert("trace".into(), transition.trace.to_value());
        output.insert(
            "monitor_flags".into(),
            Value::Arr(
                transition
                    .monitor_flags
                    .iter()
                    .cloned()
                    .map(Value::Str)
                    .collect(),
            ),
        );
    }
    Some(response)
}

pub(super) fn reconstruct_ignored(
    folded: &StoreState,
    rec: &Record,
    iid: &str,
    request_id: &str,
    created_seq: u64,
    machine_history: Vec<Value>,
) -> Option<Value> {
    let inst = folded.instances.get(iid)?;
    let mid = folded.instance_machines.get(iid)?;
    let m = folded.machines.get(mid)?;
    let mut v = view_at(
        folded,
        iid,
        Some(request_id),
        Some(true),
        rec.seq,
        created_seq,
        machine_history,
    )
    .ok()?;
    if let Value::Obj(o) = &mut v {
        o.insert("ok".into(), Value::Str("true".into()));
        o.insert("ignored".into(), Value::Bool(true));
        o.insert("applied".into(), Value::Bool(false));
        o.insert("seq".into(), Value::Num(rec.seq.to_string()));
        o.insert("monitor_flags".into(), Value::Arr(vec![]));
        o.insert("trace".into(), Value::Obj(BTreeMap::new()));
        o.insert(
            "transition".into(),
            Value::Obj({
                let mut transition = BTreeMap::from([
                    ("transition_idx".into(), Value::Num("-1".into())),
                    ("internal".into(), Value::Bool(false)),
                    ("exited".into(), Value::Arr(vec![])),
                    ("entered".into(), Value::Arr(vec![])),
                ]);
                if let Some(leaf) = inst.configuration.sequential_leaf() {
                    transition.insert("source_state".into(), Value::Str(leaf.to_string()));
                }
                insert_transition_configuration_fields(
                    &mut transition,
                    &inst.configuration,
                    &inst.configuration,
                );
                transition
            }),
        );
        insert_configuration_fields(o, &m.tree, &inst.configuration);
    }
    Some(v)
}

pub(super) fn load_tags_from_records(records: &[Record]) -> BTreeMap<String, Vec<String>> {
    let mut tags = BTreeMap::new();
    for rec in records {
        if rec.kind != RecordKind::InstanceCreated {
            continue;
        }
        let Some(iid) = rec.body.get("instance_id").and_then(Value::as_str) else {
            continue;
        };
        if let Some(arr) = rec.body.get("tags").and_then(Value::as_arr) {
            let v: Vec<String> = arr
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect();
            if !v.is_empty() {
                tags.insert(iid.into(), v);
            }
        }
    }
    tags
}

pub(super) fn verify_prefix_hashes(records: &[Record]) -> bool {
    records.windows(2).all(|w| w[1].prev == w[0].hash)
}

pub(super) fn history_entry(
    store: &Store,
    rec: &Record,
    include_trace: bool,
) -> Result<Value, ErrorObj> {
    let mut e = BTreeMap::new();
    e.insert("seq".into(), Value::Num(rec.seq.to_string()));
    e.insert("kind".into(), Value::Str(format!("{:?}", rec.kind)));
    e.insert("ts".into(), Value::Num(rec.ts.to_string()));
    e.insert("hash".into(), Value::Str(rec.hash.clone()));
    if let Some(rid) = rec.body.get("request_id") {
        e.insert("request_id".into(), rid.clone());
    }
    if let Some(ev) = rec.body.get("event") {
        e.insert("event".into(), ev.clone());
    }
    if let Some(deadline) = rec.body.get("deadline") {
        e.insert("deadline".into(), deadline.clone());
    }
    if let Some(p) = rec.body.get("payload") {
        e.insert("payload".into(), p.clone());
    }
    if let Some(n) = rec.body.get("note") {
        e.insert("note".into(), n.clone());
    }
    if let Some(r) = rec.body.get("reason") {
        e.insert("reason".into(), r.clone());
    }
    if let Some(microsteps) = rec.body.get("microsteps") {
        e.insert("microsteps".into(), microsteps.clone());
    }
    if rec.seq > 0 {
        if let Ok(pre) = fold_prefix(&store.records, rec.seq.saturating_sub(1)) {
            if let Ok(post) = fold_prefix(&store.records, rec.seq) {
                if let Some(iid) = rec.body.get("instance_id").and_then(Value::as_str) {
                    if let Some(before) = pre.instances.get(iid) {
                        e.insert(
                            "from_configuration".into(),
                            configuration_value(&before.configuration),
                        );
                        e.insert(
                            "before_configuration".into(),
                            configuration_value(&before.configuration),
                        );
                        if let Some(leaf) = before.configuration.sequential_leaf() {
                            e.insert("from_leaf".into(), Value::Str(leaf.to_string()));
                            e.insert("before_leaf".into(), Value::Str(leaf.to_string()));
                        }
                        let mut ctx = BTreeMap::new();
                        for (k, v) in &before.ctx {
                            ctx.insert(k.clone(), ctx_val_json(v));
                        }
                        e.insert("before_context".into(), Value::Obj(ctx));
                    }
                    if let Some(after) = post.instances.get(iid) {
                        e.insert(
                            "to_configuration".into(),
                            configuration_value(&after.configuration),
                        );
                        e.insert(
                            "after_configuration".into(),
                            configuration_value(&after.configuration),
                        );
                        if let Some(leaf) = after.configuration.sequential_leaf() {
                            e.insert("to_leaf".into(), Value::Str(leaf.to_string()));
                            e.insert("after_leaf".into(), Value::Str(leaf.to_string()));
                        }
                        let mut ctx = BTreeMap::new();
                        for (k, v) in &after.ctx {
                            ctx.insert(k.clone(), ctx_val_json(v));
                        }
                        e.insert("context_after".into(), Value::Obj(ctx.clone()));
                        e.insert("after_context".into(), Value::Obj(ctx));
                        if !e.contains_key("from_configuration") {
                            e.insert(
                                "from_configuration".into(),
                                configuration_value(&after.configuration),
                            );
                            if let Some(leaf) = after.configuration.sequential_leaf() {
                                e.insert("from_leaf".into(), Value::Str(leaf.to_string()));
                            }
                        }
                    }
                }
            }
            if include_trace && rec.kind == RecordKind::EventApplied {
                if let Some(iid) = rec.body.get("instance_id").and_then(Value::as_str) {
                    if let Some(rid) = rec.body.get("request_id").and_then(Value::as_str) {
                        if let Some(v) = reconstruct_applied(
                            &pre,
                            rec,
                            iid,
                            rid,
                            store.created_seq(iid),
                            store.machine_history(iid),
                        ) {
                            if let Some(tr) = v.get("trace") {
                                e.insert("trace".into(), tr.clone());
                            }
                        }
                    }
                }
            } else if include_trace && rec.kind == RecordKind::DeadlineApplied {
                if let Some(iid) = rec.body.get("instance_id").and_then(Value::as_str) {
                    if let Some(rid) = rec.body.get("request_id").and_then(Value::as_str) {
                        if let Some(value) = reconstruct_deadline_applied(
                            &pre,
                            rec,
                            iid,
                            rid,
                            store.created_seq(iid),
                            store.machine_history(iid),
                        ) {
                            if let Some(trace) = value.get("trace") {
                                e.insert("trace".into(), trace.clone());
                            }
                        }
                    }
                }
            } else if include_trace && rec.kind == RecordKind::EventRejected {
                if let Some(iid) = rec.body.get("instance_id").and_then(Value::as_str) {
                    if let Some(ev) = rec.body.get("event").and_then(Value::as_str) {
                        if let Some(mid) = pre.instance_machines.get(iid) {
                            if let Some(m) = pre.machines.get(mid) {
                                if let Some(inst) = pre.instances.get(iid) {
                                    let payload = rec
                                        .body
                                        .get("payload")
                                        .cloned()
                                        .unwrap_or(Value::Obj(BTreeMap::new()));
                                    if let Outcome::Rejected(r) = replay_sealed_step(
                                        &m.compiled,
                                        &m.tree,
                                        inst,
                                        ev,
                                        &payload,
                                        rec.ts,
                                        &rec.body,
                                    ) {
                                        e.insert("trace".into(), r.trace.to_value());
                                    }
                                }
                            }
                        }
                    }
                }
            } else if include_trace && rec.kind == RecordKind::DeadlineRejected {
                if let Some(iid) = rec.body.get("instance_id").and_then(Value::as_str) {
                    if let Some(machine_id) = pre.instance_machines.get(iid) {
                        if let (Some(machine), Some(instance)) =
                            (pre.machines.get(machine_id), pre.instances.get(iid))
                        {
                            let mut budget = Budget::new(fsm_core::limits::MACROSTEP_EVAL_TICKS);
                            if let DeadlineOutcome::Rejected(rejected) = poll_deadline(
                                &machine.compiled,
                                &machine.tree,
                                instance,
                                rec.ts,
                                &mut budget,
                            ) {
                                e.insert("trace".into(), rejected.rejection.trace.to_value());
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(Value::Obj(e))
}

pub(super) fn fold_prefix(records: &[Record], through: u64) -> Result<StoreState, ErrorObj> {
    let recs: Vec<Record> = records
        .iter()
        .filter(|r| r.seq <= through)
        .cloned()
        .collect();
    fold_with(recs, &mut NopSink)
        .map_err(|e| ErrorObj::new("store/state_hash_mismatch", format!("{e:?}")))
}

/// The parent edge as the folded prefix knows it.
///
/// A child's id derives from its parent and slot through a hash, which does
/// not invert, so the edge is found by asking every instance whose slots
/// could name this one — bounded by the number of slots in the store, which
/// `MAX_INVOKES_PER_STATE` and `MAX_INVOKE_DEPTH` keep small.
fn parent_at(state: &StoreState, instance_id: &str) -> Value {
    for (candidate, instance) in &state.instances {
        for slot in instance.invocations.keys() {
            if fsm_core::hashes::child_instance_id(candidate, slot) == instance_id {
                return Value::Obj(BTreeMap::from([
                    ("instance_id".into(), Value::Str(candidate.clone())),
                    ("slot".into(), Value::Str(slot.clone())),
                ]));
            }
        }
    }
    Value::Null
}

/// The slots this instance holds at the reconstructed point.
fn children_at(
    state: &StoreState,
    instance_id: &str,
    instance: &fsm_core::machine::InstanceState,
) -> Vec<Value> {
    instance
        .invocations
        .iter()
        .map(|(slot, invocation)| {
            let child_id = fsm_core::hashes::child_instance_id(instance_id, slot);
            let status = state
                .instances
                .get(&child_id)
                .map(|child| child.status.as_str().to_string());
            let mut entry = BTreeMap::from([
                ("slot".into(), Value::Str(slot.clone())),
                ("child_instance_id".into(), Value::Str(child_id)),
                (
                    "child_machine_id".into(),
                    Value::Str(invocation.child_machine_id.clone()),
                ),
                (
                    "invocation_status".into(),
                    Value::Str(invocation.status.as_str().into()),
                ),
            ]);
            if let Some(status) = status {
                entry.insert("status".into(), Value::Str(status));
            }
            Value::Obj(entry)
        })
        .collect()
}

pub(super) fn view_at(
    state: &StoreState,
    instance_id: &str,
    request_id: Option<&str>,
    duplicate: Option<bool>,
    seq: u64,
    // The record that brought this instance into existence. A folded prefix
    // cannot derive it — a creation is a record, not a state — so the caller,
    // which holds the history index, supplies it.
    created_seq: u64,
    // The definitions this instance has been on, for the same reason: they
    // are read from records, and a reconstruction sees only a folded state.
    machine_history: Vec<Value>,
) -> Result<Value, ErrorObj> {
    let inst = state
        .instances
        .get(instance_id)
        .ok_or_else(|| ErrorObj::new("req/instance_not_found", instance_id))?;
    let mid = state
        .instance_machines
        .get(instance_id)
        .ok_or_else(|| ErrorObj::new("req/instance_not_found", instance_id))?;
    let m = state
        .machines
        .get(mid)
        .ok_or_else(|| ErrorObj::new("req/machine_not_found", mid.as_str()))?;
    let mut bud = Budget::new(fsm_core::limits::MAX_EVAL_TICKS);
    let enabled = enabled_events(&m.compiled, &m.tree, inst, &mut bud);
    let mut ctx = BTreeMap::new();
    for (k, v) in &inst.ctx {
        ctx.insert(k.clone(), ctx_val_json(v));
    }
    let mut mobj = BTreeMap::new();
    mobj.insert("ok".into(), Value::Str("true".into()));
    mobj.insert("instance_id".into(), Value::Str(instance_id.into()));
    insert_configuration_fields(&mut mobj, &m.tree, &inst.configuration);
    let mut mac = BTreeMap::new();
    mac.insert("machine_id".into(), Value::Str(mid.clone()));
    mac.insert("name".into(), Value::Str(m.compiled.spec.name.clone()));
    mobj.insert("machine".into(), Value::Obj(mac));
    mobj.insert("status".into(), Value::Str(inst.status.as_str().into()));
    mobj.insert("context".into(), Value::Obj(ctx));
    mobj.insert(
        "effects_pending".into(),
        Value::Arr(inst.pending.iter().cloned().map(Value::Str).collect()),
    );
    // The same three fields the live view carries, so a replayed response is
    // the response — derived from the folded prefix rather than from a live
    // index, because that prefix is all a reconstruction may look at.
    mobj.insert("parent".into(), parent_at(state, instance_id));
    mobj.insert(
        "children".into(),
        Value::Arr(children_at(state, instance_id, inst)),
    );
    mobj.insert("created_seq".into(), Value::Num(created_seq.to_string()));
    mobj.insert("machine_history".into(), Value::Arr(machine_history));
    mobj.insert("seq".into(), Value::Num(seq.to_string()));
    mobj.insert(
        "state_hash".into(),
        Value::Str(state_hash(mid, instance_id, seq, inst)),
    );
    mobj.insert("state_format".into(), Value::Str(STATE_FORMAT.into()));
    mobj.insert("enabled_events".into(), enabled_json(&enabled));
    mobj.insert("deadlines_pending".into(), pending_deadlines_value(m, inst));
    if let Some(r) = request_id {
        mobj.insert("request_id".into(), Value::Str(r.into()));
    }
    if let Some(d) = duplicate {
        mobj.insert("duplicate".into(), Value::Bool(d));
    }
    Ok(Value::Obj(mobj))
}

pub(super) fn health_err(h: &JournalHealth) -> ErrorObj {
    let code = match h {
        JournalHealth::TornTail { .. } => "store/torn_tail",
        JournalHealth::ChainBroken { .. } => "store/chain_broken",
        JournalHealth::StateHashMismatch { .. } => "store/state_hash_mismatch",
        JournalHealth::NonCanonical { .. } => "store/non_canonical",
        JournalHealth::LockIo(_) => "store/lock",
        JournalHealth::ReplayMismatch { .. } => "store/state_hash_mismatch",
        JournalHealth::MissingGenesis => "store/chain_broken",
        JournalHealth::VersionMismatch { .. } => "store/version_mismatch",
        JournalHealth::StoreIo(_) => "io/read",
        JournalHealth::Ok => "store/lock",
    };
    let err = ErrorObj::new(code, h.message());
    if matches!(h, JournalHealth::VersionMismatch { .. }) {
        // Post-migration this fires for newer or unknown formats, where the
        // store may be the only good copy — never advise deleting it.
        err.hint("upgrade fsm to a build that supports this store format, or point --data-dir at a fresh directory")
    } else if matches!(h, JournalHealth::StoreIo(_)) {
        err.hint("restore the named persistence path as a readable regular file or directory within the documented per-unit limit, then retry")
    } else {
        err
    }
}
