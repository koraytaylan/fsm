use std::collections::BTreeMap;

use fsm_core::analyze::{analyze_all, completeness_matrix, reactive_summary};
use fsm_core::diagram::{dot, mermaid};
use fsm_core::json::Value;
use fsm_core::spec::compile_accepted;
use fsm_core::tree::Tree;

use crate::clock::Clock;
use crate::store::{ErrorObj, Store};

use crate::mcp::tools::dispatch::str_arg;

pub(in crate::mcp::tools) fn run_machine_create(
    store: &mut Store,
    clock: &mut dyn Clock,
    args: &Value,
) -> Result<Value, ErrorObj> {
    let spec = args
        .get("spec")
        .cloned()
        .ok_or_else(|| ErrorObj::new("req/args_invalid", "spec"))?;
    let dry = args
        .get("dry_run")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let strict = str_arg(args, "if_exists") == Some("error");
    let o = store.define_machine_on(clock, spec, dry, strict)?;
    let mut m = BTreeMap::new();
    m.insert("machine_id".into(), Value::Str(o.machine_id.clone()));
    m.insert("name".into(), Value::Str(o.name));
    m.insert("created".into(), Value::Bool(o.created));
    m.insert("dry_run".into(), Value::Bool(dry));
    m.insert(
        "warnings".into(),
        Value::Arr(
            o.warnings
                .into_iter()
                .map(|f| Value::Str(f.code.into()))
                .collect(),
        ),
    );
    if let Ok(stored) = store.resolve_machine(&o.machine_id) {
        m.insert("summary".into(), machine_summary(&stored.compiled));
    } else if let Some(spec) = args.get("spec") {
        if let Ok(c) = compile_accepted(spec) {
            m.insert("summary".into(), machine_summary(&c));
        }
    }
    Ok(Value::Obj(m))
}

pub fn machine_summary(c: &fsm_core::machine::CompiledMachine) -> Value {
    let terminals: Vec<Value> = c
        .spec
        .walk_states()
        .into_iter()
        .filter(|(state, _)| state.terminal)
        .map(|(state, _)| Value::Str(state.name.clone()))
        .collect();
    let mut summary = BTreeMap::from([
        (
            "states".into(),
            Value::Num(c.spec.walk_states().len().to_string()),
        ),
        ("events".into(), Value::Num(c.spec.events.len().to_string())),
        (
            "transitions".into(),
            Value::Num(c.spec.transitions.len().to_string()),
        ),
        (
            "deadlines".into(),
            Value::Num(c.spec.deadlines.len().to_string()),
        ),
        ("terminal_states".into(), Value::Arr(terminals)),
    ]);
    match &c.spec.topology {
        fsm_core::spec::Topology::Sequential { initial, .. } => {
            summary.insert("topology".into(), Value::Str("sequential".into()));
            summary.insert("initial".into(), Value::Str(initial.clone()));
            summary.insert("regions".into(), Value::Arr(Vec::new()));
        }
        fsm_core::spec::Topology::Parallel { regions } => {
            summary.insert("topology".into(), Value::Str("parallel".into()));
            summary.insert(
                "regions".into(),
                Value::Arr(
                    regions
                        .iter()
                        .map(|region| {
                            Value::Obj(BTreeMap::from([
                                ("name".into(), Value::Str(region.name.clone())),
                                ("initial".into(), Value::Str(region.initial.clone())),
                                (
                                    "states".into(),
                                    Value::Num(
                                        fsm_core::spec::count_states(&region.states).to_string(),
                                    ),
                                ),
                                (
                                    "terminal_states".into(),
                                    Value::Arr(
                                        fsm_core::spec::terminal_states(&region.states)
                                            .into_iter()
                                            .map(|name| Value::Str(name.to_string()))
                                            .collect(),
                                    ),
                                ),
                            ]))
                        })
                        .collect(),
                ),
            );
        }
    }
    Value::Obj(summary)
}

