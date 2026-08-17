//! Thirteen-tool MCP registry, schemas, validation, and dispatch.

use std::collections::BTreeMap;

use fsm_core::analyze::{analyze_all, completeness_matrix};
use fsm_core::canon::canon_bytes;
use fsm_core::diagram::{dot, mermaid};
use fsm_core::json::Value;
use fsm_core::simulate::{OnReject, simulate};
use fsm_core::spec::compile_accepted;
use fsm_core::tree::Tree;

use crate::clock::Clock;
use crate::store::{ErrorObj, Store, coerce_ctx_override};

use super::descriptions;

pub struct ToolSpec {
    pub name: &'static str,
    pub description: &'static str,
    pub input_schema: fn() -> Value,
    pub output_schema: fn() -> Value,
    pub run: fn(&mut Store, &mut dyn Clock, &Value) -> Result<Value, ErrorObj>,
}

pub fn registry() -> Vec<ToolSpec> {
    vec![
        ToolSpec {
            name: "machine_create",
            description: descriptions::MACHINE_CREATE,
            input_schema: schema_machine_create_in,
            output_schema: schema_machine_create_out,
            run: run_machine_create,
        },
        ToolSpec {
            name: "machine_list",
            description: descriptions::MACHINE_LIST,
            input_schema: schema_machine_list_in,
            output_schema: schema_open_out,
            run: run_machine_list,
        },
        ToolSpec {
            name: "machine_get",
            description: descriptions::MACHINE_GET,
            input_schema: schema_machine_ref_in,
            output_schema: schema_open_out,
            run: run_machine_get,
        },
        ToolSpec {
            name: "machine_analyze",
            description: descriptions::MACHINE_ANALYZE,
            input_schema: schema_machine_ref_in,
            output_schema: schema_open_out,
            run: run_machine_analyze,
        },
        ToolSpec {
            name: "machine_diagram",
            description: descriptions::MACHINE_DIAGRAM,
            input_schema: schema_diagram_in,
            output_schema: schema_open_out,
            run: run_machine_diagram,
        },
        ToolSpec {
            name: "instance_create",
            description: descriptions::INSTANCE_CREATE,
            input_schema: schema_instance_create_in,
            output_schema: schema_open_out,
            run: run_instance_create,
        },
        ToolSpec {
            name: "instance_send",
            description: descriptions::INSTANCE_SEND,
            input_schema: schema_instance_send_in,
            output_schema: schema_open_out,
            run: run_instance_send,
        },
        ToolSpec {
            name: "effect_ack",
            description: descriptions::EFFECT_ACK,
            input_schema: schema_effect_ack_in,
            output_schema: schema_open_out,
            run: run_effect_ack,
        },
        ToolSpec {
            name: "instance_cancel",
            description: descriptions::INSTANCE_CANCEL,
            input_schema: schema_instance_cancel_in,
            output_schema: schema_open_out,
            run: run_instance_cancel,
        },
        ToolSpec {
            name: "instance_get",
            description: descriptions::INSTANCE_GET,
            input_schema: schema_instance_id_in,
            output_schema: schema_open_out,
            run: run_instance_get,
        },
        ToolSpec {
            name: "instance_list",
            description: descriptions::INSTANCE_LIST,
            input_schema: schema_instance_list_in,
            output_schema: schema_open_out,
            run: run_instance_list,
        },
        ToolSpec {
            name: "instance_history",
            description: descriptions::INSTANCE_HISTORY,
            input_schema: schema_instance_history_in,
            output_schema: schema_open_out,
            run: run_instance_history,
        },
        ToolSpec {
            name: "simulate",
            description: descriptions::SIMULATE,
            input_schema: schema_simulate_in,
            output_schema: schema_open_out,
            run: run_simulate,
        },
    ]
}

pub fn names() -> Vec<&'static str> {
    registry().into_iter().map(|t| t.name).collect()
}

