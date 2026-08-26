use std::collections::BTreeMap;

use fsm_core::json::Value;

pub(super) fn schema_obj(
    props: BTreeMap<String, Value>,
    required: &[&str],
    additional: bool,
) -> Value {
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

pub(super) fn ty(t: &str) -> Value {
    Value::Obj(BTreeMap::from([("type".into(), Value::Str(t.into()))]))
}

pub(super) fn ty_num(min: i64, max: i64) -> Value {
    Value::Obj(BTreeMap::from([
        ("type".into(), Value::Str("integer".into())),
        ("minimum".into(), Value::Num(min.to_string())),
        ("maximum".into(), Value::Num(max.to_string())),
    ]))
}

pub(super) fn ty_str_array(max_items: usize) -> Value {
    Value::Obj(BTreeMap::from([
        ("type".into(), Value::Str("array".into())),
        ("items".into(), ty("string")),
        ("maxItems".into(), Value::Num(max_items.to_string())),
    ]))
}

pub(super) fn ty_array_of(item: Value) -> Value {
    Value::Obj(BTreeMap::from([
        ("type".into(), Value::Str("array".into())),
        ("items".into(), item),
    ]))
}

pub(super) fn event_obj() -> Value {
    let mut p = BTreeMap::new();
    p.insert("name".into(), ty("string"));
    p.insert("payload".into(), ty("object"));
    schema_obj(p, &["name"], true)
}

pub(super) fn machine_row() -> Value {
    let mut inst = BTreeMap::new();
    inst.insert("running".into(), ty("number"));
    inst.insert("completed".into(), ty("number"));
    inst.insert("cancelled".into(), ty("number"));
    let inst = schema_obj(inst, &["running", "completed", "cancelled"], true);
    let mut p = BTreeMap::new();
    p.insert("machine_id".into(), ty("string"));
    p.insert("name".into(), ty("string"));
    p.insert("defined_seq".into(), ty("number"));
    p.insert("topology".into(), enum_str(&["sequential", "parallel"]));
    p.insert("regions".into(), ty("number"));
    p.insert("states".into(), ty("number"));
    p.insert("events".into(), ty("number"));
    p.insert("deadlines".into(), ty("number"));
    p.insert("instances".into(), inst);
    schema_obj(
        p,
        &[
            "machine_id",
            "name",
            "defined_seq",
            "topology",
            "regions",
            "states",
            "events",
            "deadlines",
            "instances",
        ],
        true,
    )
}

pub(super) fn instance_row() -> Value {
    let mut p = BTreeMap::new();
    p.insert("instance_id".into(), ty("string"));
    p.insert("configuration".into(), configuration_obj());
    p.insert("leaf".into(), ty("string"));
    p.insert("state".into(), ty("string"));
    p.insert("regions".into(), ty("object"));
    p.insert("status".into(), ty("string"));
    p.insert("machine_name".into(), ty("string"));
    p.insert("seq".into(), ty("number"));
    p.insert("tags".into(), ty("array"));
    schema_obj(
        p,
        &[
            "instance_id",
            "configuration",
            "status",
            "machine_name",
            "seq",
            "tags",
        ],
        true,
    )
}

pub(super) fn simulate_step_obj() -> Value {
    let mut p = BTreeMap::new();
    p.insert("index".into(), ty("number"));
    p.insert("event".into(), ty("string"));
    p.insert("from_leaf".into(), ty("string"));
    p.insert("to_leaf".into(), ty("string"));
    p.insert("from_configuration".into(), configuration_obj());
    p.insert("to_configuration".into(), configuration_obj());
    p.insert("region".into(), ty("string"));
    p.insert("applied".into(), ty("boolean"));
    p.insert("context".into(), ty("object"));
    p.insert("args".into(), ty("object"));
    p.insert("effects".into(), ty("array"));
    p.insert("error".into(), ty("object"));
    p.insert("ignored".into(), ty("boolean"));
    p.insert("trace".into(), ty("object"));
    p.insert("microsteps".into(), ty("array"));
    schema_obj(
        p,
        &[
            "index",
            "event",
            "from_configuration",
            "to_configuration",
            "applied",
            "context",
            "effects",
            "trace",
        ],
        true,
    )
}

pub(super) fn simulate_initial_obj() -> Value {
    let mut p = BTreeMap::new();
    p.insert("configuration".into(), configuration_obj());
    p.insert("state".into(), ty("string"));
    p.insert("context".into(), ty("object"));
    schema_obj(p, &["configuration", "context"], true)
}

pub(super) fn simulate_final_obj() -> Value {
    let mut p = BTreeMap::new();
    p.insert("configuration".into(), configuration_obj());
    p.insert("state".into(), ty("string"));
    p.insert("terminal".into(), ty("boolean"));
    p.insert("context".into(), ty("object"));
    schema_obj(p, &["configuration", "context", "terminal"], true)
}

pub(super) fn enum_str(vals: &[&str]) -> Value {
    let mut m = BTreeMap::new();
    m.insert("type".into(), Value::Str("string".into()));
    m.insert(
        "enum".into(),
        Value::Arr(vals.iter().map(|s| Value::Str((*s).into())).collect()),
    );
    Value::Obj(m)
}

pub(super) fn summary_obj() -> Value {
    let mut p = BTreeMap::new();
    p.insert("topology".into(), enum_str(&["sequential", "parallel"]));
    p.insert("initial".into(), ty("string"));
    p.insert("regions".into(), ty("array"));
    p.insert("states".into(), ty("number"));
    p.insert("events".into(), ty("number"));
    p.insert("transitions".into(), ty("number"));
    p.insert("deadlines".into(), ty("number"));
    p.insert("terminal_states".into(), ty("array"));
    schema_obj(
        p,
        &[
            "topology",
            "regions",
            "states",
            "events",
            "transitions",
            "deadlines",
            "terminal_states",
        ],
        true,
    )
}

pub(super) fn machine_ref_obj() -> Value {
    let mut p = BTreeMap::new();
    p.insert("machine_id".into(), ty("string"));
    p.insert("name".into(), ty("string"));
    schema_obj(p, &["machine_id", "name"], true)
}

pub(super) fn transition_obj() -> Value {
    let mut p = BTreeMap::new();
    p.insert("source_state".into(), ty("string"));
    p.insert("transition_idx".into(), ty("number"));
    p.insert("deadline_idx".into(), ty("number"));
    p.insert("region".into(), ty("string"));
    p.insert("internal".into(), ty("boolean"));
    p.insert("from_leaf".into(), ty("string"));
    p.insert("to_leaf".into(), ty("string"));
    p.insert("from_configuration".into(), configuration_obj());
    p.insert("to_configuration".into(), configuration_obj());
    p.insert("exited".into(), ty("array"));
    p.insert("entered".into(), ty("array"));
    schema_obj(
        p,
        &[
            "internal",
            "from_configuration",
            "to_configuration",
            "exited",
            "entered",
        ],
        true,
    )
}

pub(super) fn completeness_obj() -> Value {
    let mut p = BTreeMap::new();
    p.insert("by_leaf".into(), ty("object"));
    schema_obj(p, &["by_leaf"], true)
}

pub(super) fn reachability_obj() -> Value {
    let mut p = BTreeMap::new();
    p.insert("unenterable".into(), ty_array_of(ty("string")));
    schema_obj(p, &["unenterable"], true)
}

pub(super) fn instance_core_props() -> BTreeMap<String, Value> {
    let mut p = BTreeMap::new();
    p.insert("instance_id".into(), ty("string"));
    p.insert("leaf".into(), ty("string"));
    p.insert("state".into(), ty("string"));
    p.insert("status".into(), ty("string"));
    p.insert("context".into(), ty("object"));
    p.insert("seq".into(), ty("number"));
    p.insert("machine".into(), machine_ref_obj());
    p.insert("configuration".into(), configuration_obj());
    p.insert("regions".into(), ty("object"));
    p.insert("effects_pending".into(), ty("array"));
    p.insert("deadlines_pending".into(), ty("array"));
    p.insert("enabled_events".into(), ty("array"));
    p.insert("internal_events".into(), ty("array"));
    p.insert("state_hash".into(), ty("string"));
    p.insert("state_format".into(), ty("string"));
    p
}

pub(super) fn configuration_obj() -> Value {
    let mut p = BTreeMap::new();
    p.insert("kind".into(), enum_str(&["sequential", "parallel"]));
    // `leaf` (sequential) and `leaves` (parallel) are deliberately left as
    // open tagged-variant payloads to keep the complete tools/list response
    // inside its hard context budget.
    schema_obj(p, &["kind"], true)
}