pub(in crate::mcp::tools) fn run_machine_list(
    store: &mut Store,
    _c: &mut dyn Clock,
    args: &Value,
) -> Result<Value, ErrorObj> {
    let filter = str_arg(args, "name_contains");
    let limit = args
        .get("limit")
        .and_then(|v| v.as_num().and_then(|s| s.parse::<usize>().ok()))
        .unwrap_or(50);
    let cursor = str_arg(args, "cursor");
    let mut rows = Vec::new();
    let mut next_cursor = None;
    for (id, m) in &store.state.machines {
        if let Some(c) = cursor {
            if id.as_str() <= c {
                continue;
            }
        }
        if let Some(f) = filter {
            if !m.compiled.spec.name.contains(f) {
                continue;
            }
        }
        if rows.len() >= limit {
            next_cursor = rows.last().and_then(|r: &Value| {
                r.get("machine_id")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            });
            break;
        }
        let mut row = BTreeMap::new();
        row.insert("machine_id".into(), Value::Str(id.clone()));
        row.insert("name".into(), Value::Str(m.compiled.spec.name.clone()));
        let insts: Vec<_> = store
            .state
            .instance_machines
            .iter()
            .filter(|(_, mid)| *mid == id)
            .map(|(iid, _)| iid)
            .collect();
        let mut running = 0u32;
        let mut completed = 0u32;
        let mut cancelled = 0u32;
        for iid in &insts {
            if let Some(inst) = store.state.instances.get(*iid) {
                match inst.status {
                    fsm_core::machine::Status::Running => running += 1,
                    fsm_core::machine::Status::Completed => completed += 1,
                    fsm_core::machine::Status::Cancelled => cancelled += 1,
                }
            }
        }
        row.insert(
            "instances".into(),
            Value::Obj(BTreeMap::from([
                ("running".into(), Value::Num(running.to_string())),
                ("completed".into(), Value::Num(completed.to_string())),
                ("cancelled".into(), Value::Num(cancelled.to_string())),
            ])),
        );
        row.insert(
            "states".into(),
            Value::Num(m.compiled.spec.walk_states().len().to_string()),
        );
        row.insert(
            "events".into(),
            Value::Num(m.compiled.spec.events.len().to_string()),
        );
        row.insert(
            "deadlines".into(),
            Value::Num(m.compiled.spec.deadlines.len().to_string()),
        );
        let (topology, regions) = match &m.compiled.spec.topology {
            fsm_core::spec::Topology::Sequential { .. } => ("sequential", 0),
            fsm_core::spec::Topology::Parallel { regions } => ("parallel", regions.len()),
        };
        row.insert("topology".into(), Value::Str(topology.into()));
        row.insert("regions".into(), Value::Num(regions.to_string()));
        let defined_seq = store
            .records
            .iter()
            .find(|r| r.body.get("machine_id").and_then(Value::as_str) == Some(id.as_str()))
            .map(|r| r.seq)
            .unwrap_or(0);
        row.insert("defined_seq".into(), Value::Num(defined_seq.to_string()));
        rows.push(Value::Obj(row));
    }
    let mut out = BTreeMap::from([("machines".into(), Value::Arr(rows))]);
    if let Some(c) = next_cursor {
        out.insert("next_cursor".into(), Value::Str(c));
    }
    Ok(Value::Obj(out))
}

pub(in crate::mcp::tools) fn run_machine_get(
    store: &mut Store,
    _c: &mut dyn Clock,
    args: &Value,
) -> Result<Value, ErrorObj> {
    let r = str_arg(args, "machine").unwrap_or("");
    let m = store.resolve_machine(r)?;
    let mut o = BTreeMap::new();
    o.insert(
        "machine_id".into(),
        Value::Str(fsm_core::hashes::machine_id(&m.def)),
    );
    o.insert("name".into(), Value::Str(m.compiled.spec.name.clone()));
    o.insert("spec".into(), m.def.clone());
    o.insert("summary".into(), machine_summary(&m.compiled));
    Ok(Value::Obj(o))
}

