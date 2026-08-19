use std::collections::BTreeMap;

use crate::json::Value;

use super::super::{Block, EmitSpec, Finding, HistoryKind, RegionSpec, SetSpec, StateNode};
use super::{check_keys, req_str};

pub(super) fn parse_block(v: Option<&Value>, path: &str, errs: &mut Vec<Finding>) -> Option<Block> {
    let v = v?;
    let Some(obj) = v.as_obj() else {
        errs.push(Finding::err(
            "def/shape",
            path,
            "block must be an object",
            "use {do, emit}",
        ));
        return None;
    };
    check_keys(obj, &["do", "emit"], path, errs);
    let mut sets = Vec::new();
    if let Some(do_v) = obj.get("do") {
        let Some(a) = do_v.as_arr() else {
            errs.push(Finding::err(
                "def/shape",
                format!("{path}/do"),
                "do must be an array",
                "use an array",
            ));
            return Some(Block {
                sets,
                emits: Vec::new(),
            });
        };
        for (i, s) in a.iter().enumerate() {
            let sp = format!("{path}/do/{i}");
            let Some(o) = s.as_obj() else {
                errs.push(Finding::err(
                    "def/shape",
                    &sp,
                    "set must be an object",
                    "use {target, value}",
                ));
                continue;
            };
            check_keys(o, &["target", "value"], &sp, errs);
            sets.push(SetSpec {
                target: req_str(o, "target", &format!("{sp}/target"), errs)
                    .unwrap_or("")
                    .to_string(),
                value: match o.get("value") {
                    Some(Value::Num(_)) => {
                        errs.push(Finding::err(
                            "req/number_token",
                            format!("{sp}/value"),
                            "value must be a string",
                            "quote the expression",
                        ));
                        String::new()
                    }
                    Some(Value::Str(s)) => s.clone(),
                    Some(_) => {
                        errs.push(Finding::err(
                            "def/shape",
                            format!("{sp}/value"),
                            "value must be a string",
                            "quote the expression",
                        ));
                        String::new()
                    }
                    None => {
                        errs.push(Finding::err(
                            "def/shape",
                            format!("{sp}/value"),
                            "value is required",
                            "set value",
                        ));
                        String::new()
                    }
                },
            });
        }
    }
    let mut emits = Vec::new();
    if let Some(em_v) = obj.get("emit") {
        let Some(a) = em_v.as_arr() else {
            errs.push(Finding::err(
                "def/shape",
                format!("{path}/emit"),
                "emit must be an array",
                "use an array",
            ));
            return Some(Block { sets, emits });
        };
        for (i, s) in a.iter().enumerate() {
            let ep = format!("{path}/emit/{i}");
            let Some(o) = s.as_obj() else {
                errs.push(Finding::err(
                    "def/shape",
                    &ep,
                    "emit must be an object",
                    "use {effect, args}",
                ));
                continue;
            };
            check_keys(o, &["effect", "args"], &ep, errs);
            let mut args = BTreeMap::new();
            if let Some(am) = o.get("args") {
                if let Some(map) = am.as_obj() {
                    for (k, val) in map {
                        match val {
                            Value::Num(_) => {
                                errs.push(Finding::err(
                                    "req/number_token",
                                    format!("{ep}/args/{k}"),
                                    "argument must be a string",
                                    "quote the expression",
                                ));
                            }
                            Value::Str(s) => {
                                args.insert(k.clone(), s.clone());
                            }
                            _ => {
                                errs.push(Finding::err(
                                    "def/shape",
                                    format!("{ep}/args/{k}"),
                                    "argument must be a string",
                                    "quote the expression",
                                ));
                            }
                        }
                    }
                } else {
                    errs.push(Finding::err(
                        "def/shape",
                        format!("{ep}/args"),
                        "args must be an object",
                        "map names to expressions",
                    ));
                }
            }
            emits.push(EmitSpec {
                effect: req_str(o, "effect", &format!("{ep}/effect"), errs)
                    .unwrap_or("")
                    .to_string(),
                args,
            });
        }
    }
    Some(Block { sets, emits })
}