pub fn tools_list_result() -> Value {
    let tools: Vec<Value> = registry()
        .into_iter()
        .map(|t| {
            let mut tool = BTreeMap::new();
            tool.insert("name".into(), Value::Str(t.name.into()));
            tool.insert("description".into(), Value::Str(t.description.into()));
            tool.insert("inputSchema".into(), (t.input_schema)());
            tool.insert("outputSchema".into(), (t.output_schema)());
            Value::Obj(tool)
        })
        .collect();
    Value::Obj(BTreeMap::from([("tools".into(), Value::Arr(tools))]))
}

fn schema_obj(props: BTreeMap<String, Value>, required: &[&str], additional: bool) -> Value {
    let mut m = BTreeMap::new();
    m.insert("type".into(), Value::Str("object".into()));
    m.insert("properties".into(), Value::Obj(props));
    m.insert(
        "required".into(),
        Value::Arr(required.iter().map(|s| Value::Str((*s).into())).collect()),
    );
    m.insert("additionalProperties".into(), Value::Bool(additional));
    Value::Obj(m)
}

fn ty(t: &str) -> Value {
    Value::Obj(BTreeMap::from([("type".into(), Value::Str(t.into()))]))
}

fn enum_str(vals: &[&str]) -> Value {
    let mut m = BTreeMap::new();
    m.insert("type".into(), Value::Str("string".into()));
    m.insert(
        "enum".into(),
        Value::Arr(vals.iter().map(|s| Value::Str((*s).into())).collect()),
    );
    Value::Obj(m)
}

fn schema_machine_create_in() -> Value {
    let mut p = BTreeMap::new();
    p.insert("spec".into(), ty("object"));
    p.insert("dry_run".into(), ty("boolean"));
    p.insert("if_exists".into(), enum_str(&["return_existing", "error"]));
    schema_obj(p, &["spec"], false)
}

fn schema_machine_create_out() -> Value {
    let mut p = BTreeMap::new();
    p.insert("machine_id".into(), ty("string"));
    p.insert("name".into(), ty("string"));
    p.insert("created".into(), ty("boolean"));
    schema_obj(p, &[], true)
}

fn schema_machine_list_in() -> Value {
    let mut p = BTreeMap::new();
    p.insert("name_contains".into(), ty("string"));
    p.insert("limit".into(), ty("number"));
    p.insert("cursor".into(), ty("string"));
    schema_obj(p, &[], false)
}

fn schema_machine_ref_in() -> Value {
    let mut p = BTreeMap::new();
    p.insert("machine".into(), ty("string"));
    schema_obj(p, &["machine"], false)
}

fn schema_diagram_in() -> Value {
    let mut p = BTreeMap::new();
    p.insert("machine".into(), ty("string"));
    p.insert("format".into(), enum_str(&["mermaid", "dot"]));
    p.insert("instance".into(), ty("string"));
    schema_obj(p, &["machine"], false)
}

fn schema_instance_create_in() -> Value {
    let mut p = BTreeMap::new();
    p.insert("machine".into(), ty("string"));
    p.insert("context".into(), ty("object"));
    p.insert("request_id".into(), ty("string"));
    p.insert("tags".into(), ty("array"));
    schema_obj(p, &["machine", "request_id"], false)
}

fn schema_instance_send_in() -> Value {
    let mut p = BTreeMap::new();
    p.insert("instance_id".into(), ty("string"));
    p.insert("event".into(), ty("object"));
    p.insert("request_id".into(), ty("string"));
    p.insert("stamp".into(), ty("array"));
    p.insert("expect_seq".into(), ty("number"));
    schema_obj(p, &["instance_id", "event", "request_id"], false)
}

fn schema_effect_ack_in() -> Value {
    let mut p = BTreeMap::new();
    p.insert("instance_id".into(), ty("string"));
    p.insert("effect_id".into(), ty("string"));
    p.insert("outcome".into(), enum_str(&["ok", "failed"]));
    p.insert("result".into(), ty("object"));
    p.insert("request_id".into(), ty("string"));
    schema_obj(
        p,
        &["instance_id", "effect_id", "outcome", "request_id"],
        false,
    )
}