pub(in crate::mcp::tools) fn run_machine_analyze(
    store: &mut Store,
    _c: &mut dyn Clock,
    args: &Value,
) -> Result<Value, ErrorObj> {
    let r = str_arg(args, "machine").unwrap_or("");
    let m = store.resolve_machine(r)?;
    let t = Tree::for_machine(&m.compiled.spec);
    let findings = analyze_all(&m.compiled, &t);
    let matrix = completeness_matrix(&m.compiled, &t);
    let flist: Vec<Value> = findings
        .into_iter()
        .map(|f| {
            let mut o = BTreeMap::new();
            o.insert(
                "severity".into(),
                Value::Str(format!("{:?}", f.severity).to_lowercase()),
            );
            o.insert("code".into(), Value::Str(f.code.into()));
            o.insert("message".into(), Value::Str(f.message));
            o.insert("path".into(), Value::Str(f.path));
            o.insert("hint".into(), Value::Str(f.hint));
            Value::Obj(o)
        })
        .collect();
    let mut cells = BTreeMap::new();
    for ((leaf, ev), cell) in matrix {
        cells.insert(format!("{leaf}/{ev}"), Value::Str(cell));
    }
    let reach = fsm_core::analyze::enterable(&m.compiled, &t);
    let all: std::collections::BTreeSet<String> = t.names.iter().cloned().collect();
    let unenterable: Vec<Value> = all.difference(&reach).cloned().map(Value::Str).collect();
    let shadow: Vec<Value> = fsm_core::analyze::shadowing_findings(&m.compiled)
        .into_iter()
        .map(|f| Value::Str(f.code.into()))
        .collect();
    let reactive = reactive_summary(&m.compiled, &t);
    let names = |names: Vec<String>| Value::Arr(names.into_iter().map(Value::Str).collect());
    Ok(Value::Obj(BTreeMap::from([
        (
            "machine_id".into(),
            Value::Str(fsm_core::hashes::machine_id(&m.def)),
        ),
        ("findings".into(), Value::Arr(flist)),
        (
            "completeness".into(),
            Value::Obj(BTreeMap::from([("by_leaf".into(), Value::Obj(cells))])),
        ),
        (
            "reachability".into(),
            Value::Obj(BTreeMap::from([(
                "unenterable".into(),
                Value::Arr(unenterable),
            )])),
        ),
        ("shadowing".into(), Value::Arr(shadow)),
        // A count only: the eventless cycle and depth findings are already
        // in `findings`, where `analyze_all` reports them.
        (
            "eventless_transitions".into(),
            Value::Num(reactive.eventless_transitions.to_string()),
        ),
        ("done_events".into(), names(reactive.done_events)),
        (
            "unhandled_done_events".into(),
            names(reactive.unhandled_done_events),
        ),
        ("internal_events".into(), names(reactive.internal_events)),
    ])))
}

pub(in crate::mcp::tools) fn run_machine_diagram(
    store: &mut Store,
    _c: &mut dyn Clock,
    args: &Value,
) -> Result<Value, ErrorObj> {
    let r = str_arg(args, "machine").unwrap_or("");
    let fmt = str_arg(args, "format").unwrap_or("mermaid");
    let m = store.resolve_machine(r)?;
    let overlay = if let Some(iid) = str_arg(args, "instance") {
        let inst = store
            .state
            .instances
            .get(iid)
            .ok_or_else(|| ErrorObj::new("req/instance_not_found", iid))?;
        let current_leaves = match &inst.configuration {
            fsm_core::machine::ActiveConfiguration::Sequential { leaf } => {
                std::collections::BTreeSet::from([leaf.clone()])
            }
            fsm_core::machine::ActiveConfiguration::Parallel { leaves } => {
                leaves.values().cloned().collect()
            }
        };
        Some(fsm_core::diagram::InstanceOverlay {
            visited: current_leaves.clone(),
            current_leaves,
        })
    } else {
        None
    };
    let text = if fmt == "dot" {
        dot(&m.compiled, overlay.as_ref())
    } else {
        mermaid(&m.compiled, overlay.as_ref())
    };
    Ok(Value::Obj(BTreeMap::from([
        ("format".into(), Value::Str(fmt.into())),
        ("diagram".into(), Value::Str(text)),
    ])))
}
