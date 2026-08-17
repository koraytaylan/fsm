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
            output_schema: schema_machine_list_out,
            run: run_machine_list,
        },
        ToolSpec {
            name: "machine_get",
            description: descriptions::MACHINE_GET,
            input_schema: schema_machine_ref_in,
            output_schema: schema_machine_get_out,
            run: run_machine_get,
        },
        ToolSpec {
            name: "machine_analyze",
            description: descriptions::MACHINE_ANALYZE,
            input_schema: schema_machine_ref_in,
            output_schema: schema_machine_analyze_out,
            run: run_machine_analyze,
        },
        ToolSpec {
            name: "machine_diagram",
            description: descriptions::MACHINE_DIAGRAM,
            input_schema: schema_diagram_in,
            output_schema: schema_machine_diagram_out,
            run: run_machine_diagram,
        },
        ToolSpec {
            name: "instance_create",
            description: descriptions::INSTANCE_CREATE,
            input_schema: schema_instance_create_in,
            output_schema: schema_instance_create_out,
            run: run_instance_create,
        },
        ToolSpec {
            name: "instance_send",
            description: descriptions::INSTANCE_SEND,
            input_schema: schema_instance_send_in,
            output_schema: schema_instance_send_out,
            run: run_instance_send,
        },
        ToolSpec {
            name: "effect_ack",
            description: descriptions::EFFECT_ACK,
            input_schema: schema_effect_ack_in,
            output_schema: schema_effect_ack_out,
            run: run_effect_ack,
        },
        ToolSpec {
            name: "instance_cancel",
            description: descriptions::INSTANCE_CANCEL,
            input_schema: schema_instance_cancel_in,
            output_schema: schema_instance_cancel_out,
            run: run_instance_cancel,
        },
        ToolSpec {
            name: "instance_get",
            description: descriptions::INSTANCE_GET,
            input_schema: schema_instance_id_in,
            output_schema: schema_instance_get_out,
            run: run_instance_get,
        },
        ToolSpec {
            name: "instance_list",
            description: descriptions::INSTANCE_LIST,
            input_schema: schema_instance_list_in,
            output_schema: schema_instance_list_out,
            run: run_instance_list,
        },
        ToolSpec {
            name: "instance_history",
            description: descriptions::INSTANCE_HISTORY,
            input_schema: schema_instance_history_in,
            output_schema: schema_instance_history_out,
            run: run_instance_history,
        },
        ToolSpec {
            name: "simulate",
            description: descriptions::SIMULATE,
            input_schema: schema_simulate_in,
            output_schema: schema_simulate_out,
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

fn ty_num(min: i64, max: i64) -> Value {
    Value::Obj(BTreeMap::from([
        ("type".into(), Value::Str("number".into())),
        ("minimum".into(), Value::Num(min.to_string())),
        ("maximum".into(), Value::Num(max.to_string())),
        ("integer".into(), Value::Bool(true)),
    ]))
}

fn ty_str_array(max_items: usize) -> Value {
    Value::Obj(BTreeMap::from([
        ("type".into(), Value::Str("array".into())),
        ("items".into(), ty("string")),
        ("maxItems".into(), Value::Num(max_items.to_string())),
    ]))
}

fn ty_array_of(item: Value) -> Value {
    Value::Obj(BTreeMap::from([
        ("type".into(), Value::Str("array".into())),
        ("items".into(), item),
    ]))
}

fn event_obj() -> Value {
    let mut p = BTreeMap::new();
    p.insert("name".into(), ty("string"));
    p.insert("payload".into(), ty("object"));
    schema_obj(p, &["name"], true)
}

fn machine_row() -> Value {
    let mut inst = BTreeMap::new();
    inst.insert("running".into(), ty("number"));
    inst.insert("completed".into(), ty("number"));
    inst.insert("cancelled".into(), ty("number"));
    let inst = schema_obj(inst, &["running", "completed", "cancelled"], true);
    let mut p = BTreeMap::new();
    p.insert("machine_id".into(), ty("string"));
    p.insert("name".into(), ty("string"));
    p.insert("defined_seq".into(), ty("number"));
    p.insert("states".into(), ty_array_of(ty("string")));
    p.insert("events".into(), ty_array_of(ty("string")));
    p.insert("instances".into(), inst);
    schema_obj(
        p,
        &[
            "machine_id",
            "name",
            "defined_seq",
            "states",
            "events",
            "instances",
        ],
        true,
    )
}

fn instance_row() -> Value {
    let mut p = BTreeMap::new();
    p.insert("instance_id".into(), ty("string"));
    p.insert("state".into(), ty("string"));
    p.insert("status".into(), ty("string"));
    p.insert("machine_name".into(), ty("string"));
    p.insert("seq".into(), ty("number"));
    p.insert("tags".into(), ty_str_array(32));
    schema_obj(
        p,
        &[
            "instance_id",
            "state",
            "status",
            "machine_name",
            "seq",
            "tags",
        ],
        true,
    )
}

fn history_entry_obj() -> Value {
    let mut p = BTreeMap::new();
    p.insert("seq".into(), ty("number"));
    p.insert("kind".into(), ty("string"));
    p.insert("ts".into(), ty("number"));
    p.insert("hash".into(), ty("string"));
    p.insert("request_id".into(), ty("string"));
    p.insert("from_leaf".into(), ty("string"));
    p.insert("to_leaf".into(), ty("string"));
    p.insert("context_after".into(), ty("object"));
    schema_obj(p, &["seq", "ts", "kind", "hash"], true)
}

fn simulate_step_obj() -> Value {
    let mut p = BTreeMap::new();
    p.insert("index".into(), ty("number"));
    p.insert("event".into(), ty("string"));
    p.insert("from_leaf".into(), ty("string"));
    p.insert("to_leaf".into(), ty("string"));
    p.insert("applied".into(), ty("boolean"));
    p.insert("context".into(), ty("object"));
    p.insert("args".into(), ty("object"));
    p.insert("effects".into(), ty("array"));
    p.insert("error".into(), ty("object"));
    p.insert("ignored".into(), ty("boolean"));
    p.insert("trace".into(), ty("object"));
    schema_obj(
        p,
        &[
            "index",
            "event",
            "from_leaf",
            "to_leaf",
            "applied",
            "context",
            "effects",
            "trace",
        ],
        true,
    )
}

fn simulate_initial_obj() -> Value {
    let mut p = BTreeMap::new();
    p.insert("state".into(), ty("string"));
    p.insert("context".into(), ty("object"));
    schema_obj(p, &["state", "context"], true)
}

fn simulate_final_obj() -> Value {
    let mut p = BTreeMap::new();
    p.insert("state".into(), ty("string"));
    p.insert("terminal".into(), ty("boolean"));
    p.insert("context".into(), ty("object"));
    schema_obj(p, &["state", "context"], true)
}

fn finding_obj() -> Value {
    let mut p = BTreeMap::new();
    p.insert("severity".into(), ty("string"));
    p.insert("code".into(), ty("string"));
    p.insert("message".into(), ty("string"));
    p.insert("path".into(), ty("string"));
    p.insert("hint".into(), ty("string"));
    schema_obj(p, &["severity", "code", "message", "path", "hint"], true)
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
    p.insert("dry_run".into(), ty("boolean"));
    p.insert("warnings".into(), ty_array_of(ty("string")));
    p.insert("summary".into(), summary_obj());
    schema_obj(
        p,
        &[
            "machine_id",
            "name",
            "created",
            "dry_run",
            "warnings",
            "summary",
        ],
        true,
    )
}

fn summary_obj() -> Value {
    let mut p = BTreeMap::new();
    p.insert("initial".into(), ty("string"));
    p.insert("states".into(), ty("number"));
    p.insert("events".into(), ty("number"));
    p.insert("transitions".into(), ty("number"));
    p.insert("terminal_states".into(), ty_array_of(ty("string")));
    schema_obj(
        p,
        &[
            "initial",
            "states",
            "events",
            "transitions",
            "terminal_states",
        ],
        true,
    )
}

fn schema_machine_list_out() -> Value {
    let mut p = BTreeMap::new();
    p.insert("machines".into(), ty_array_of(machine_row()));
    p.insert("next_cursor".into(), ty("string"));
    schema_obj(p, &["machines"], true)
}

fn schema_machine_get_out() -> Value {
    let mut p = BTreeMap::new();
    p.insert("machine_id".into(), ty("string"));
    p.insert("name".into(), ty("string"));
    p.insert("spec".into(), ty("object"));
    p.insert("summary".into(), summary_obj());
    schema_obj(p, &["machine_id", "name", "spec", "summary"], true)
}

fn schema_machine_analyze_out() -> Value {
    let mut p = BTreeMap::new();
    p.insert("machine_id".into(), ty("string"));
    p.insert("findings".into(), ty_array_of(finding_obj()));
    p.insert("completeness".into(), ty("object"));
    p.insert("reachability".into(), ty("object"));
    p.insert("shadowing".into(), ty("array"));
    schema_obj(
        p,
        &[
            "machine_id",
            "findings",
            "completeness",
            "reachability",
            "shadowing",
        ],
        true,
    )
}

fn schema_machine_diagram_out() -> Value {
    let mut p = BTreeMap::new();
    p.insert("format".into(), ty("string"));
    p.insert("diagram".into(), ty("string"));
    schema_obj(p, &["format", "diagram"], true)
}

fn schema_instance_create_out() -> Value {
    let mut p = instance_core_props();
    p.insert("request_id".into(), ty("string"));
    schema_obj(
        p,
        &[
            "instance_id",
            "machine",
            "status",
            "state",
            "configuration",
            "seq",
            "context",
            "effects_pending",
            "enabled_events",
            "state_hash",
        ],
        true,
    )
}

fn schema_instance_send_out() -> Value {
    let mut p = instance_core_props();
    p.insert("applied".into(), ty("boolean"));
    p.insert("duplicate".into(), ty("boolean"));
    p.insert("ignored".into(), ty("boolean"));
    p.insert("request_id".into(), ty("string"));
    p.insert("transition".into(), ty("object"));
    p.insert("monitor_flags".into(), ty_array_of(ty("string")));
    p.insert("trace".into(), ty("object"));
    schema_obj(
        p,
        &[
            "applied",
            "duplicate",
            "seq",
            "state",
            "configuration",
            "status",
            "context",
            "effects_pending",
            "enabled_events",
            "state_hash",
            "transition",
            "trace",
            "monitor_flags",
        ],
        true,
    )
}

fn schema_instance_get_out() -> Value {
    let mut p = instance_core_props();
    p.insert("history".into(), ty("object"));
    schema_obj(
        p,
        &[
            "instance_id",
            "machine",
            "status",
            "state",
            "configuration",
            "seq",
            "context",
            "history",
            "effects_pending",
            "enabled_events",
            "state_hash",
        ],
        true,
    )
}

fn schema_instance_cancel_out() -> Value {
    let mut p = BTreeMap::new();
    p.insert("instance_id".into(), ty("string"));
    p.insert("status".into(), ty("string"));
    p.insert("seq".into(), ty("number"));
    p.insert("state".into(), ty("string"));
    p.insert("context".into(), ty("object"));
    p.insert("state_hash".into(), ty("string"));
    p.insert("request_id".into(), ty("string"));
    schema_obj(
        p,
        &[
            "instance_id",
            "status",
            "seq",
            "state",
            "context",
            "state_hash",
        ],
        true,
    )
}

fn instance_core_props() -> BTreeMap<String, Value> {
    let mut p = BTreeMap::new();
    p.insert("instance_id".into(), ty("string"));
    p.insert("leaf".into(), ty("string"));
    p.insert("state".into(), ty("string"));
    p.insert("status".into(), ty("string"));
    p.insert("context".into(), ty("object"));
    p.insert("seq".into(), ty("number"));
    p.insert("machine".into(), ty("object"));
    p.insert("configuration".into(), ty_array_of(ty("string")));
    p.insert("effects_pending".into(), ty_array_of(ty("string")));
    p.insert("enabled_events".into(), ty("array"));
    p.insert("state_hash".into(), ty("string"));
    p
}

fn schema_effect_ack_out() -> Value {
    let mut p = BTreeMap::new();
    p.insert("instance_id".into(), ty("string"));
    p.insert("effect_id".into(), ty("string"));
    p.insert("acked".into(), ty("boolean"));
    p.insert("duplicate".into(), ty("boolean"));
    p.insert("seq".into(), ty("number"));
    p.insert("effects_pending".into(), ty_array_of(ty("string")));
    p.insert("request_id".into(), ty("string"));
    schema_obj(
        p,
        &[
            "instance_id",
            "effect_id",
            "acked",
            "duplicate",
            "seq",
            "effects_pending",
        ],
        true,
    )
}

fn schema_instance_list_out() -> Value {
    let mut p = BTreeMap::new();
    p.insert("instances".into(), ty_array_of(instance_row()));
    p.insert("next_cursor".into(), ty("string"));
    schema_obj(p, &["instances"], true)
}

fn schema_instance_history_out() -> Value {
    let mut p = BTreeMap::new();
    p.insert("instance_id".into(), ty("string"));
    p.insert("entries".into(), ty_array_of(history_entry_obj()));
    p.insert("chain_verified".into(), ty("boolean"));
    p.insert("next_from_seq".into(), ty("number"));
    schema_obj(p, &["instance_id", "entries", "chain_verified"], true)
}

fn schema_simulate_out() -> Value {
    let mut p = BTreeMap::new();
    p.insert("steps".into(), ty_array_of(simulate_step_obj()));
    p.insert("final".into(), simulate_final_obj());
    p.insert("initial".into(), simulate_initial_obj());
    p.insert("stopped_at".into(), ty("number"));
    schema_obj(p, &["steps", "final", "initial"], true)
}

fn schema_machine_list_in() -> Value {
    let mut p = BTreeMap::new();
    p.insert("name_contains".into(), ty("string"));
    p.insert("limit".into(), ty_num(1, 200));
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
    schema_obj(p, &["machine", "format"], false)
}

fn schema_instance_create_in() -> Value {
    let mut p = BTreeMap::new();
    p.insert("machine".into(), ty("string"));
    p.insert("context".into(), ty("object"));
    p.insert("request_id".into(), ty("string"));
    p.insert("tags".into(), ty_str_array(32));
    schema_obj(p, &["machine", "request_id"], false)
}

fn schema_instance_send_in() -> Value {
    let mut p = BTreeMap::new();
    p.insert("instance_id".into(), ty("string"));
    p.insert("event".into(), event_obj());
    p.insert("request_id".into(), ty("string"));
    p.insert("stamp".into(), ty_str_array(32));
    p.insert("expect_seq".into(), ty_num(0, i64::MAX));
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
    p.insert("limit".into(), ty_num(1, 200));
    p.insert("cursor".into(), ty("string"));
    schema_obj(p, &[], false)
}

fn schema_instance_history_in() -> Value {
    let mut p = BTreeMap::new();
    p.insert("instance_id".into(), ty("string"));
    p.insert("from_seq".into(), ty_num(0, i64::MAX));
    p.insert("limit".into(), ty_num(1, 500));
    p.insert("include_trace".into(), ty("boolean"));
    p.insert("include_rejected".into(), ty("boolean"));
    schema_obj(p, &["instance_id"], false)
}

fn schema_simulate_in() -> Value {
    let mut p = BTreeMap::new();
    p.insert("machine".into(), ty("string"));
    p.insert("spec".into(), ty("object"));
    p.insert("context".into(), ty("object"));
    p.insert("events".into(), ty_array_of(event_obj()));
    p.insert("on_reject".into(), enum_str(&["stop", "continue"]));
    schema_obj(p, &["events"], false)
}

pub fn validate_args(schema: &Value, args: &Value) -> Result<(), ErrorObj> {
    let Some(_obj) = args.as_obj() else {
        return Err(invalid(
            "arguments must be an object",
            "arguments",
            "object",
            "not-object",
        ));
    };
    let mut violations = Vec::new();
    collect_violations("", schema, args, &mut violations);
    if violations.is_empty() {
        return Ok(());
    }
    let mut details = BTreeMap::new();
    let fields: Vec<Value> = violations.iter().map(|v| Value::Str(v.0.clone())).collect();
    details.insert("fields".into(), Value::Arr(fields));
    details.insert("field".into(), Value::Str(violations[0].0.clone()));
    details.insert("expected".into(), Value::Str(violations[0].1.clone()));
    details.insert("got".into(), Value::Str(violations[0].2.clone()));
    Err(ErrorObj::new("req/args_invalid", "invalid arguments")
        .hint(format!("fix {}", violations[0].0))
        .details(Value::Obj(details)))
}

fn collect_violations(
    path: &str,
    schema: &Value,
    got: &Value,
    out: &mut Vec<(String, String, String)>,
) {
    if let Some(arr) = schema.get("enum").and_then(Value::as_arr) {
        let s = got.as_str().unwrap_or("");
        if !arr.iter().any(|x| x.as_str() == Some(s)) {
            let listed: Vec<&str> = arr.iter().filter_map(Value::as_str).collect();
            out.push((path.into(), listed.join("|"), s.into()));
            return;
        }
    }
    let want = schema.get("type").and_then(Value::as_str).unwrap_or("");
    let ok = match want {
        "object" => got.is_obj(),
        "string" => got.is_str(),
        "boolean" => got.is_bool(),
        "number" => got.is_num(),
        "array" => got.is_arr(),
        "" => true,
        _ => true,
    };
    if !ok && !want.is_empty() {
        out.push((path.into(), want.into(), type_name(got).into()));
        return;
    }
    if want == "number" {
        if schema.get("integer").and_then(Value::as_bool) == Some(true) {
            let raw = got.as_num().unwrap_or("");
            if raw.is_empty()
                || !raw.bytes().all(|b| b.is_ascii_digit())
                || raw.parse::<u64>().is_err()
            {
                out.push((path.into(), "integer".into(), raw.into()));
                return;
            }
        }
        if let Some(max) = schema
            .get("maximum")
            .and_then(Value::as_num)
            .and_then(|s| s.parse::<i64>().ok())
        {
            if let Some(n) = got.as_num().and_then(|s| s.parse::<i64>().ok()) {
                if n > max {
                    out.push((path.into(), format!("<= {max}"), n.to_string()));
                }
            }
        }
        if let Some(min) = schema
            .get("minimum")
            .and_then(Value::as_num)
            .and_then(|s| s.parse::<i64>().ok())
        {
            if let Some(n) = got.as_num().and_then(|s| s.parse::<i64>().ok()) {
                if n < min {
                    out.push((path.into(), format!(">= {min}"), n.to_string()));
                }
            }
        }
    }
    if want == "object" {
        if let Some(obj) = got.as_obj() {
            let props = schema.get("properties").and_then(Value::as_obj);
            let required = schema
                .get("required")
                .and_then(Value::as_arr)
                .unwrap_or(&[]);
            for req in required {
                let name = req.as_str().unwrap_or("");
                if !obj.contains_key(name) {
                    let p = if path.is_empty() {
                        name.into()
                    } else {
                        format!("{path}.{name}")
                    };
                    out.push((p, "present".into(), "missing".into()));
                }
            }
            let additional = schema
                .get("additionalProperties")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            if let Some(ps) = props {
                for (k, v) in obj {
                    match ps.get(k) {
                        None if !additional => {
                            let p = if path.is_empty() {
                                k.clone()
                            } else {
                                format!("{path}.{k}")
                            };
                            out.push((p, "declared".into(), "extra".into()));
                        }
                        Some(pschema) => {
                            let p = if path.is_empty() {
                                k.clone()
                            } else {
                                format!("{path}.{k}")
                            };
                            collect_violations(&p, pschema, v, out);
                        }
                        None => {}
                    }
                }
            }
        }
    }
    if want == "array" {
        if let Some(arr) = got.as_arr() {
            if let Some(max) = schema
                .get("maxItems")
                .and_then(Value::as_num)
                .and_then(|s| s.parse::<usize>().ok())
            {
                if arr.len() > max {
                    out.push((
                        path.into(),
                        format!("maxItems {max}"),
                        arr.len().to_string(),
                    ));
                }
            }
            if let Some(item) = schema.get("items") {
                for (i, v) in arr.iter().enumerate() {
                    collect_violations(&format!("{path}[{i}]"), item, v, out);
                }
            }
        }
    }
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
    if name == "instance_create" {
        if let Some(ctx) = args.get("context") {
            if !ctx.is_obj() {
                return Err(attach_request_id(
                    crate::store::context_not_object(type_name(ctx)),
                    args,
                ));
            }
        }
    }
    if let Err(e) = validate_args(&(spec.input_schema)(), args) {
        return Err(attach_request_id(e, args));
    }
    let _ = clock;
    (spec.run)(store, clock, args).map_err(|e| attach_request_id(e, args))
}

fn attach_request_id(e: ErrorObj, args: &Value) -> ErrorObj {
    match args.get("request_id").and_then(Value::as_str) {
        Some(rid) if !rid.is_empty() => e.request_id(rid),
        _ => e,
    }
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
    let mut terminals = Vec::new();
    fn walk(nodes: &[fsm_core::spec::StateNode], out: &mut Vec<Value>) {
        for n in nodes {
            if n.terminal {
                out.push(Value::Str(n.name.clone()));
            }
            walk(&n.states, out);
        }
    }
    walk(&c.spec.states, &mut terminals);
    Value::Obj(BTreeMap::from([
        ("initial".into(), Value::Str(c.spec.initial.clone())),
        (
            "states".into(),
            Value::Num(count_nodes(&c.spec.states).to_string()),
        ),
        ("events".into(), Value::Num(c.spec.events.len().to_string())),
        (
            "transitions".into(),
            Value::Num(c.spec.transitions.len().to_string()),
        ),
        ("terminal_states".into(), Value::Arr(terminals)),
    ]))
}

fn run_machine_list(
    store: &mut Store,
    _c: &mut dyn Clock,
    args: &Value,
) -> Result<Value, ErrorObj> {
    let filter = str_arg(args, "name_contains");
    let limit = args
        .get("limit")
        .and_then(|v| v.as_num().and_then(|s| s.parse::<usize>().ok()))
        .unwrap_or(50)
        .clamp(1, 200);
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
        fn collect_names(nodes: &[fsm_core::spec::StateNode], out: &mut Vec<Value>) {
            for n in nodes {
                out.push(Value::Str(n.name.clone()));
                collect_names(&n.states, out);
            }
        }
        let mut states = Vec::new();
        collect_names(&m.compiled.spec.states, &mut states);
        row.insert("states".into(), Value::Arr(states));
        row.insert(
            "events".into(),
            Value::Arr(
                m.compiled
                    .spec
                    .events
                    .iter()
                    .map(|e| Value::Str(e.name.clone()))
                    .collect(),
            ),
        );
        row.insert(
            "state_count".into(),
            Value::Num(count_nodes(&m.compiled.spec.states).to_string()),
        );
        row.insert(
            "event_count".into(),
            Value::Num(m.compiled.spec.events.len().to_string()),
        );
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

fn count_nodes(nodes: &[fsm_core::spec::StateNode]) -> usize {
    nodes.iter().map(|n| 1 + count_nodes(&n.states)).sum()
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
    o.insert("summary".into(), machine_summary(&m.compiled));
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
    let overlay = if let Some(iid) = str_arg(args, "instance") {
        let inst = store
            .state
            .instances
            .get(iid)
            .ok_or_else(|| ErrorObj::new("req/instance_not_found", iid))?;
        Some(fsm_core::diagram::InstanceOverlay {
            current_leaf: inst.leaf.clone(),
            visited: std::collections::BTreeSet::from([inst.leaf.clone()]),
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

fn run_instance_create(
    store: &mut Store,
    _c: &mut dyn Clock,
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
    store.create_instance_ctx(machine, &iid, rid, None, &overrides, &tags)
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
    let stamps: Vec<&str> = args
        .get("stamp")
        .and_then(Value::as_arr)
        .map(|a| a.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    store.send_event_stamp(iid, &name, &mut payload, rid, expect, &stamps)
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
        .unwrap_or(50)
        .clamp(1, 200);
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
        if let Some(st) = state {
            if inst.leaf != st {
                continue;
            }
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
        row.insert("state".into(), Value::Str(inst.leaf.clone()));
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
    if let Some(ctx) = args.get("context") {
        let Value::Obj(obj) = ctx else {
            return Err(ErrorObj::new(
                "req/args_invalid",
                "context must be an object",
            ));
        };
        for (k, val) in obj {
            let decl = compiled
                .spec
                .context
                .iter()
                .find(|c| c.name == *k)
                .ok_or_else(|| ErrorObj::new("req/field_unknown", k.clone()))?;
            let raw = match val {
                Value::Str(s) => s.clone(),
                Value::Bool(b) => b.to_string(),
                Value::Num(_) => {
                    return Err(ErrorObj::new("req/number_token", k.clone()));
                }
                _ => return Err(ErrorObj::new("req/field_type", k.clone())),
            };
            let v = coerce_ctx_override(&decl.ty, k, &raw)?;
            overrides.insert(k.clone(), v);
        }
    }
    let created = fsm_core::step::create(&compiled, &tree, &overrides)
        .map_err(|r| ErrorObj::from_rejection(&r))?;
    let mut initial_ctx = BTreeMap::new();
    for (k, v) in &created.ctx_after {
        initial_ctx.insert(k.clone(), crate::store::val_json(v));
    }
    let initial_state = tree.dotted_path(&created.leaf_after);
    let report = simulate(&compiled, &tree, &overrides, &events, on);
    let mut from_leaf = created.leaf_after.clone();
    let mut steps = Vec::new();
    for st in &report.steps {
        let mut m = BTreeMap::new();
        m.insert("index".into(), Value::Num(st.index.to_string()));
        m.insert("event".into(), Value::Str(st.event.clone()));
        m.insert("from_leaf".into(), Value::Str(from_leaf.clone()));
        let args = events
            .get(st.index)
            .map(|(_, p)| p.clone())
            .unwrap_or(Value::Obj(BTreeMap::new()));
        m.insert("args".into(), args);
        m.insert(
            "applied".into(),
            Value::Bool(matches!(st.outcome, fsm_core::step::Outcome::Applied(_))),
        );
        m.insert("to_leaf".into(), Value::Str(st.leaf_after.clone()));
        let mut ctx = BTreeMap::new();
        for (k, v) in &st.ctx_after {
            ctx.insert(k.clone(), crate::store::val_json(v));
        }
        m.insert("context".into(), Value::Obj(ctx));
        m.insert(
            "effects".into(),
            Value::Arr(
                st.effects
                    .iter()
                    .map(|ef| {
                        let mut em = BTreeMap::new();
                        em.insert("effect".into(), Value::Str(ef.name.clone()));
                        em.insert("k".into(), Value::Num(ef.k.to_string()));
                        let mut args = BTreeMap::new();
                        for (k, v) in &ef.args {
                            args.insert(k.clone(), crate::store::val_json(v));
                        }
                        em.insert("args".into(), Value::Obj(args));
                        Value::Obj(em)
                    })
                    .collect(),
            ),
        );
        match &st.outcome {
            fsm_core::step::Outcome::Applied(a) => {
                m.insert("trace".into(), a.trace.to_value());
            }
            fsm_core::step::Outcome::Rejected(r) => {
                let mut err = BTreeMap::new();
                err.insert("code".into(), Value::Str(r.code.into()));
                err.insert("message".into(), Value::Str(r.message.clone()));
                err.insert("hint".into(), Value::Str(r.hint.clone()));
                m.insert("error".into(), Value::Obj(err));
                m.insert("trace".into(), r.trace.to_value());
            }
            fsm_core::step::Outcome::Ignored => {
                m.insert("ignored".into(), Value::Bool(true));
                m.insert("trace".into(), Value::Obj(BTreeMap::new()));
            }
        }
        if !m.contains_key("trace") {
            m.insert("trace".into(), Value::Obj(BTreeMap::new()));
        }
        from_leaf = st.leaf_after.clone();
        steps.push(Value::Obj(m));
    }
    let mut final_ctx = initial_ctx.clone();
    if let Some(last) = report.steps.last() {
        final_ctx.clear();
        for (k, v) in &last.ctx_after {
            final_ctx.insert(k.clone(), crate::store::val_json(v));
        }
    }
    let mut out = BTreeMap::from([
        (
            "initial".into(),
            Value::Obj(BTreeMap::from([
                ("state".into(), Value::Str(initial_state)),
                ("context".into(), Value::Obj(initial_ctx)),
            ])),
        ),
        ("steps".into(), Value::Arr(steps)),
        (
            "final".into(),
            Value::Obj(BTreeMap::from([
                (
                    "state".into(),
                    Value::Str(tree.dotted_path(&report.final_leaf)),
                ),
                ("terminal".into(), Value::Bool(report.terminal)),
                ("context".into(), Value::Obj(final_ctx)),
            ])),
        ),
    ]);
    if let Some(n) = report.stopped_at {
        out.insert("stopped_at".into(), Value::Num(n.to_string()));
    }
    Ok(Value::Obj(out))
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
