use std::collections::BTreeMap;

use crate::json::Value;
use crate::machine::EnforceMode;

use super::super::{Block, DeadlineSpec, Finding, InvariantSpec, TransitionSpec, TySpec};
use super::states::parse_block;
use super::{check_keys, req_str};

pub(super) fn parse_ty_spec(v: &Value, path: &str, errs: &mut Vec<Finding>) -> Option<TySpec> {
    if let Some(s) = v.as_str() {
        return match s {
            "int" => Some(TySpec::Int),
            "str" => Some(TySpec::Str),
            "bool" => Some(TySpec::Bool),
            "timestamp" => Some(TySpec::Ts),
            "duration" => Some(TySpec::Dur),
            other => {
                errs.push(Finding::err(
                    "def/shape",
                    path,
                    format!("unknown type {other}"),
                    "use int, str, bool, timestamp, duration, or an object",
                ));
                None
            }
        };
    }
    if let Some(obj) = v.as_obj() {
        check_keys(obj, &["decimal", "enum"], path, errs);
        if obj.contains_key("decimal") && obj.contains_key("enum") {
            errs.push(Finding::err(
                "def/shape",
                path,
                "type object must be decimal or enum, not both",
                "use one type constructor",
            ));
            return None;
        }
        if let Some(n) = obj.get("decimal") {
            if n.as_num().is_some() {
                errs.push(Finding::err(
                    "req/number_token",
                    format!("{path}/decimal"),
                    "scale must be a string",
                    "quote the scale",
                ));
                return None;
            }
            if let Some(s) = n.as_str() {
                match s.parse::<u8>() {
                    Ok(scale) if scale <= crate::decimal::MAX_SCALE => {
                        return Some(TySpec::Dec { scale });
                    }
                    _ => {
                        errs.push(Finding::err(
                            "def/shape",
                            format!("{path}/decimal"),
                            "decimal scale must be 0-12",
                            "use a scale of at most 12",
                        ));
                        return None;
                    }
                }
            }
        }
        if let Some(e) = obj.get("enum") {
            match e.as_str() {
                Some(name) => {
                    return Some(TySpec::Enum {
                        of: name.to_string(),
                    });
                }
                None => {
                    errs.push(Finding::err(
                        "def/shape",
                        format!("{path}/enum"),
                        "enum name must be a string",
                        "quote the enum name",
                    ));
                    return None;
                }
            }
        }
    }
    if v.as_num().is_some() {
        errs.push(Finding::err(
            "req/number_token",
            path,
            "type must not be a raw number",
            "use a string type name",
        ));
        return None;
    }
    errs.push(Finding::err(
        "def/shape",
        path,
        "invalid type",
        "use a type name or object",
    ));
    None
}

pub(super) fn parse_transitions(v: Option<&Value>, errs: &mut Vec<Finding>) -> Vec<TransitionSpec> {
    let Some(v) = v else {
        return Vec::new();
    };
    let Some(arr) = v.as_arr() else {
        errs.push(Finding::err(
            "def/shape",
            "/transitions",
            "transitions must be an array",
            "use an array",
        ));
        return Vec::new();
    };
    let mut out = Vec::new();
    for (i, item) in arr.iter().enumerate() {
        let p = format!("/transitions/{i}");
        let Some(obj) = item.as_obj() else {
            errs.push(Finding::err(
                "def/shape",
                &p,
                "transition must be an object",
                "use an object",
            ));
            continue;
        };
        check_keys(
            obj,
            &["from", "on", "if", "do", "emit", "raise", "signal", "to"],
            &p,
            errs,
        );
        // An absent `on` is an eventless transition. An explicit null is a
        // typo, not an intention, and says so.
        let on = match obj.get("on") {
            None => None,
            Some(Value::Str(s)) => Some(s.clone()),
            Some(Value::Null) => {
                errs.push(Finding::err(
                    "def/shape",
                    format!("{p}/on"),
                    "on is null",
                    "omit on entirely for an eventless transition; null is not a trigger",
                ));
                None
            }
            Some(Value::Num(_)) => {
                errs.push(Finding::err(
                    "req/number_token",
                    format!("{p}/on"),
                    "numeric values must be strings",
                    "quote the number",
                ));
                None
            }
            Some(_) => {
                errs.push(Finding::err(
                    "def/shape",
                    format!("{p}/on"),
                    "on must be a string",
                    "name a declared event, or omit on for an eventless transition",
                ));
                None
            }
        };
        if obj.get("from").is_none() {
            errs.push(Finding::err(
                "def/shape",
                format!("{p}/from"),
                "transition missing from",
                "set from",
            ));
        }
        let block = parse_block(
            Some(&{
                let mut b = BTreeMap::new();
                if let Some(d) = obj.get("do") {
                    b.insert("do".into(), d.clone());
                }
                if let Some(e) = obj.get("emit") {
                    b.insert("emit".into(), e.clone());
                }
                if let Some(r) = obj.get("raise") {
                    b.insert("raise".into(), r.clone());
                }
                if let Some(signal) = obj.get("signal") {
                    b.insert("signal".into(), signal.clone());
                }
                Value::Obj(b)
            }),
            &p,
            errs,
        )
        .unwrap_or(Block {
            sets: Vec::new(),
            emits: Vec::new(),
            raises: Vec::new(),
            signals: Vec::new(),
        });
        let guard = match obj.get("if") {
            None => None,
            Some(Value::Str(s)) => Some(s.clone()),
            Some(Value::Num(_)) => {
                errs.push(Finding::err(
                    "req/number_token",
                    format!("{p}/if"),
                    "guard must be a string",
                    "quote the expression",
                ));
                None
            }
            Some(_) => {
                errs.push(Finding::err(
                    "def/shape",
                    format!("{p}/if"),
                    "guard must be a string",
                    "quote the expression",
                ));
                None
            }
        };
        let to = match obj.get("to") {
            None => None,
            Some(Value::Str(s)) => Some(s.clone()),
            Some(_) => {
                errs.push(Finding::err(
                    "def/shape",
                    format!("{p}/to"),
                    "to must be a string",
                    "name a state",
                ));
                None
            }
        };
        out.push(TransitionSpec {
            from: req_str(obj, "from", &format!("{p}/from"), errs)
                .unwrap_or("")
                .to_string(),
            on,
            guard,
            sets: block.sets,
            emits: block.emits,
            raises: block.raises,
            signals: block.signals,
            to,
        });
    }
    out
}

