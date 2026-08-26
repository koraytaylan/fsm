use std::collections::BTreeMap;

use fsm_cli::clock::FixedClock;
use fsm_cli::store::Store;
use fsm_core::json::{JsonLimits, Value, parse};

pub(crate) fn case() -> Value {
    parse(
        include_bytes!("../../../fsm-core/tests/fixtures/machines/case_review.json"),
        &JsonLimits::DEFAULT,
    )
    .unwrap()
}

pub(crate) fn store() -> (Store, FixedClock) {
    (Store::open_memory().unwrap(), FixedClock::new(1000, 1000))
}

pub(crate) fn obj(pairs: &[(&str, Value)]) -> Value {
    Value::Obj(
        pairs
            .iter()
            .map(|(k, v)| ((*k).into(), v.clone()))
            .collect(),
    )
}

fn finding_path(err: &fsm_cli::store::ErrorObj) -> String {
    if !err.path.is_empty() {
        return err.path.clone();
    }
    err.details
        .get("findings")
        .and_then(Value::as_arr)
        .and_then(|a| a.first())
        .and_then(|f| f.get("path"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

fn finding_hint(err: &fsm_cli::store::ErrorObj) -> String {
    if !err.hint.is_empty() {
        return err.hint.clone();
    }
    err.details
        .get("findings")
        .and_then(Value::as_arr)
        .and_then(|a| a.first())
        .and_then(|f| f.get("hint"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

pub(crate) fn first_trace_int(v: &Value) -> Option<i64> {
    match v {
        Value::Obj(o) => {
            if let Some(n) = o
                .get("value")
                .and_then(Value::as_str)
                .and_then(|s| s.parse().ok())
            {
                return Some(n);
            }
            o.values().find_map(first_trace_int)
        }
        Value::Arr(a) => a.iter().find_map(first_trace_int),
        _ => None,
    }
}

pub(crate) fn payload_field_for(err: &fsm_cli::store::ErrorObj, event: &str) -> String {
    err.details
        .get("enabled_events")
        .and_then(Value::as_arr)
        .into_iter()
        .flatten()
        .find(|e| e.get("event").and_then(Value::as_str) == Some(event))
        .and_then(|e| e.get("payload_fields"))
        .and_then(Value::as_arr)
        .and_then(|a| a.first())
        .and_then(Value::as_str)
        .unwrap_or_else(|| {
            panic!(
                "error identifies the failing event payload field: {:?}",
                err.to_value()
            )
        })
        .to_string()
}

fn delete_pointer(v: &mut Value, path: &str) {
    let segs: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    if segs.is_empty() {
        return;
    }
    let (last, rest) = segs.split_last().unwrap();
    let mut cur = v;
    for s in rest {
        match cur {
            Value::Obj(o) => {
                if let Some(n) = o.get_mut(*s) {
                    cur = n;
                } else {
                    return;
                }
            }
            Value::Arr(a) => {
                if let Ok(i) = s.parse::<usize>() {
                    if let Some(n) = a.get_mut(i) {
                        cur = n;
                    } else {
                        return;
                    }
                } else {
                    return;
                }
            }
            _ => return,
        }
    }
    match cur {
        Value::Obj(o) => {
            o.remove(*last);
        }
        Value::Arr(a) => {
            if let Ok(i) = last.parse::<usize>() {
                if i < a.len() {
                    a.remove(i);
                }
            }
        }
        _ => {}
    }
}

fn set_pointer(v: &mut Value, path: &str, val: Value) {
    let segs: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    if segs.is_empty() {
        *v = val;
        return;
    }
    let (last, rest) = segs.split_last().unwrap();
    let mut cur = v;
    for s in rest {
        match cur {
            Value::Obj(o) => {
                cur = o.entry((*s).into()).or_insert(Value::Obj(BTreeMap::new()));
            }
            Value::Arr(a) => {
                if let Ok(i) = s.parse::<usize>() {
                    if i < a.len() {
                        cur = &mut a[i];
                    } else {
                        return;
                    }
                } else {
                    return;
                }
            }
            _ => return,
        }
    }
    match cur {
        Value::Obj(o) => {
            o.insert((*last).into(), val);
        }
        Value::Arr(a) => {
            if let Ok(i) = last.parse::<usize>() {
                if i < a.len() {
                    a[i] = val;
                }
            }
        }
        _ => {}
    }
}

fn string_at_pointer<'a>(v: &'a Value, path: &str) -> Option<&'a str> {
    let mut cur = v;
    for s in path.split('/').filter(|s| !s.is_empty()) {
        cur = match cur {
            Value::Obj(o) => o.get(s)?,
            Value::Arr(a) => a.get(s.parse::<usize>().ok()?)?,
            _ => return None,
        };
    }
    cur.as_str()
}

fn first_state_name(v: &Value) -> Option<String> {
    v.get("states")
        .and_then(Value::as_arr)
        .and_then(|a| a.first())
        .and_then(|s| s.get("name"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn truncate_array(v: &mut Value, path: &str, keep: usize) {
    let segs: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    let mut cur = v;
    for s in &segs {
        match cur {
            Value::Obj(o) => {
                if let Some(n) = o.get_mut(*s) {
                    cur = n;
                } else {
                    return;
                }
            }
            Value::Arr(a) => {
                if let Ok(i) = s.parse::<usize>() {
                    if let Some(n) = a.get_mut(i) {
                        cur = n;
                    } else {
                        return;
                    }
                } else {
                    return;
                }
            }
            _ => return,
        }
    }
    if let Value::Arr(a) = cur {
        a.truncate(keep);
    } else if let Value::Obj(o) = cur {
        o.clear();
    }
}

pub(crate) fn repair_spec(bad: &Value, err: &fsm_cli::store::ErrorObj) -> Value {
    let mut v = bad.clone();
    let path = finding_path(err);
    let hint = finding_hint(err);
    if let Value::Obj(o) = &mut v {
        if let Some(n) = o
            .get_mut("name")
            .and_then(|n| if let Value::Str(s) = n { Some(s) } else { None })
        {
            if !n.ends_with("_fix") {
                n.push_str("_fix");
            }
        }
    }
    match err.code.as_str() {
        "def/unknown_key" => delete_pointer(&mut v, &path),
        "def/shape" => {
            if v.get("name").is_none() {
                set_pointer(&mut v, "/name", Value::Str("fixed".into()));
            }
            if v.get("states").is_none() {
                set_pointer(
                    &mut v,
                    "/states",
                    Value::Arr(vec![obj(&[("name", Value::Str("a".into()))])]),
                );
            }
            if v.get("initial").is_none() {
                if let Some(n) = first_state_name(&v) {
                    set_pointer(&mut v, "/initial", Value::Str(n));
                } else {
                    set_pointer(&mut v, "/initial", Value::Str("a".into()));
                }
            }
            if v.get("events").is_none() {
                set_pointer(&mut v, "/events", Value::Arr(vec![]));
            }
            if v.get("transitions").is_none() {
                set_pointer(&mut v, "/transitions", Value::Arr(vec![]));
            }
            if v.get("context").is_none() {
                set_pointer(&mut v, "/context", Value::Arr(vec![]));
            }
            let _ = (&path, &hint);
        }
        "def/dup_name" => {
            if let Some(Value::Arr(st)) = v.as_obj_mut().and_then(|o| o.get_mut("states")) {
                if st.len() > 1 {
                    if let Some(Value::Obj(o)) = st.last_mut() {
                        o.insert("name".into(), Value::Str("b".into()));
                    }
                }
            }
        }
        "def/reserved_ident" => {
            if let Some(Value::Arr(st)) = v.as_obj_mut().and_then(|o| o.get_mut("states")) {
                if let Some(Value::Obj(o)) = st.first_mut() {
                    o.insert("name".into(), Value::Str("a".into()));
                }
            }
            set_pointer(&mut v, "/initial", Value::Str("a".into()));
        }
        "def/unknown_state" | "def/initial_not_child" | "def/initial_is_history" => {
            if let Some(n) = first_state_name(&v) {
                if path.contains("initial") || path.is_empty() {
                    let child = v
                        .get("states")
                        .and_then(Value::as_arr)
                        .and_then(|st| st.first())
                        .and_then(|c| c.get("states"))
                        .and_then(Value::as_arr)
                        .and_then(|a| a.iter().find(|x| x.get("history").is_none()))
                        .and_then(|x| x.get("name"))
                        .and_then(Value::as_str)
                        .map(str::to_string);
                    if let Some(ch) = child {
                        set_pointer(&mut v, "/states/0/initial", Value::Str(ch));
                    }
                    set_pointer(&mut v, "/initial", Value::Str(n));
                }
            }
        }
        "def/one_initial" => {
            let child = v
                .get("states")
                .and_then(Value::as_arr)
                .and_then(|a| a.first())
                .and_then(|c| c.get("states"))
                .and_then(Value::as_arr)
                .and_then(|a| a.first())
                .and_then(|s| s.get("name"))
                .and_then(Value::as_str)
                .map(str::to_string);
            if let Some(ch) = child {
                set_pointer(&mut v, "/states/0/initial", Value::Str(ch));
            }
        }
        "def/initial_terminal" | "def/terminal_not_leaf" => {
            delete_pointer(&mut v, "/states/0/terminal");
        }
        "def/eventless_evt" => {
            // The hint offers two fixes; the caller takes the second and
            // names the declared event that supplies `evt`.
            if let Some(Value::Arr(tr)) = v.as_obj_mut().and_then(|o| o.get_mut("transitions")) {
                if let Some(Value::Obj(t)) = tr.first_mut() {
                    t.insert("on".into(), Value::Str("e".into()));
                }
            }
        }
        "def/eventless_from_terminal" => {
            if let Some(Value::Arr(st)) = v.as_obj_mut().and_then(|o| o.get_mut("states")) {
                for state in st.iter_mut() {
                    if let Value::Obj(o) = state {
                        o.remove("terminal");
                    }
                }
            }
        }
        "def/eventless_internal_noop" => {
            if let Some(Value::Arr(tr)) = v.as_obj_mut().and_then(|o| o.get_mut("transitions")) {
                tr.retain(|t| t.get("on").is_some() || t.get("to").is_some());
            }
        }
        "def/terminal_has_transitions" => {
            if let Some(Value::Arr(tr)) = v.as_obj_mut().and_then(|o| o.get_mut("transitions")) {
                if let Some(Value::Obj(t)) = tr.first_mut() {
                    t.insert("from".into(), Value::Str("a".into()));
                }
            }
            if let Some(Value::Arr(st)) = v.as_obj_mut().and_then(|o| o.get_mut("states")) {
                if let Some(Value::Obj(o)) = st.get_mut(1) {
                    o.remove("terminal");
                }
            }
        }
        "def/from_history" => {
            if let Some(Value::Arr(tr)) = v.as_obj_mut().and_then(|o| o.get_mut("transitions")) {
                if let Some(Value::Obj(t)) = tr.first_mut() {
                    t.insert("from".into(), Value::Str("l".into()));
                }
            }
        }
        "def/history_target_from_inside" => {
            if let Some(Value::Arr(tr)) = v.as_obj_mut().and_then(|o| o.get_mut("transitions")) {
                if let Some(Value::Obj(t)) = tr.first_mut() {
                    t.insert("to".into(), Value::Str("r".into()));
                }
            }
        }
        "def/multiple_history" => {
            if let Some(Value::Arr(top)) = v.as_obj_mut().and_then(|o| o.get_mut("states")) {
                if let Some(Value::Obj(c)) = top.first_mut() {
                    if let Some(Value::Arr(ch)) = c.get_mut("states") {
                        let mut seen = false;
                        ch.retain(|n| {
                            if n.get("history").is_some() {
                                if seen {
                                    return false;
                                }
                                seen = true;
                            }
                            true
                        });
                    }
                }
            }
        }
        "def/unknown_event" => {
            set_pointer(
                &mut v,
                "/events",
                Value::Arr(vec![obj(&[
                    ("name", Value::Str("nope".into())),
                    ("fields", Value::Arr(vec![])),
                ])]),
            );
        }
        "def/unknown_effect" => {
            set_pointer(
                &mut v,
                "/effects",
                Value::Arr(vec![obj(&[
                    ("name", Value::Str("nope".into())),
                    ("fields", Value::Arr(vec![])),
                ])]),
            );
        }
        "def/unknown_enum" => {
            set_pointer(
                &mut v,
                "/enums",
                Value::Obj(BTreeMap::from([(
                    "Color".into(),
                    Value::Arr(vec![Value::Str("red".into())]),
                )])),
            );
        }
        "def/dup_set" => {
            if let Some(Value::Arr(tr)) = v.as_obj_mut().and_then(|o| o.get_mut("transitions")) {
                if let Some(Value::Obj(t)) = tr.first_mut() {
                    if let Some(Value::Arr(d)) = t.get_mut("do") {
                        d.truncate(1);
                    }
                }
            }
        }
        "def/assign_type" => {
            if let Some(Value::Arr(tr)) = v.as_obj_mut().and_then(|o| o.get_mut("transitions")) {
                if let Some(Value::Obj(t)) = tr.first_mut() {
                    if let Some(Value::Arr(d)) = t.get_mut("do") {
                        if let Some(Value::Obj(s)) = d.first_mut() {
                            s.insert("value".into(), Value::Str("1".into()));
                        }
                    }
                }
            }
        }
        "def/cross_region" => {
            let owner_path = path
                .strip_suffix("/to")
                .expect("cross_region finding points to a target");
            let from = string_at_pointer(&v, &format!("{owner_path}/from"))
                .expect("cross_region source is present")
                .to_string();
            set_pointer(&mut v, &path, Value::Str(from));
        }
        "def/deadline_type" => {
            set_pointer(&mut v, &path, Value::Str("dur(1, s)".into()));
        }
        "def/duplicate_deadline" => truncate_array(&mut v, "/deadlines", 1),
        "expr/unknown_var" => {
            let suggested = hint
                .split('`')
                .nth(1)
                .expect("unknown_var must carry its suggested identifier");
            let finding_message = err
                .details
                .get("findings")
                .and_then(Value::as_arr)
                .and_then(|a| a.first())
                .and_then(|f| f.get("message"))
                .and_then(Value::as_str)
                .unwrap_or(&err.message);
            let unknown = finding_message
                .split("unknown ctx.")
                .nth(1)
                .expect("unknown_var message names the bad identifier");
            let src = string_at_pointer(&v, &path)
                .expect("unknown_var path points to an expression")
                .to_string();
            set_pointer(
                &mut v,
                &path,
                Value::Str(src.replace(&format!("ctx.{unknown}"), &format!("ctx.{suggested}"))),
            );
        }
        "expr/scale_cap" => {
            let target: u8 = hint
                .split(|c: char| !c.is_ascii_digit())
                .find(|s| !s.is_empty())
                .and_then(|s| s.parse().ok())
                .expect("scale_cap hint names the target scale");
            let src = string_at_pointer(&v, &path)
                .expect("scale_cap path points to an expression")
                .to_string();
            let (lhs, rest) = src
                .split_once(" * ")
                .expect("scale_cap fixture contains multiplication");
            set_pointer(
                &mut v,
                &path,
                Value::Str(format!("round({lhs}, {target}, down) * {rest}")),
            );
        }
        "expr/round_widens" => {
            let p = if path.is_empty() {
                "/transitions/0/do/0/value"
            } else {
                path.as_str()
            };
            set_pointer(&mut v, p, Value::Str("ctx.d".into()));
        }
        c if c.starts_with("expr/") => {
            if path.contains("/if") || hint.contains("if") {
                set_pointer(&mut v, "/transitions/0/if", Value::Str("true".into()));
            } else if path.contains("entry") {
                set_pointer(&mut v, "/states/0/entry/do/0/value", Value::Str("1".into()));
            } else if path.contains("invariant") {
                set_pointer(&mut v, "/invariants", Value::Arr(vec![]));
            } else if path.contains("/do") || path.contains("value") {
                set_pointer(
                    &mut v,
                    "/transitions/0/do/0/value",
                    Value::Str("ctx.d".into()),
                );
            } else {
                set_pointer(&mut v, "/transitions/0/if", Value::Str("true".into()));
            }
        }
        c if c.starts_with("def/limit_") => match c {
            "def/limit_bytes" => {
                set_pointer(&mut v, "/description", Value::Str("x".into()));
            }
            "def/limit_eval" => truncate_array(&mut v, "/deadlines", 1),
            "def/limit_depth" => {
                set_pointer(
                    &mut v,
                    "/states",
                    Value::Arr(vec![obj(&[("name", Value::Str("a".into()))])]),
                );
                set_pointer(&mut v, "/initial", Value::Str("a".into()));
            }
            "def/limit_fields" => {
                let name = path
                    .trim_start_matches("/events/")
                    .trim_start_matches("/effects/");
                let bucket = if path.starts_with("/effects/") {
                    "effects"
                } else {
                    "events"
                };
                if let Some(Value::Arr(evs)) = v.as_obj_mut().and_then(|o| o.get_mut(bucket)) {
                    for ev in evs {
                        if ev.get("name").and_then(Value::as_str) == Some(name) {
                            if let Some(Value::Arr(f)) =
                                ev.as_obj_mut().and_then(|o| o.get_mut("fields"))
                            {
                                f.truncate(1);
                            }
                        }
                    }
                } else {
                    truncate_array(&mut v, "/events/0/fields", 1);
                }
            }
            "def/limit_sets" => {
                let p = if path.is_empty() {
                    "/transitions/0/do".into()
                } else if path.ends_with("/do") {
                    path.clone()
                } else {
                    format!("{path}/do")
                };
                truncate_array(&mut v, &p, 1);
            }
            "def/limit_emits" => {
                let p = if path.is_empty() {
                    "/transitions/0/emit".into()
                } else if path.ends_with("/emit") {
                    path.clone()
                } else {
                    format!("{path}/emit")
                };
                truncate_array(&mut v, &p, 1);
            }
            "def/limit_raises" => {
                let p = if path.is_empty() {
                    "/transitions/0/raise".into()
                } else if path.ends_with("/raise") {
                    path.clone()
                } else {
                    format!("{path}/raise")
                };
                truncate_array(&mut v, &p, 1);
            }
            "def/limit_cell" | "def/limit_transitions" => {
                truncate_array(&mut v, "/transitions", 1);
            }
            "def/limit_deadlines" => truncate_array(&mut v, "/deadlines", 1),
            "def/limit_regions" => truncate_array(&mut v, "/regions", 2),
            "def/limit_variants" => {
                let p = if path.is_empty() {
                    "/enums/E"
                } else {
                    path.as_str()
                };
                truncate_array(&mut v, p, 1);
            }
            "def/limit_enums" => truncate_array(&mut v, "/enums", 1),
            "def/limit_states" | "def/limit_history" => {
                truncate_array(&mut v, "/states", 1);
                if let Some(n) = first_state_name(&v) {
                    set_pointer(&mut v, "/initial", Value::Str(n));
                }
            }
            "def/limit_events" => truncate_array(&mut v, "/events", 1),
            "def/limit_ctx" => truncate_array(&mut v, "/context", 1),
            "def/limit_invariants" => truncate_array(&mut v, "/invariants", 1),
            _ => {
                let p = if path.is_empty() {
                    "/states"
                } else {
                    path.as_str()
                };
                truncate_array(&mut v, p, 1);
            }
        },
        "def/final_not_leaf" => {
            // The compound was marked final; only a leaf may be, and its own
            // initial child cannot, so the mark simply comes off.
            delete_pointer(&mut v, "/states/0/states/1/final");
        }
        "def/final_at_root" => {
            delete_pointer(&mut v, "/states/1/final");
            set_pointer(&mut v, "/states/1/terminal", Value::Bool(true));
        }
        "def/final_and_terminal" => {
            delete_pointer(&mut v, "/states/0/states/1/terminal");
        }
        "def/final_has_transitions" => {
            if let Some(Value::Arr(tr)) = v.as_obj_mut().and_then(|o| o.get_mut("transitions")) {
                if let Some(Value::Obj(t)) = tr.first_mut() {
                    t.insert("from".into(), Value::Str("a".into()));
                    t.insert("to".into(), Value::Str("f".into()));
                }
            }
        }
        "def/final_is_initial" => {
            set_pointer(&mut v, "/states/0/initial", Value::Str("a".into()));
        }
        "def/eventless_cycle" | "def/eventless_cycle_guarded" | "def/eventless_depth" => {
            // Every repair breaks the cascade after its first transition:
            // the cycle loses its back edge, the depth its length.
            if let Some(Value::Arr(tr)) = v.as_obj_mut().and_then(|o| o.get_mut("transitions")) {
                tr.truncate(1);
            }
        }
        "def/shadowed"
        | "def/eventless_shadowed"
        | "def/duplicate_guard"
        | "def/ancestor_shadowed" => {
            if let Some(Value::Arr(tr)) = v.as_obj_mut().and_then(|o| o.get_mut("transitions")) {
                tr.truncate(1);
            }
        }
        "def/unreachable_state" => {
            if let Some(Value::Arr(st)) = v.as_obj_mut().and_then(|o| o.get_mut("states")) {
                st.truncate(1);
            }
        }
        "def/create_always_fails" => {
            set_pointer(&mut v, "/invariants", Value::Arr(vec![]));
        }
        _ => {}
    }
    v
}

trait AsObjMut {
    fn as_obj_mut(&mut self) -> Option<&mut BTreeMap<String, Value>>;
}

impl AsObjMut for Value {
    fn as_obj_mut(&mut self) -> Option<&mut BTreeMap<String, Value>> {
        match self {
            Value::Obj(o) => Some(o),
            _ => None,
        }
    }
}