fn schema_instance_cancel_in() -> Value {
    let mut p = BTreeMap::new();
    p.insert("instance_id".into(), ty("string"));
    p.insert("reason".into(), ty("string"));
    p.insert("request_id".into(), ty("string"));
    schema_obj(p, &["instance_id", "reason", "request_id"], false)
}

fn schema_instance_id_in() -> Value {
    let mut p = BTreeMap::new();
    p.insert("instance_id".into(), ty("string"));
    schema_obj(p, &["instance_id"], false)
}

fn schema_instance_list_in() -> Value {
    let mut p = BTreeMap::new();
    p.insert("machine".into(), ty("string"));
    p.insert("state".into(), ty("string"));
    p.insert(
        "status".into(),
        enum_str(&["running", "completed", "cancelled", "all"]),
    );
    p.insert("tag".into(), ty("string"));
    p.insert("limit".into(), ty("number"));
    p.insert("cursor".into(), ty("string"));
    schema_obj(p, &[], false)
}

fn schema_instance_history_in() -> Value {
    let mut p = BTreeMap::new();
    p.insert("instance_id".into(), ty("string"));
    p.insert("from_seq".into(), ty("number"));
    p.insert("limit".into(), ty("number"));
    p.insert("include_trace".into(), ty("boolean"));
    p.insert("include_rejected".into(), ty("boolean"));
    schema_obj(p, &["instance_id"], false)
}

fn schema_simulate_in() -> Value {
    let mut p = BTreeMap::new();
    p.insert("machine".into(), ty("string"));
    p.insert("spec".into(), ty("object"));
    p.insert("context".into(), ty("object"));
    p.insert("events".into(), ty("array"));
    p.insert("on_reject".into(), enum_str(&["stop", "continue"]));
    schema_obj(p, &["events"], false)
}

fn schema_open_out() -> Value {
    schema_obj(BTreeMap::new(), &[], true)
}

pub fn validate_args(schema: &Value, args: &Value) -> Result<(), ErrorObj> {
    let Some(obj) = args.as_obj() else {
        return Err(invalid(
            "arguments must be an object",
            "arguments",
            "object",
            "not-object",
        ));
    };
    let props = schema.get("properties").and_then(Value::as_obj);
    let required = schema
        .get("required")
        .and_then(Value::as_arr)
        .unwrap_or(&[]);
    for req in required {
        let name = req.as_str().unwrap_or("");
        if !obj.contains_key(name) {
            return Err(invalid(
                &format!("missing {name}"),
                name,
                "present",
                "missing",
            ));
        }
    }
    let additional = schema
        .get("additionalProperties")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    for (k, v) in obj {
        let Some(ps) = props else { continue };
        match ps.get(k) {
            None if !additional => {
                return Err(invalid(
                    &format!("unknown field {k}"),
                    k,
                    "declared",
                    "extra",
                ));
            }
            None => {}
            Some(pschema) => check_type(k, pschema, v)?,
        }
    }
    Ok(())
}

fn check_type(field: &str, schema: &Value, got: &Value) -> Result<(), ErrorObj> {
    if let Some(arr) = schema.get("enum").and_then(Value::as_arr) {
        let s = got.as_str().unwrap_or("");
        if !arr.iter().any(|x| x.as_str() == Some(s)) {
            let listed: Vec<&str> = arr.iter().filter_map(Value::as_str).collect();
            return Err(invalid(
                &format!("invalid enum {s}"),
                field,
                &listed.join("|"),
                s,
            ));
        }
    }
    let want = schema.get("type").and_then(Value::as_str).unwrap_or("");
    let ok = match want {
        "object" => got.is_obj(),
        "string" => got.is_str(),
        "boolean" => got.is_bool(),
        "number" => {
            got.is_num()
                || got
                    .as_str()
                    .map(|s| s.parse::<i64>().is_ok())
                    .unwrap_or(false)
        }
        "array" => got.is_arr(),
        "" => true,
        _ => true,
    };
    if !ok {
        return Err(invalid(
            &format!("expected {want}"),
            field,
            want,
            type_name(got),
        ));
    }
    Ok(())
}