pub(super) fn parse_deadlines(value: Option<&Value>, errs: &mut Vec<Finding>) -> Vec<DeadlineSpec> {
    let Some(value) = value else {
        return Vec::new();
    };
    let Some(deadlines) = value.as_arr() else {
        errs.push(Finding::err(
            "def/shape",
            "/deadlines",
            "deadlines must be an array",
            "use an array of deadline objects",
        ));
        return Vec::new();
    };
    let mut out = Vec::new();
    for (index, value) in deadlines.iter().enumerate() {
        let path = format!("/deadlines/{index}");
        let Some(object) = value.as_obj() else {
            errs.push(Finding::err(
                "def/shape",
                &path,
                "deadline must be an object",
                "use an object with name, from, after, and to",
            ));
            continue;
        };
        check_keys(
            object,
            &[
                "name", "from", "after", "to", "do", "emit", "raise", "signal",
            ],
            &path,
            errs,
        );
        let block_value = Value::Obj(
            ["do", "emit", "raise", "signal"]
                .into_iter()
                .filter_map(|key| object.get(key).cloned().map(|value| (key.into(), value)))
                .collect(),
        );
        let block = parse_block(Some(&block_value), &path, errs).unwrap_or(Block {
            sets: Vec::new(),
            emits: Vec::new(),
            raises: Vec::new(),
            signals: Vec::new(),
        });
        out.push(DeadlineSpec {
            name: req_str(object, "name", &format!("{path}/name"), errs)
                .unwrap_or("")
                .to_string(),
            from: req_str(object, "from", &format!("{path}/from"), errs)
                .unwrap_or("")
                .to_string(),
            after: req_str(object, "after", &format!("{path}/after"), errs)
                .unwrap_or("")
                .to_string(),
            sets: block.sets,
            emits: block.emits,
            raises: block.raises,
            signals: block.signals,
            to: req_str(object, "to", &format!("{path}/to"), errs)
                .unwrap_or("")
                .to_string(),
        });
    }
    out
}

pub(super) fn parse_invariants(v: Option<&Value>, errs: &mut Vec<Finding>) -> Vec<InvariantSpec> {
    let Some(v) = v else {
        return Vec::new();
    };
    let Some(arr) = v.as_arr() else {
        errs.push(Finding::err(
            "def/shape",
            "/invariants",
            "invariants must be an array",
            "use an array",
        ));
        return Vec::new();
    };
    let mut out = Vec::new();
    for (i, item) in arr.iter().enumerate() {
        let p = format!("/invariants/{i}");
        let Some(obj) = item.as_obj() else {
            errs.push(Finding::err(
                "def/shape",
                &p,
                "invariant must be an object",
                "use an object",
            ));
            continue;
        };
        check_keys(obj, &["name", "expr", "mode"], &p, errs);
        let mode = match obj.get("mode") {
            None => EnforceMode::Enforce,
            Some(Value::Str(s)) if s == "monitor" => EnforceMode::Monitor,
            Some(Value::Str(s)) if s == "enforce" => EnforceMode::Enforce,
            Some(Value::Str(s)) => {
                errs.push(Finding::err(
                    "def/shape",
                    format!("{p}/mode"),
                    format!("unknown mode {s}"),
                    "use enforce or monitor",
                ));
                EnforceMode::Enforce
            }
            Some(_) => {
                errs.push(Finding::err(
                    "def/shape",
                    format!("{p}/mode"),
                    "mode must be a string",
                    "use enforce or monitor",
                ));
                EnforceMode::Enforce
            }
        };
        out.push(InvariantSpec {
            name: req_str(obj, "name", &format!("{p}/name"), errs)
                .unwrap_or("")
                .to_string(),
            expr: req_str(obj, "expr", &format!("{p}/expr"), errs)
                .unwrap_or("")
                .to_string(),
            mode,
        });
    }
    out
}