pub(super) fn parse_states(arr: &[Value], path: &str, errs: &mut Vec<Finding>) -> Vec<StateNode> {
    let mut out = Vec::new();
    for (i, item) in arr.iter().enumerate() {
        let p = format!("{path}/{i}");
        let Some(obj) = item.as_obj() else {
            errs.push(Finding::err(
                "def/shape",
                &p,
                "state must be an object",
                "use an object",
            ));
            continue;
        };
        check_keys(
            obj,
            &[
                "name", "terminal", "history", "initial", "entry", "exit", "states",
            ],
            &p,
            errs,
        );
        let name = req_str(obj, "name", &format!("{p}/name"), errs)
            .unwrap_or("")
            .to_string();
        let terminal = match obj.get("terminal") {
            None => false,
            Some(Value::Bool(b)) => *b,
            Some(_) => {
                errs.push(Finding::err(
                    "def/shape",
                    format!("{p}/terminal"),
                    "terminal must be a boolean",
                    "use true or false",
                ));
                false
            }
        };
        let history = match obj.get("history") {
            None => None,
            Some(Value::Str(s)) if s == "deep" => Some(HistoryKind::Deep),
            Some(Value::Str(s)) if s == "shallow" => Some(HistoryKind::Shallow),
            Some(Value::Str(s)) => {
                errs.push(Finding::err(
                    "def/shape",
                    format!("{p}/history"),
                    format!("unknown history mode {s}"),
                    "use deep or shallow",
                ));
                None
            }
            Some(_) => {
                errs.push(Finding::err(
                    "def/shape",
                    format!("{p}/history"),
                    "history must be a string",
                    "use deep or shallow",
                ));
                None
            }
        };
        let initial = match obj.get("initial") {
            None => None,
            Some(Value::Str(s)) => Some(s.clone()),
            Some(_) => {
                errs.push(Finding::err(
                    "def/shape",
                    format!("{p}/initial"),
                    "initial must be a string",
                    "name a direct child",
                ));
                None
            }
        };
        let entry = parse_block(obj.get("entry"), &format!("{p}/entry"), errs);
        let exit = parse_block(obj.get("exit"), &format!("{p}/exit"), errs);
        let states = match obj.get("states") {
            None => Vec::new(),
            Some(s) => match s.as_arr() {
                Some(a) => parse_states(a, &format!("{p}/states"), errs),
                None => {
                    errs.push(Finding::err(
                        "def/shape",
                        format!("{p}/states"),
                        "states must be an array",
                        "use an array",
                    ));
                    Vec::new()
                }
            },
        };
        out.push(StateNode {
            name,
            terminal,
            history,
            initial,
            entry,
            exit,
            states,
        });
    }
    out
}

pub(super) fn parse_regions(value: &Value, errs: &mut Vec<Finding>) -> Vec<RegionSpec> {
    let Some(regions) = value.as_arr() else {
        errs.push(Finding::err(
            "def/shape",
            "/regions",
            "regions must be an array",
            "use an array of region objects",
        ));
        return Vec::new();
    };
    let mut out = Vec::new();
    for (index, value) in regions.iter().enumerate() {
        let path = format!("/regions/{index}");
        let Some(object) = value.as_obj() else {
            errs.push(Finding::err(
                "def/shape",
                &path,
                "region must be an object",
                "use an object with name, states, and initial",
            ));
            continue;
        };
        check_keys(object, &["name", "states", "initial"], &path, errs);
        let name = req_str(object, "name", &format!("{path}/name"), errs)
            .unwrap_or("")
            .to_string();
        let states = match object.get("states") {
            Some(Value::Arr(states)) => parse_states(states, &format!("{path}/states"), errs),
            Some(_) => {
                errs.push(Finding::err(
                    "def/shape",
                    format!("{path}/states"),
                    "region states must be an array",
                    "use an array of state objects",
                ));
                Vec::new()
            }
            None => {
                errs.push(Finding::err(
                    "def/shape",
                    format!("{path}/states"),
                    "region states is required",
                    "declare the region's state tree",
                ));
                Vec::new()
            }
        };
        let initial = req_str(object, "initial", &format!("{path}/initial"), errs)
            .unwrap_or("")
            .to_string();
        out.push(RegionSpec {
            name,
            states,
            initial,
        });
    }
    out
}
