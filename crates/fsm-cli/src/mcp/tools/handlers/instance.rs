use std::collections::BTreeMap;

use fsm_core::json::Value;

use crate::clock::Clock;
use crate::store::{ErrorObj, Store};

use crate::mcp::tools::dispatch::{expect_seq_arg, str_arg};
use crate::mcp::tools::validate::type_name;

pub(in crate::mcp::tools) fn run_instance_create(
    store: &mut Store,
    clock: &mut dyn Clock,
    args: &Value,
) -> Result<Value, ErrorObj> {
    let machine = str_arg(args, "machine").unwrap_or("");
    let rid = str_arg(args, "request_id").unwrap_or("");
    let iid = format!("inst-{rid}");
    let mut overrides = BTreeMap::new();
    if let Some(ctx) = args.get("context") {
        match ctx {
            Value::Obj(o) => {
                let m = store.resolve_machine(machine)?;
                overrides = crate::store::apply_context_overrides(&m.compiled.spec, o)?;
            }
            Value::Arr(_) => return Err(crate::store::context_not_object("array")),
            other => {
                return Err(crate::store::context_not_object(type_name(other)));
            }
        }
    }
    let tags: Vec<String> = args
        .get("tags")
        .and_then(Value::as_arr)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    store.create_instance_ctx_on(clock, machine, &iid, rid, None, &overrides, &tags)
}

