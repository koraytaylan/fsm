use std::collections::BTreeMap;

use crate::json::Value;

use super::{Finding, MachineSpec, Topology, Unhandled};

mod decls;
mod states;
mod transitions;

use decls::{parse_ctx, parse_effects, parse_events};
use states::{parse_regions, parse_states};
use transitions::{parse_deadlines, parse_invariants, parse_transitions};

pub fn parse_machine(v: &Value) -> Result<MachineSpec, Vec<Finding>> {
    let mut errs = Vec::new();
    let obj = match v.as_obj() {
        Some(o) => o,
        None => {
            return Err(vec![Finding::err(
                "def/shape",
                "/",
                "machine must be an object",
                "wrap the definition in a JSON object",
            )]);
        }
    };
    check_keys(
        obj,
        &[
            "format",
            "name",
            "description",
            "enums",
            "context",
            "events",
            "effects",
            "states",
            "initial",
            "on_unhandled",
            "transitions",
            "invariants",
            "regions",
            "deadlines",
            "supersedes",
        ],
        "",
        &mut errs,
    );
    let format = req_str(obj, "format", "/format", &mut errs)
        .unwrap_or("")
        .to_string();
    if !format.is_empty() && format != "fsm.machine/1" {
        errs.push(Finding::err(
            "def/shape",
            "/format",
            "format must be fsm.machine/1",
            "set format to \"fsm.machine/1\"",
        ));
    }
    let name = req_str(obj, "name", "/name", &mut errs)
        .unwrap_or("")
        .to_string();
    let description = match obj.get("description") {
        None => None,
        Some(Value::Str(s)) => Some(s.clone()),
        Some(_) => {
            errs.push(Finding::err(
                "def/shape",
                "/description",
                "description must be a string",
                "use a string",
            ));
            None
        }
    };
    if obj.get("context").is_none() {
        errs.push(Finding::err(
            "def/shape",
            "/context",
            "context is required",
            "declare a context array",
        ));
    }
    if obj.get("events").is_none() {
        errs.push(Finding::err(
            "def/shape",
            "/events",
            "events is required",
            "declare an events array",
        ));
    }
    if obj.get("transitions").is_none() {
        errs.push(Finding::err(
            "def/shape",
            "/transitions",
            "transitions is required",
            "declare a transitions array",
        ));
    }
    let mut enums = BTreeMap::new();
    if let Some(en) = obj.get("enums") {
        if let Some(map) = en.as_obj() {
            for (k, val) in map {
                match val.as_arr() {
                    Some(arr) => {
                        let mut vars = Vec::new();
                        for (i, x) in arr.iter().enumerate() {
                            match x.as_str() {
                                Some(s) => vars.push(s.to_string()),
                                None => errs.push(Finding::err(
                                    "def/shape",
                                    format!("/enums/{k}/{i}"),
                                    "enum variant must be a string",
                                    "use an array of strings",
                                )),
                            }
                        }
                        enums.insert(k.clone(), vars);
                    }
                    None => errs.push(Finding::err(
                        "def/shape",
                        format!("/enums/{k}"),
                        "enum variants must be an array",
                        "use an array of strings",
                    )),
                }
            }
        } else {
            errs.push(Finding::err(
                "def/shape",
                "/enums",
                "enums must be an object",
                "map names to variant arrays",
            ));
        }
    }
    let context = parse_ctx(obj.get("context"), &mut errs);
    let events = parse_events(obj.get("events"), &mut errs);
    let effects = parse_effects(obj.get("effects"), &mut errs);
    let topology = match obj.get("regions") {
        Some(regions) => {
            if obj.contains_key("states") || obj.contains_key("initial") {
                errs.push(Finding::err(
                    "def/shape",
                    "/regions",
                    "regions cannot be combined with states or initial",
                    "use either regions, or states with initial",
                ));
            }
            Topology::Parallel {
                regions: parse_regions(regions, &mut errs),
            }
        }
        None => {
            let states = match obj.get("states") {
                Some(Value::Arr(states)) => parse_states(states, "/states", &mut errs),
                Some(_) => {
                    errs.push(Finding::err(
                        "def/shape",
                        "/states",
                        "states must be an array",
                        "use an array of state objects",
                    ));
                    Vec::new()
                }
                None => {
                    errs.push(Finding::err(
                        "def/shape",
                        "/states",
                        "states is required when regions is absent",
                        "declare states and initial, or declare regions",
                    ));
                    Vec::new()
                }
            };
            let initial = req_str(obj, "initial", "/initial", &mut errs)
                .unwrap_or("")
                .to_string();
            Topology::Sequential { states, initial }
        }
    };
    let on_unhandled = match obj.get("on_unhandled") {
        None => Unhandled::Reject,
        Some(Value::Str(s)) if s == "reject" => Unhandled::Reject,
        Some(Value::Str(s)) if s == "ignore" => Unhandled::Ignore,
        Some(Value::Str(s)) => {
            errs.push(Finding::err(
                "def/shape",
                "/on_unhandled",
                format!("unknown on_unhandled {s}"),
                "use reject or ignore",
            ));
            Unhandled::Reject
        }
        Some(_) => {
            errs.push(Finding::err(
                "def/shape",
                "/on_unhandled",
                "on_unhandled must be a string",
                "use reject or ignore",
            ));
            Unhandled::Reject
        }
    };
    let transitions = parse_transitions(obj.get("transitions"), &mut errs);
    let deadlines = parse_deadlines(obj.get("deadlines"), &mut errs);
    let invariants = parse_invariants(obj.get("invariants"), &mut errs);
    let supersedes = decls::parse_supersedes(obj.get("supersedes"), &mut errs);
    if !errs.is_empty() {
        return Err(errs);
    }
    Ok(MachineSpec {
        format,
        name,
        description,
        enums,
        context,
        events,
        effects,
        topology,
        deadlines,
        on_unhandled,
        transitions,
        invariants,
        source: Some(v.clone()),
        supersedes,
    })
}

fn check_keys(
    obj: &BTreeMap<String, Value>,
    allowed: &[&str],
    path: &str,
    errs: &mut Vec<Finding>,
) {
    for k in obj.keys() {
        if !allowed.contains(&k.as_str()) {
            let p = if path.is_empty() {
                format!("/{k}")
            } else {
                format!("{path}/{k}")
            };
            errs.push(Finding::err(
                "def/unknown_key",
                p,
                format!("unknown key {k}"),
                "remove the unknown key",
            ));
        }
    }
}

fn req_str<'a>(
    obj: &'a BTreeMap<String, Value>,
    key: &str,
    path: &str,
    errs: &mut Vec<Finding>,
) -> Option<&'a str> {
    match obj.get(key) {
        Some(Value::Str(s)) => Some(s.as_str()),
        Some(Value::Num(_)) => {
            errs.push(Finding::err(
                "req/number_token",
                path,
                "numeric values must be strings",
                "quote the number",
            ));
            None
        }
        Some(_) => {
            errs.push(Finding::err(
                "def/shape",
                path,
                format!("{key} must be a string"),
                "use a string",
            ));
            None
        }
        None => {
            errs.push(Finding::err(
                "def/shape",
                path,
                format!("{key} is required"),
                format!("set {key}"),
            ));
            None
        }
    }
}