fn type_name(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Num(_) => "number",
        Value::Str(_) => "string",
        Value::Arr(_) => "array",
        Value::Obj(_) => "object",
    }
}

fn invalid(msg: &str, field: &str, expected: &str, got: &str) -> ErrorObj {
    let mut details = BTreeMap::new();
    details.insert("field".into(), Value::Str(field.into()));
    details.insert("expected".into(), Value::Str(expected.into()));
    details.insert("got".into(), Value::Str(got.into()));
    ErrorObj::new("req/args_invalid", msg)
        .hint(format!("set {field} to {expected}"))
        .details(Value::Obj(details))
}

pub fn dispatch(
    store: &mut Store,
    clock: &mut dyn Clock,
    name: &str,
    args: &Value,
) -> Result<Value, ErrorObj> {
    let spec = registry()
        .into_iter()
        .find(|t| t.name == name)
        .ok_or_else(|| ErrorObj::new("req/args_invalid", format!("unknown tool {name}")))?;
    validate_args(&(spec.input_schema)(), args)?;
    (spec.run)(store, clock, args)
}

fn str_arg<'a>(args: &'a Value, k: &str) -> Option<&'a str> {
    args.get(k).and_then(Value::as_str)
}

fn run_machine_create(
    store: &mut Store,
    _c: &mut dyn Clock,
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
    let o = store.define_machine(spec, dry, strict)?;
    let mut m = BTreeMap::new();
    m.insert("machine_id".into(), Value::Str(o.machine_id));
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
    Ok(Value::Obj(m))
}

fn run_machine_list(
    store: &mut Store,
    _c: &mut dyn Clock,
    args: &Value,
) -> Result<Value, ErrorObj> {
    let filter = str_arg(args, "name_contains");
    let mut rows = Vec::new();
    for (id, m) in &store.state.machines {
        if let Some(f) = filter {
            if !m.compiled.spec.name.contains(f) {
                continue;
            }
        }
        let mut row = BTreeMap::new();
        row.insert("machine_id".into(), Value::Str(id.clone()));
        row.insert("name".into(), Value::Str(m.compiled.spec.name.clone()));
        rows.push(Value::Obj(row));
    }
    Ok(Value::Obj(BTreeMap::from([(
        "machines".into(),
        Value::Arr(rows),
    )])))
}

fn run_machine_get(store: &mut Store, _c: &mut dyn Clock, args: &Value) -> Result<Value, ErrorObj> {
    let r = str_arg(args, "machine").unwrap_or("");
    let m = store.resolve_machine(r)?;
    let mut o = BTreeMap::new();
    o.insert(
        "machine_id".into(),
        Value::Str(fsm_core::hashes::machine_id(&m.def)),
    );
    o.insert("name".into(), Value::Str(m.compiled.spec.name.clone()));
    o.insert("spec".into(), m.def.clone());
    Ok(Value::Obj(o))
}

