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
    let mut v = store.instance_view(iid, None, None)?;
    if let Value::Obj(o) = &mut v {
        if let Some(inst) = store.state.instances.get(iid) {
            let mut h = BTreeMap::new();
            for (k, val) in &inst.history {
                h.insert(k.clone(), Value::Str(val.clone()));
            }
            o.insert("history".into(), Value::Obj(h));
        }
    }
    Ok(v)
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