pub(in crate::mcp::tools) fn run_instance_send(
    store: &mut Store,
    clock: &mut dyn Clock,
    args: &Value,
) -> Result<Value, ErrorObj> {
    let iid = str_arg(args, "instance_id").unwrap_or("");
    let rid = str_arg(args, "request_id").unwrap_or("");
    let ev = args
        .get("event")
        .cloned()
        .unwrap_or(Value::Obj(BTreeMap::new()));
    let name = ev
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let mut payload = ev
        .get("payload")
        .cloned()
        .unwrap_or(Value::Obj(BTreeMap::new()));
    let expect = expect_seq_arg(args);
    let stamps: Vec<&str> = args
        .get("stamp")
        .and_then(Value::as_arr)
        .map(|a| a.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    store.send_event_stamp_on(clock, iid, &name, &mut payload, rid, expect, &stamps)
}

pub(in crate::mcp::tools) fn run_deadline_poll(
    store: &mut Store,
    clock: &mut dyn Clock,
    args: &Value,
) -> Result<Value, ErrorObj> {
    let instance_id = str_arg(args, "instance_id").unwrap_or("");
    let request_id = str_arg(args, "request_id").unwrap_or("");
    store.poll_instance_deadline_on(clock, instance_id, request_id, expect_seq_arg(args))
}

pub(in crate::mcp::tools) fn run_effect_ack(
    store: &mut Store,
    clock: &mut dyn Clock,
    args: &Value,
) -> Result<Value, ErrorObj> {
    let iid = str_arg(args, "instance_id").unwrap_or("");
    let eid = str_arg(args, "effect_id").unwrap_or("");
    let rid = str_arg(args, "request_id").unwrap_or("");
    let outcome = str_arg(args, "outcome").unwrap_or("ok");
    let result = args.get("result").cloned();
    store.ack_effect_outcome_on(clock, iid, eid, rid, outcome, result)
}

pub(in crate::mcp::tools) fn run_instance_migrate(
    store: &mut Store,
    clock: &mut dyn Clock,
    args: &Value,
) -> Result<Value, ErrorObj> {
    let instance_id = str_arg(args, "instance_id").unwrap_or("");
    let to_machine = str_arg(args, "to_machine").unwrap_or("");
    if args.get("dry_run").and_then(Value::as_bool) == Some(true) {
        return migration_preview(store, instance_id, to_machine);
    }
    let request_id = str_arg(args, "request_id").unwrap_or("");
    if request_id.is_empty() {
        // The schema cannot require it — a dry run must not carry one — so
        // the writing form checks here, with the same message shape a
        // required argument would have given.
        return Err(ErrorObj::new("args", "request_id is required to migrate")
            .hint("pass request_id, or dry_run: true to ask without writing"));
    }
    let response = store.migrate_instance_on(clock, instance_id, to_machine, request_id)?;
    let mut view = store.instance_view(instance_id, Some(request_id), Some(false))?;
    if let (Value::Obj(fields), Value::Obj(migrated)) = (&mut view, &response) {
        for key in ["from_machine_id", "to_machine_id", "migrated", "seq"] {
            if let Some(value) = migrated.get(key) {
                fields.insert(key.into(), value.clone());
            }
        }
        fields.insert("dry_run".into(), Value::Bool(false));
    }
    Ok(view)
}

/// What the migration would do, answered without writing anything.
fn migration_preview(
    store: &Store,
    instance_id: &str,
    to_machine: &str,
) -> Result<Value, ErrorObj> {
    let target = store.resolve_machine(to_machine)?.clone();
    let from_machine_id = store
        .state
        .instance_machines
        .get(instance_id)
        .cloned()
        .ok_or_else(|| ErrorObj::new("req/instance_not_found", instance_id))?;
    let from = store
        .state
        .machines
        .get(&from_machine_id)
        .cloned()
        .ok_or_else(|| ErrorObj::new("req/machine_not_found", from_machine_id.clone()))?;
    let state = store.state.instances[instance_id].clone();
    let mut budget = fsm_core::expr::eval::Budget::new(fsm_core::limits::MACROSTEP_EVAL_TICKS);
    // A preview reads the clock only to answer "what would the timers become
    // if this happened now", which is the question an operator is asking.
    let now_ms = clock_now();
    let outcome = fsm_core::migrate::preview::preview(
        &from.compiled,
        &target.compiled,
        &target.tree,
        &state,
        now_ms,
        &mut budget,
    );
    let mut out = BTreeMap::new();
    out.insert("ok".into(), Value::Str("true".into()));
    out.insert("dry_run".into(), Value::Bool(true));
    out.insert("instance_id".into(), Value::Str(instance_id.into()));
    out.insert("from_machine_id".into(), Value::Str(from_machine_id));
    out.insert(
        "to_machine_id".into(),
        Value::Str(target.compiled.machine_id.clone()),
    );
    out.insert("would_migrate".into(), Value::Bool(outcome.clean()));
    if let Some(configuration) = &outcome.mapped_configuration {
        out.insert(
            "configuration_mapped".into(),
            fsm_core::hashes::configuration_value(configuration),
        );
    }
    if let Some(configuration) = &outcome.settled_configuration {
        out.insert(
            "configuration_after".into(),
            fsm_core::hashes::configuration_value(configuration),
        );
    }
    // Named apart from the instance view's `context`, which is a map of
    // current values: one field name cannot mean two shapes.
    out.insert(
        "context_changes".into(),
        Value::Arr(
            outcome
                .context
                .iter()
                .map(|(name, before, after)| {
                    let mut entry =
                        BTreeMap::from([("name".to_string(), Value::Str(name.clone()))]);
                    if let Some(before) = before {
                        entry.insert("before".into(), Value::Str(before.clone()));
                    }
                    if let Some(after) = after {
                        entry.insert("after".into(), Value::Str(after.clone()));
                    }
                    Value::Obj(entry)
                })
                .collect(),
        ),
    );
    out.insert(
        "dropped_history".into(),
        Value::Arr(
            outcome
                .report
                .dropped_history
                .iter()
                .cloned()
                .map(Value::Str)
                .collect(),
        ),
    );
    out.insert(
        "rescheduled_deadlines".into(),
        fsm_core::migrate::apply::rescheduled_value(&outcome.report.rescheduled_deadlines),
    );
    out.insert(
        "dropped_slots".into(),
        Value::Arr(
            outcome
                .report
                .dropped_slots
                .iter()
                .cloned()
                .map(Value::Str)
                .collect(),
        ),
    );
    out.insert(
        "retained_effects".into(),
        Value::Arr(
            outcome
                .report
                .retained_effects
                .iter()
                .cloned()
                .map(Value::Str)
                .collect(),
        ),
    );
    // The refusal is data, not a transport failure: a model acts on the code
    // without parsing prose.
    if let Some(rejection) = &outcome.refusal {
        out.insert(
            "refusal".into(),
            Value::Obj(BTreeMap::from([
                ("code".to_string(), Value::Str(rejection.code.into())),
                ("message".into(), Value::Str(rejection.message.clone())),
                ("hint".into(), Value::Str(rejection.hint.clone())),
            ])),
        );
    }
    Ok(Value::Obj(out))
}

/// The wall clock, for a preview's "if this happened now".
fn clock_now() -> i64 {
    crate::clock::Clock::now_ms(&mut crate::clock::SystemClock)
}

pub(in crate::mcp::tools) fn run_invocation_start(
    store: &mut Store,
    clock: &mut dyn Clock,
    args: &Value,
) -> Result<Value, ErrorObj> {
    let parent = str_arg(args, "instance_id").unwrap_or("");
    let slot = str_arg(args, "slot").unwrap_or("");
    let rid = str_arg(args, "request_id").unwrap_or("");
    store.invoke_child_on(clock, parent, slot, rid)
}

pub(in crate::mcp::tools) fn run_invocation_return(
    store: &mut Store,
    clock: &mut dyn Clock,
    args: &Value,
) -> Result<Value, ErrorObj> {
    let parent = str_arg(args, "instance_id").unwrap_or("");
    let slot = str_arg(args, "slot").unwrap_or("");
    let rid = str_arg(args, "request_id").unwrap_or("");
    store.invocation_return_on(clock, parent, slot, rid)
}

pub(in crate::mcp::tools) fn run_signal_deliver(
    store: &mut Store,
    clock: &mut dyn Clock,
    args: &Value,
) -> Result<Value, ErrorObj> {
    let sender = str_arg(args, "instance_id").unwrap_or("");
    let signal_id = str_arg(args, "signal_id").unwrap_or("");
    let rid = str_arg(args, "request_id").unwrap_or("");
    store.signal_deliver_on(clock, sender, signal_id, rid)
}

pub(in crate::mcp::tools) fn run_instance_cancel(
    store: &mut Store,
    clock: &mut dyn Clock,
    args: &Value,
) -> Result<Value, ErrorObj> {
    let iid = str_arg(args, "instance_id").unwrap_or("");
    let reason = str_arg(args, "reason").unwrap_or("");
    let rid = str_arg(args, "request_id").unwrap_or("");
    store.cancel_instance_reason_on(clock, iid, rid, reason)
}

pub(in crate::mcp::tools) fn run_instance_get(
    store: &mut Store,
    _c: &mut dyn Clock,
    args: &Value,
) -> Result<Value, ErrorObj> {
    let iid = str_arg(args, "instance_id").unwrap_or("");
    store.instance_report(iid)
}

pub(in crate::mcp::tools) fn run_instance_list(
    store: &mut Store,
    _c: &mut dyn Clock,
    args: &Value,
) -> Result<Value, ErrorObj> {
    let status = str_arg(args, "status");
    let machine = str_arg(args, "machine");
    let state = str_arg(args, "state");
    let limit = args
        .get("limit")
        .and_then(|v| v.as_num().and_then(|s| s.parse::<usize>().ok()))
        .unwrap_or(50);
    if let Some(mref) = machine {
        store.resolve_machine(mref)?;
    }
    let mut rows = Vec::new();
    let mut next_cursor = None;
    for (id, inst) in &store.state.instances {
        if let Some(st) = status {
            if st != "all" && inst.status.as_str() != st {
                continue;
            }
        }
        if let Some(mref) = machine {
            let m = store.resolve_machine(mref)?;
            if store.state.instance_machines.get(id) != Some(&m.compiled.machine_id)
                && store.state.instance_machines.get(id)
                    != Some(&fsm_core::hashes::machine_id(&m.def))
            {
                continue;
            }
        }
        if let Some(st) = state
            && match &inst.configuration {
                fsm_core::machine::ActiveConfiguration::Sequential { leaf } => leaf != st,
                fsm_core::machine::ActiveConfiguration::Parallel { leaves } => {
                    !leaves.values().any(|leaf| leaf == st)
                }
            }
        {
            continue;
        }
        // The tree filters, applied before the cursor so they compose with
        // pagination rather than replacing it: `parent` is one instance's
        // children, `roots_only` is every instance nobody invoked.
        if let Some(parent) = str_arg(args, "parent")
            && store.parents.get(id).map(|(p, _)| p.as_str()) != Some(parent)
        {
            continue;
        }
        if args.get("roots_only").and_then(Value::as_bool) == Some(true)
            && store.parents.contains_key(id)
        {
            continue;
        }
        if let Some(tag) = str_arg(args, "tag") {
            let tagged = store
                .tags
                .get(id)
                .map(|ts| ts.iter().any(|t| t == tag))
                .unwrap_or(false);
            if !tagged {
                continue;
            }
        }
        if let Some(cur) = str_arg(args, "cursor") {
            if id.as_str() <= cur {
                continue;
            }
        }
        if rows.len() >= limit {
            next_cursor = rows.last().and_then(|r: &Value| {
                r.get("instance_id")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            });
            break;
        }
        let mut row = BTreeMap::new();
        row.insert("instance_id".into(), Value::Str(id.clone()));
        row.insert(
            "configuration".into(),
            fsm_core::hashes::configuration_value(&inst.configuration),
        );
        match &inst.configuration {
            fsm_core::machine::ActiveConfiguration::Sequential { leaf } => {
                row.insert("leaf".into(), Value::Str(leaf.clone()));
                row.insert("state".into(), Value::Str(leaf.clone()));
            }
            fsm_core::machine::ActiveConfiguration::Parallel { leaves } => {
                row.insert(
                    "regions".into(),
                    Value::Obj(
                        leaves
                            .iter()
                            .map(|(region, leaf)| (region.clone(), Value::Str(leaf.clone())))
                            .collect(),
                    ),
                );
            }
        }
        row.insert("status".into(), Value::Str(inst.status.as_str().into()));
        row.insert(
            "created_seq".into(),
            Value::Num(store.created_seq(id).to_string()),
        );
        if let Some((parent, slot)) = store.parents.get(id) {
            row.insert(
                "parent".into(),
                Value::Obj(BTreeMap::from([
                    ("instance_id".into(), Value::Str(parent.clone())),
                    ("slot".into(), Value::Str(slot.clone())),
                ])),
            );
        }
        let mid = store
            .state
            .instance_machines
            .get(id)
            .cloned()
            .unwrap_or_default();
        let machine_name = store
            .state
            .machines
            .get(&mid)
            .map(|m| m.compiled.spec.name.clone())
            .unwrap_or_default();
        row.insert("machine_name".into(), Value::Str(machine_name));
        let seq = store
            .history
            .get(id)
            .and_then(|h| h.last().copied())
            .unwrap_or(0);
        row.insert("seq".into(), Value::Num(seq.to_string()));
        let tags = store
            .tags
            .get(id)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(Value::Str)
            .collect();
        row.insert("tags".into(), Value::Arr(tags));
        rows.push(Value::Obj(row));
    }
    let mut out = BTreeMap::from([("instances".into(), Value::Arr(rows))]);
    if let Some(c) = next_cursor {
        out.insert("next_cursor".into(), Value::Str(c));
    }
    Ok(Value::Obj(out))
}

pub(in crate::mcp::tools) fn run_instance_history(
    store: &mut Store,
    _c: &mut dyn Clock,
    args: &Value,
) -> Result<Value, ErrorObj> {
    let iid = str_arg(args, "instance_id").unwrap_or("");
    let from = args
        .get("from_seq")
        .and_then(|v| v.as_num().and_then(|s| s.parse().ok()))
        .unwrap_or(0u64);
    let limit = args
        .get("limit")
        .and_then(|v| v.as_num().and_then(|s| s.parse().ok()))
        .unwrap_or(50usize)
        .min(500);
    let include_trace = args
        .get("include_trace")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let include_rejected = args
        .get("include_rejected")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    store.history_page(iid, from, limit, include_trace, include_rejected)
}

/// The same page, reporting as it goes.
///
/// A page is assembled in one call, so the reports are about the entries the
/// caller asked for rather than invented steps: one per chunk of the page,
/// with the page's own size as the denominator.
pub(in crate::mcp::tools) fn run_instance_history_with(
    store: &mut Store,
    clock: &mut dyn Clock,
    args: &Value,
    progress: &crate::mcp::progress::ProgressReporter,
) -> Result<Value, ErrorObj> {
    let page = run_instance_history(store, clock, args)?;
    let total = page
        .get("entries")
        .and_then(Value::as_arr)
        .map(<[Value]>::len)
        .unwrap_or(0) as u64;
    const CHUNK: u64 = 10;
    let mut done = 0;
    while done < total {
        done = (done + CHUNK).min(total);
        progress.report(clock.now_ms(), done, Some(total), None, done == total);
    }
    if total == 0 {
        progress.report(clock.now_ms(), 0, Some(0), None, true);
    }
    Ok(page)
}