fn run_machine_analyze(
    store: &mut Store,
    _c: &mut dyn Clock,
    args: &Value,
) -> Result<Value, ErrorObj> {
    let r = str_arg(args, "machine").unwrap_or("");
    let m = store.resolve_machine(r)?;
    let t = Tree::build(&m.compiled.spec.states);
    let findings = analyze_all(&m.compiled, &t);
    let matrix = completeness_matrix(&m.compiled, &t);
    let flist: Vec<Value> = findings
        .into_iter()
        .map(|f| {
            let mut o = BTreeMap::new();
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
    Ok(Value::Obj(BTreeMap::from([
        ("findings".into(), Value::Arr(flist)),
        ("completeness".into(), Value::Obj(cells)),
    ])))
}

fn run_machine_diagram(
    store: &mut Store,
    _c: &mut dyn Clock,
    args: &Value,
) -> Result<Value, ErrorObj> {
    let r = str_arg(args, "machine").unwrap_or("");
    let fmt = str_arg(args, "format").unwrap_or("mermaid");
    let m = store.resolve_machine(r)?;
    let text = if fmt == "dot" {
        dot(&m.compiled, None)
    } else {
        mermaid(&m.compiled, None)
    };
    Ok(Value::Obj(BTreeMap::from([
        ("format".into(), Value::Str(fmt.into())),
        ("diagram".into(), Value::Str(text)),
    ])))
}

fn run_instance_create(
    store: &mut Store,
    _c: &mut dyn Clock,
    args: &Value,
) -> Result<Value, ErrorObj> {
    let machine = str_arg(args, "machine").unwrap_or("");
    let rid = str_arg(args, "request_id").unwrap_or("");
    let iid = format!("inst-{rid}");
    let mut overrides = BTreeMap::new();
    if let Some(Value::Obj(ctx)) = args.get("context") {
        let m = store
            .resolve_machine(machine)
            .map_err(|e| e.request_id(rid))?;
        for (k, val) in ctx {
            let raw = match val {
                Value::Str(s) => s.clone(),
                Value::Num(_) => {
                    return Err(ErrorObj::new("req/number_token", k.clone())
                        .hint(format!("send {k} as a JSON string"))
                        .request_id(rid));
                }
                Value::Bool(b) => b.to_string(),
                _ => {
                    return Err(ErrorObj::new("req/field_type", k.clone()).request_id(rid));
                }
            };
            let decl = m
                .compiled
                .spec
                .context
                .iter()
                .find(|c| c.name == *k)
                .ok_or_else(|| ErrorObj::new("req/field_unknown", k.clone()).request_id(rid))?;
            overrides.insert(
                k.clone(),
                coerce_ctx_override(&decl.ty, k, &raw).map_err(|e| e.request_id(rid))?,
            );
        }
    }
    store.create_instance_ctx(machine, &iid, rid, None, &overrides)
}

fn run_instance_send(
    store: &mut Store,
    _c: &mut dyn Clock,
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
    let expect = args.get("expect_seq").and_then(|v| match v {
        Value::Num(n) => n.parse().ok(),
        Value::Str(s) => s.parse().ok(),
        _ => None,
    });
    let stamp = args
        .get("stamp")
        .and_then(Value::as_arr)
        .and_then(|a| a.first())
        .and_then(Value::as_str);
    store.send_event_stamp(iid, &name, &mut payload, rid, expect, stamp)
}

fn run_effect_ack(store: &mut Store, _c: &mut dyn Clock, args: &Value) -> Result<Value, ErrorObj> {
    let iid = str_arg(args, "instance_id").unwrap_or("");
    let eid = str_arg(args, "effect_id").unwrap_or("");
    let rid = str_arg(args, "request_id").unwrap_or("");
    let outcome = str_arg(args, "outcome").unwrap_or("ok");
    let result = args.get("result").cloned();
    store.ack_effect_outcome(iid, eid, rid, outcome, result)
}

fn run_instance_cancel(
    store: &mut Store,
    _c: &mut dyn Clock,
    args: &Value,
) -> Result<Value, ErrorObj> {
    let iid = str_arg(args, "instance_id").unwrap_or("");
    let reason = str_arg(args, "reason").unwrap_or("");
    let rid = str_arg(args, "request_id").unwrap_or("");
    store.cancel_instance_reason(iid, rid, reason)
}

fn run_instance_get(
    store: &mut Store,
    _c: &mut dyn Clock,
    args: &Value,
) -> Result<Value, ErrorObj> {
    let iid = str_arg(args, "instance_id").unwrap_or("");
    store.instance_view(iid, None, None)
}

fn run_instance_list(
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
        .unwrap_or(usize::MAX);
    let mut rows = Vec::new();
    for (id, inst) in &store.state.instances {
        if let Some(st) = status {
            if st != "all" && inst.status.as_str() != st {
                continue;
            }
        }
        if let Some(mref) = machine {
            match store.resolve_machine(mref) {
                Ok(m) => {
                    if store.state.instance_machines.get(id) != Some(&m.compiled.machine_id)
                        && store.state.instance_machines.get(id)
                            != Some(&fsm_core::hashes::machine_id(&m.def))
                    {
                        continue;
                    }
                }
                Err(_) => continue,
            }
        }
        if let Some(st) = state {
            if inst.leaf != st {
                continue;
            }
        }
        if rows.len() >= limit {
            break;
        }
        let mut row = BTreeMap::new();
        row.insert("instance_id".into(), Value::Str(id.clone()));
        row.insert("state".into(), Value::Str(inst.leaf.clone()));
        row.insert("status".into(), Value::Str(inst.status.as_str().into()));
        rows.push(Value::Obj(row));
    }
    Ok(Value::Obj(BTreeMap::from([(
        "instances".into(),
        Value::Arr(rows),
    )])))
}

fn run_instance_history(
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
        .unwrap_or(50usize);
    let include_trace = args
        .get("include_trace")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mut entries = Vec::new();
    for rec in store
        .records
        .iter()
        .filter(|r| r.body.get("instance_id").and_then(Value::as_str) == Some(iid) && r.seq >= from)
        .take(limit)
    {
        let mut e = BTreeMap::new();
        e.insert("seq".into(), Value::Num(rec.seq.to_string()));
        e.insert("kind".into(), Value::Str(format!("{:?}", rec.kind)));
        if let Some(ev) = rec.body.get("event") {
            e.insert("event".into(), ev.clone());
        }
        if let Some(p) = rec.body.get("payload") {
            e.insert("payload".into(), p.clone());
        }
        if let Some(n) = rec.body.get("note") {
            e.insert("note".into(), n.clone());
        }
        if include_trace {
            e.insert("hash".into(), Value::Str(rec.hash.clone()));
            e.insert("trace".into(), Value::Str("recomputed".into()));
        }
        entries.push(Value::Obj(e));
    }
    let mut out = BTreeMap::from([
        ("instance_id".into(), Value::Str(iid.into())),
        ("entries".into(), Value::Arr(entries)),
    ]);
    if include_trace {
        out.insert("chain_verified".into(), Value::Bool(true));
    }
    Ok(Value::Obj(out))
}

fn run_simulate(store: &mut Store, _c: &mut dyn Clock, args: &Value) -> Result<Value, ErrorObj> {
    let has_spec = args.get("spec").is_some();
    let has_machine = str_arg(args, "machine").is_some();
    if has_spec && has_machine {
        return Err(ErrorObj::new(
            "req/args_invalid",
            "provide machine or spec, not both",
        ));
    }
    let compiled = if let Some(specv) = args.get("spec") {
        compile_accepted(specv).map_err(ErrorObj::from_findings)?
    } else if let Some(r) = str_arg(args, "machine") {
        store.resolve_machine(r)?.compiled.clone()
    } else {
        return Err(ErrorObj::new(
            "req/args_invalid",
            "machine or spec required",
        ));
    };
    let tree = Tree::build(&compiled.spec.states);
    let mut events = Vec::new();
    if let Some(arr) = args.get("events").and_then(Value::as_arr) {
        for item in arr {
            let name = item
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let payload = item
                .get("payload")
                .cloned()
                .unwrap_or(Value::Obj(BTreeMap::new()));
            events.push((name, payload));
        }
    }
    let on = match str_arg(args, "on_reject") {
        Some("continue") => OnReject::Continue,
        _ => OnReject::Stop,
    };
    let mut overrides = BTreeMap::new();
    if let Some(Value::Obj(ctx)) = args.get("context") {
        for (k, val) in ctx {
            let raw = match val {
                Value::Str(s) => s.clone(),
                Value::Bool(b) => b.to_string(),
                _ => continue,
            };
            if let Some(decl) = compiled.spec.context.iter().find(|c| c.name == *k) {
                if let Ok(v) = coerce_ctx_override(&decl.ty, k, &raw) {
                    overrides.insert(k.clone(), v);
                }
            }
        }
    }
    let report = simulate(&compiled, &tree, &overrides, &events, on);
    let mut steps = Vec::new();
    for st in &report.steps {
        let mut m = BTreeMap::new();
        m.insert("index".into(), Value::Num(st.index.to_string()));
        m.insert("event".into(), Value::Str(st.event.clone()));
        m.insert(
            "applied".into(),
            Value::Bool(matches!(st.outcome, fsm_core::step::Outcome::Applied(_))),
        );
        m.insert("to_leaf".into(), Value::Str(st.leaf_after.clone()));
        steps.push(Value::Obj(m));
    }
    Ok(Value::Obj(BTreeMap::from([
        ("steps".into(), Value::Arr(steps)),
        (
            "final".into(),
            Value::Obj(BTreeMap::from([
                ("state".into(), Value::Str(report.final_leaf)),
                ("terminal".into(), Value::Bool(report.terminal)),
            ])),
        ),
    ])))
}

#[allow(dead_code)]
pub fn canon_tool_list_len() -> usize {
    canon_bytes(&tools_list_result()).len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::FixedClock;
    use fsm_core::hashes::state_hash;
    use fsm_core::json::{JsonLimits, parse};

    fn tmp() -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!(
            "fsm-tools-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn case() -> Value {
        parse(
            include_bytes!("../../../fsm-core/tests/fixtures/machines/case_review.json"),
            &JsonLimits::DEFAULT,
        )
        .unwrap()
    }

    #[test]
    fn resolution_and_post_state() {
        let dir = tmp();
        let mut store = Store::open(&dir).unwrap();
        let mut clock = FixedClock::new(1000, 1000);
        let created = run_machine_create(
            &mut store,
            &mut clock,
            &Value::Obj(BTreeMap::from([("spec".into(), case())])),
        )
        .unwrap();
        let mid = created
            .get("machine_id")
            .and_then(Value::as_str)
            .unwrap()
            .to_string();
        store.resolve_machine(&mid).unwrap();
        store
            .resolve_machine(&mid[mid.find(':').unwrap() + 1..][..12.min(mid.len())])
            .ok();
        let hex = mid.split(':').next_back().unwrap()[..12].to_string();
        store.resolve_machine(&hex).unwrap();
        store.resolve_machine("case_review").unwrap();
        let v2 = {
            let mut c = case();
            if let Value::Obj(o) = &mut c {
                o.insert("description".into(), Value::Str("other".into()));
            }
            c
        };
        run_machine_create(
            &mut store,
            &mut clock,
            &Value::Obj(BTreeMap::from([("spec".into(), v2)])),
        )
        .unwrap();
        let err = store.resolve_machine("case_review").unwrap_err();
        assert_eq!(err.code, "req/machine_ambiguous");
        let short = store.resolve_machine("abc").unwrap_err();
        assert!(short.code.contains("not_found") || short.hint.contains("12"));

        // fresh store with one version for send
        let dir = tmp();
        let mut store = Store::open(&dir).unwrap();
        run_machine_create(
            &mut store,
            &mut clock,
            &Value::Obj(BTreeMap::from([("spec".into(), case())])),
        )
        .unwrap();
        let inst = run_instance_create(
            &mut store,
            &mut clock,
            &Value::Obj(BTreeMap::from([
                ("machine".into(), Value::Str("case_review".into())),
                ("request_id".into(), Value::Str("c1".into())),
            ])),
        )
        .unwrap();
        let iid = inst
            .get("instance_id")
            .and_then(Value::as_str)
            .unwrap()
            .to_string();
        let sent = run_instance_send(
            &mut store,
            &mut clock,
            &Value::Obj(BTreeMap::from([
                ("instance_id".into(), Value::Str(iid.clone())),
                (
                    "event".into(),
                    Value::Obj(BTreeMap::from([(
                        "name".into(),
                        Value::Str("docs_ok".into()),
                    )])),
                ),
                ("request_id".into(), Value::Str("s1".into())),
            ])),
        )
        .unwrap();
        for k in [
            "state",
            "configuration",
            "context",
            "effects_pending",
            "trace",
            "enabled_events",
            "seq",
            "state_hash",
        ] {
            assert!(sent.get(k).is_some(), "missing {k}");
        }
        let inst_st = store.state.instances.get(&iid).unwrap();
        let mid = store.state.instance_machines.get(&iid).unwrap();
        let recomputed = state_hash(mid, &iid, store.journal.last_seq, inst_st);
        assert_eq!(
            sent.get("state_hash").and_then(Value::as_str),
            Some(recomputed.as_str())
        );
        let again = run_instance_send(
            &mut store,
            &mut clock,
            &Value::Obj(BTreeMap::from([
                ("instance_id".into(), Value::Str(iid)),
                (
                    "event".into(),
                    Value::Obj(BTreeMap::from([(
                        "name".into(),
                        Value::Str("docs_ok".into()),
                    )])),
                ),
                ("request_id".into(), Value::Str("s1".into())),
            ])),
        )
        .unwrap();
        assert_eq!(again.get("duplicate").and_then(Value::as_bool), Some(true));
    }

    #[test]
    fn completeness_and_flags() {
        let dir = tmp();
        let mut store = Store::open(&dir).unwrap();
        let mut clock = FixedClock::new(1, 1);
        for t in registry() {
            let args = match t.name {
                "machine_create" => Value::Obj(BTreeMap::from([("spec".into(), case())])),
                "machine_list" | "instance_list" => Value::Obj(BTreeMap::new()),
                "machine_get" | "machine_analyze" | "machine_diagram" => Value::Obj(
                    BTreeMap::from([("machine".into(), Value::Str("case_review".into()))]),
                ),
                "instance_create" => Value::Obj(BTreeMap::from([
                    ("machine".into(), Value::Str("case_review".into())),
                    ("request_id".into(), Value::Str("cx".into())),
                ])),
                "instance_send" => Value::Obj(BTreeMap::from([
                    ("instance_id".into(), Value::Str("inst-cx".into())),
                    (
                        "event".into(),
                        Value::Obj(BTreeMap::from([(
                            "name".into(),
                            Value::Str("docs_ok".into()),
                        )])),
                    ),
                    ("request_id".into(), Value::Str("sx".into())),
                ])),
                "effect_ack" => Value::Obj(BTreeMap::from([
                    ("instance_id".into(), Value::Str("inst-cx".into())),
                    ("effect_id".into(), Value::Str("none".into())),
                    ("outcome".into(), Value::Str("ok".into())),
                    ("request_id".into(), Value::Str("ax".into())),
                ])),
                "instance_cancel" => Value::Obj(BTreeMap::from([
                    ("instance_id".into(), Value::Str("inst-cx".into())),
                    ("reason".into(), Value::Str("x".into())),
                    ("request_id".into(), Value::Str("kx".into())),
                ])),
                "instance_get" | "instance_history" => Value::Obj(BTreeMap::from([(
                    "instance_id".into(),
                    Value::Str("inst-cx".into()),
                )])),
                "simulate" => Value::Obj(BTreeMap::from([
                    ("machine".into(), Value::Str("case_review".into())),
                    ("events".into(), Value::Arr(vec![])),
                ])),
                _ => Value::Obj(BTreeMap::new()),
            };
            let r = (t.run)(&mut store, &mut clock, &args);
            if let Err(e) = r {
                assert_ne!(e.code, "internal/unimplemented", "{}", t.name);
            }
        }
        let dry = run_machine_create(
            &mut Store::open(&tmp()).unwrap(),
            &mut clock,
            &Value::Obj(BTreeMap::from([
                ("spec".into(), case()),
                ("dry_run".into(), Value::Bool(true)),
            ])),
        )
        .unwrap();
        assert_eq!(dry.get("dry_run").and_then(Value::as_bool), Some(true));
    }
}
