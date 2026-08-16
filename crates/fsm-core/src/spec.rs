//! `fsm.machine/1` parse, structural validation, and expression binding.

#![allow(
    clippy::collapsible_if,
    clippy::type_complexity,
    clippy::too_many_arguments,
    clippy::ptr_arg
)]

use std::collections::{BTreeMap, BTreeSet};

use crate::expr::lexer::Span;
use crate::expr::parser;
use crate::expr::typeck::{Scope, ScopeKind, Ty, typecheck};
use crate::json::Value;
use crate::limits;
use crate::machine::{CompiledExpr, CompiledMachine, EnforceMode};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub severity: Severity,
    pub code: &'static str,
    pub message: String,
    pub path: String,
    pub span: Option<Span>,
    pub hint: String,
}

impl Finding {
    pub fn err(
        code: &'static str,
        path: impl Into<String>,
        message: impl Into<String>,
        hint: impl Into<String>,
    ) -> Self {
        Self {
            severity: Severity::Error,
            code,
            message: message.into(),
            path: path.into(),
            span: None,
            hint: hint.into(),
        }
    }

    pub fn warn(
        code: &'static str,
        path: impl Into<String>,
        message: impl Into<String>,
        hint: impl Into<String>,
    ) -> Self {
        Self {
            severity: Severity::Warning,
            code,
            message: message.into(),
            path: path.into(),
            span: None,
            hint: hint.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryKind {
    Shallow,
    Deep,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TySpec {
    Int,
    Dec { scale: u8 },
    Str,
    Bool,
    Enum { of: String },
    Ts,
    Dur,
}

impl TySpec {
    pub fn to_ty(&self) -> Ty {
        match self {
            TySpec::Int => Ty::Int,
            TySpec::Dec { scale } => Ty::Dec(*scale),
            TySpec::Str => Ty::Str,
            TySpec::Bool => Ty::Bool,
            TySpec::Enum { of } => Ty::Enum(of.clone()),
            TySpec::Ts => Ty::Ts,
            TySpec::Dur => Ty::Dur,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CtxVar {
    pub name: String,
    pub ty: TySpec,
    pub init: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldDecl {
    pub name: String,
    pub ty: TySpec,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventDecl {
    pub name: String,
    pub fields: Vec<FieldDecl>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectDecl {
    pub name: String,
    pub fields: Vec<FieldDecl>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetSpec {
    pub target: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmitSpec {
    pub effect: String,
    pub args: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub sets: Vec<SetSpec>,
    pub emits: Vec<EmitSpec>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateNode {
    pub name: String,
    pub terminal: bool,
    pub history: Option<HistoryKind>,
    pub initial: Option<String>,
    pub entry: Option<Block>,
    pub exit: Option<Block>,
    pub states: Vec<StateNode>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransitionSpec {
    pub from: String,
    pub on: String,
    pub guard: Option<String>,
    pub sets: Vec<SetSpec>,
    pub emits: Vec<EmitSpec>,
    pub to: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvariantSpec {
    pub name: String,
    pub expr: String,
    pub mode: EnforceMode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MachineSpec {
    pub format: String,
    pub name: String,
    pub description: Option<String>,
    pub enums: BTreeMap<String, Vec<String>>,
    pub context: Vec<CtxVar>,
    pub events: Vec<EventDecl>,
    pub effects: Vec<EffectDecl>,
    pub states: Vec<StateNode>,
    pub initial: String,
    pub on_unhandled: Unhandled,
    pub transitions: Vec<TransitionSpec>,
    pub invariants: Vec<InvariantSpec>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unhandled {
    Reject,
    Ignore,
}

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
        ],
        "",
        &mut errs,
    );
    if obj.contains_key("regions") {
        errs.push(Finding::err(
            "def/not_supported",
            "/regions",
            "regions is not yet supported",
            "omit regions until parallel regions land",
        ));
    }
    if obj.contains_key("deadlines") {
        errs.push(Finding::err(
            "def/not_supported",
            "/deadlines",
            "deadlines is not yet supported",
            "omit deadlines until declarative deadlines land",
        ));
    }
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
    let description = obj
        .get("description")
        .and_then(Value::as_str)
        .map(str::to_string);
    let mut enums = BTreeMap::new();
    if let Some(en) = obj.get("enums") {
        if let Some(map) = en.as_obj() {
            for (k, val) in map {
                match val.as_arr() {
                    Some(arr) => {
                        let vars: Vec<String> = arr
                            .iter()
                            .filter_map(|x| x.as_str().map(str::to_string))
                            .collect();
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
    let states = match obj.get("states") {
        Some(s) if s.as_arr().is_some() => parse_states(s.as_arr().unwrap(), "/states", &mut errs),
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
                "states is required",
                "declare a states array",
            ));
            Vec::new()
        }
    };
    let initial = req_str(obj, "initial", "/initial", &mut errs)
        .unwrap_or("")
        .to_string();
    let on_unhandled = match obj
        .get("on_unhandled")
        .and_then(Value::as_str)
        .unwrap_or("reject")
    {
        "ignore" => Unhandled::Ignore,
        _ => Unhandled::Reject,
    };
    let transitions = parse_transitions(obj.get("transitions"), &mut errs);
    let invariants = parse_invariants(obj.get("invariants"), &mut errs);
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
        states,
        initial,
        on_unhandled,
        transitions,
        invariants,
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

fn parse_ty_spec(v: &Value, path: &str, errs: &mut Vec<Finding>) -> Option<TySpec> {
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
                let scale: u8 = s.parse().unwrap_or(99);
                return Some(TySpec::Dec { scale });
            }
        }
        if let Some(e) = obj.get("enum").and_then(Value::as_str) {
            return Some(TySpec::Enum { of: e.to_string() });
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

fn parse_ctx(v: Option<&Value>, errs: &mut Vec<Finding>) -> Vec<CtxVar> {
    let Some(arr) = v.and_then(Value::as_arr) else {
        if v.is_some() {
            errs.push(Finding::err(
                "def/shape",
                "/context",
                "context must be an array",
                "use an array",
            ));
        }
        return Vec::new();
    };
    let mut out = Vec::new();
    for (i, item) in arr.iter().enumerate() {
        let path = format!("/context/{i}");
        let Some(obj) = item.as_obj() else {
            errs.push(Finding::err(
                "def/shape",
                &path,
                "context entry must be an object",
                "use an object",
            ));
            continue;
        };
        let name = req_str(obj, "name", &format!("{path}/name"), errs)
            .unwrap_or("")
            .to_string();
        let ty = obj
            .get("ty")
            .and_then(|t| parse_ty_spec(t, &format!("{path}/ty"), errs));
        let init = match obj.get("init") {
            Some(Value::Num(_)) => {
                errs.push(Finding::err(
                    "req/number_token",
                    format!("{path}/init"),
                    "init must be a string",
                    "quote the number",
                ));
                String::new()
            }
            Some(Value::Str(s)) => s.clone(),
            _ => {
                errs.push(Finding::err(
                    "def/shape",
                    format!("{path}/init"),
                    "init is required",
                    "set init",
                ));
                String::new()
            }
        };
        if let Some(ty) = ty {
            out.push(CtxVar { name, ty, init });
        }
    }
    out
}

fn parse_fields(v: Option<&Value>, path: &str, errs: &mut Vec<Finding>) -> Vec<FieldDecl> {
    let Some(arr) = v.and_then(Value::as_arr) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (i, item) in arr.iter().enumerate() {
        let p = format!("{path}/{i}");
        let Some(obj) = item.as_obj() else { continue };
        let name = obj
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        if let Some(ty) = obj
            .get("ty")
            .and_then(|t| parse_ty_spec(t, &format!("{p}/ty"), errs))
        {
            out.push(FieldDecl { name, ty });
        }
    }
    out
}

fn parse_events(v: Option<&Value>, errs: &mut Vec<Finding>) -> Vec<EventDecl> {
    let Some(arr) = v.and_then(Value::as_arr) else {
        return Vec::new();
    };
    arr.iter()
        .enumerate()
        .filter_map(|(i, item)| {
            let obj = item.as_obj()?;
            Some(EventDecl {
                name: obj
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                fields: parse_fields(obj.get("fields"), &format!("/events/{i}/fields"), errs),
            })
        })
        .collect()
}

fn parse_effects(v: Option<&Value>, errs: &mut Vec<Finding>) -> Vec<EffectDecl> {
    let Some(arr) = v.and_then(Value::as_arr) else {
        return Vec::new();
    };
    arr.iter()
        .enumerate()
        .filter_map(|(i, item)| {
            let obj = item.as_obj()?;
            Some(EffectDecl {
                name: obj
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                fields: parse_fields(obj.get("fields"), &format!("/effects/{i}/fields"), errs),
            })
        })
        .collect()
}

fn parse_block(v: Option<&Value>, path: &str, errs: &mut Vec<Finding>) -> Option<Block> {
    let obj = v?.as_obj()?;
    let sets = obj
        .get("do")
        .and_then(Value::as_arr)
        .map(|a| {
            a.iter()
                .enumerate()
                .filter_map(|(i, s)| {
                    let o = s.as_obj()?;
                    Some(SetSpec {
                        target: o
                            .get("target")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string(),
                        value: match o.get("value") {
                            Some(Value::Num(_)) => {
                                errs.push(Finding::err(
                                    "req/number_token",
                                    format!("{path}/do/{i}/value"),
                                    "value must be a string",
                                    "quote the expression",
                                ));
                                String::new()
                            }
                            Some(Value::Str(s)) => s.clone(),
                            _ => String::new(),
                        },
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let emits = obj
        .get("emit")
        .and_then(Value::as_arr)
        .map(|a| {
            a.iter()
                .enumerate()
                .filter_map(|(i, s)| {
                    let o = s.as_obj()?;
                    let mut args = BTreeMap::new();
                    if let Some(am) = o.get("args").and_then(Value::as_obj) {
                        for (k, val) in am {
                            match val {
                                Value::Num(_) => {
                                    errs.push(Finding::err(
                                        "req/number_token",
                                        format!("{path}/emit/{i}/args/{k}"),
                                        "argument must be a string",
                                        "quote the expression",
                                    ));
                                }
                                Value::Str(s) => {
                                    args.insert(k.clone(), s.clone());
                                }
                                _ => {}
                            }
                        }
                    }
                    Some(EmitSpec {
                        effect: o
                            .get("effect")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string(),
                        args,
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    Some(Block { sets, emits })
}

fn parse_states(arr: &[Value], path: &str, errs: &mut Vec<Finding>) -> Vec<StateNode> {
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
        let name = obj
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let terminal = obj
            .get("terminal")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let history = match obj.get("history").and_then(Value::as_str) {
            Some("deep") => Some(HistoryKind::Deep),
            Some("shallow") => Some(HistoryKind::Shallow),
            _ => None,
        };
        let initial = obj
            .get("initial")
            .and_then(Value::as_str)
            .map(str::to_string);
        let entry = parse_block(obj.get("entry"), &format!("{p}/entry"), errs);
        let exit = parse_block(obj.get("exit"), &format!("{p}/exit"), errs);
        let states = obj
            .get("states")
            .and_then(Value::as_arr)
            .map(|a| parse_states(a, &format!("{p}/states"), errs))
            .unwrap_or_default();
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

fn parse_transitions(v: Option<&Value>, errs: &mut Vec<Finding>) -> Vec<TransitionSpec> {
    let Some(arr) = v.and_then(Value::as_arr) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (i, item) in arr.iter().enumerate() {
        let p = format!("/transitions/{i}");
        let Some(obj) = item.as_obj() else { continue };
        if obj.get("on").is_none() {
            errs.push(Finding::err(
                "def/shape",
                format!("{p}/on"),
                "transition missing on",
                "set on",
            ));
        }
        let sets = obj
            .get("do")
            .and_then(Value::as_arr)
            .map(|a| {
                a.iter()
                    .filter_map(|s| {
                        let o = s.as_obj()?;
                        Some(SetSpec {
                            target: o
                                .get("target")
                                .and_then(Value::as_str)
                                .unwrap_or("")
                                .to_string(),
                            value: o
                                .get("value")
                                .and_then(Value::as_str)
                                .unwrap_or("")
                                .to_string(),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        let emits = obj
            .get("emit")
            .and_then(Value::as_arr)
            .map(|a| {
                a.iter()
                    .filter_map(|s| {
                        let o = s.as_obj()?;
                        let mut args = BTreeMap::new();
                        if let Some(am) = o.get("args").and_then(Value::as_obj) {
                            for (k, val) in am {
                                if let Some(s) = val.as_str() {
                                    args.insert(k.clone(), s.to_string());
                                }
                            }
                        }
                        Some(EmitSpec {
                            effect: o
                                .get("effect")
                                .and_then(Value::as_str)
                                .unwrap_or("")
                                .to_string(),
                            args,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        out.push(TransitionSpec {
            from: obj
                .get("from")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            on: obj
                .get("on")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            guard: obj.get("if").and_then(Value::as_str).map(str::to_string),
            sets,
            emits,
            to: obj.get("to").and_then(Value::as_str).map(str::to_string),
        });
    }
    out
}

fn parse_invariants(v: Option<&Value>, _errs: &mut Vec<Finding>) -> Vec<InvariantSpec> {
    let Some(arr) = v.and_then(Value::as_arr) else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|item| {
            let obj = item.as_obj()?;
            Some(InvariantSpec {
                name: obj
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                expr: obj
                    .get("expr")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                mode: match obj.get("mode").and_then(Value::as_str) {
                    Some("monitor") => EnforceMode::Monitor,
                    _ => EnforceMode::Enforce,
                },
            })
        })
        .collect()
}

impl MachineSpec {
    pub fn to_value(&self) -> Value {
        // Structural round-trip sufficient for parse tests; not byte-canonical.
        use crate::json::parse;
        // Rebuild via a small writer
        let mut s = String::from("{\"format\":\"");
        s.push_str(&self.format);
        s.push_str("\",\"name\":\"");
        s.push_str(&self.name);
        s.push('"');
        if let Some(d) = &self.description {
            s.push_str(",\"description\":\"");
            s.push_str(d);
            s.push('"');
        }
        s.push_str(",\"initial\":\"");
        s.push_str(&self.initial);
        s.push('"');
        s.push_str(",\"context\":[");
        for (i, c) in self.context.iter().enumerate() {
            if i > 0 {
                s.push(',');
            }
            s.push_str("{\"name\":\"");
            s.push_str(&c.name);
            s.push_str("\",\"ty\":\"");
            s.push_str(match &c.ty {
                TySpec::Int => "int",
                TySpec::Str => "str",
                TySpec::Bool => "bool",
                TySpec::Ts => "timestamp",
                TySpec::Dur => "duration",
                TySpec::Dec { .. } => "int",
                TySpec::Enum { .. } => "int",
            });
            s.push_str("\",\"init\":\"");
            s.push_str(&c.init);
            s.push_str("\"}");
        }
        s.push_str("],\"events\":[");
        for (i, e) in self.events.iter().enumerate() {
            if i > 0 {
                s.push(',');
            }
            s.push_str("{\"name\":\"");
            s.push_str(&e.name);
            s.push_str("\",\"fields\":[");
            for (j, f) in e.fields.iter().enumerate() {
                if j > 0 {
                    s.push(',');
                }
                s.push_str("{\"name\":\"");
                s.push_str(&f.name);
                s.push_str("\",\"ty\":\"int\"}");
            }
            s.push_str("]}");
        }
        s.push_str("],\"effects\":[");
        for (i, e) in self.effects.iter().enumerate() {
            if i > 0 {
                s.push(',');
            }
            s.push_str("{\"name\":\"");
            s.push_str(&e.name);
            s.push_str("\",\"fields\":[]}");
        }
        s.push_str("],\"states\":");
        s.push_str(&states_json(&self.states));
        s.push_str(",\"transitions\":[");
        for (i, t) in self.transitions.iter().enumerate() {
            if i > 0 {
                s.push(',');
            }
            s.push_str("{\"from\":\"");
            s.push_str(&t.from);
            s.push_str("\",\"on\":\"");
            s.push_str(&t.on);
            s.push('"');
            if let Some(g) = &t.guard {
                s.push_str(",\"if\":\"");
                s.push_str(g);
                s.push('"');
            }
            if let Some(to) = &t.to {
                s.push_str(",\"to\":\"");
                s.push_str(to);
                s.push('"');
            }
            if !t.sets.is_empty() {
                s.push_str(",\"do\":[");
                for (j, set) in t.sets.iter().enumerate() {
                    if j > 0 {
                        s.push(',');
                    }
                    s.push_str("{\"target\":\"");
                    s.push_str(&set.target);
                    s.push_str("\",\"value\":\"");
                    s.push_str(&set.value);
                    s.push_str("\"}");
                }
                s.push(']');
            }
            s.push('}');
        }
        s.push_str("],\"invariants\":[");
        for (i, inv) in self.invariants.iter().enumerate() {
            if i > 0 {
                s.push(',');
            }
            s.push_str("{\"name\":\"");
            s.push_str(&inv.name);
            s.push_str("\",\"expr\":\"");
            s.push_str(&inv.expr);
            s.push_str("\",\"mode\":\"");
            s.push_str(match inv.mode {
                EnforceMode::Enforce => "enforce",
                EnforceMode::Monitor => "monitor",
            });
            s.push_str("\"}");
        }
        s.push_str("],\"on_unhandled\":\"");
        s.push_str(match self.on_unhandled {
            Unhandled::Reject => "reject",
            Unhandled::Ignore => "ignore",
        });
        s.push_str("\"}");
        parse(s.as_bytes(), &crate::json::JsonLimits::DEFAULT).unwrap_or(Value::Null)
    }

    pub fn walk_states(&self) -> Vec<(&StateNode, Option<&str>)> {
        let mut out = Vec::new();
        fn rec<'a>(
            nodes: &'a [StateNode],
            parent: Option<&'a str>,
            out: &mut Vec<(&'a StateNode, Option<&'a str>)>,
        ) {
            for n in nodes {
                out.push((n, parent));
                rec(&n.states, Some(n.name.as_str()), out);
            }
        }
        rec(&self.states, None, &mut out);
        out
    }
}

fn states_json(nodes: &[StateNode]) -> String {
    let mut s = String::from("[");
    for (i, n) in nodes.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        s.push_str("{\"name\":\"");
        s.push_str(&n.name);
        s.push('"');
        if n.terminal {
            s.push_str(",\"terminal\":true");
        }
        if let Some(h) = n.history {
            s.push_str(",\"history\":\"");
            s.push_str(match h {
                HistoryKind::Deep => "deep",
                HistoryKind::Shallow => "shallow",
            });
            s.push('"');
        }
        if let Some(init) = &n.initial {
            s.push_str(",\"initial\":\"");
            s.push_str(init);
            s.push('"');
        }
        if !n.states.is_empty() {
            s.push_str(",\"states\":");
            s.push_str(&states_json(&n.states));
        }
        if let Some(e) = &n.entry {
            s.push_str(",\"entry\":");
            s.push_str(&block_json(e));
        }
        if let Some(e) = &n.exit {
            s.push_str(",\"exit\":");
            s.push_str(&block_json(e));
        }
        s.push('}');
    }
    s.push(']');
    s
}

fn block_json(b: &Block) -> String {
    let mut s = String::from("{");
    s.push_str("\"do\":[");
    for (i, set) in b.sets.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        s.push_str("{\"target\":\"");
        s.push_str(&set.target);
        s.push_str("\",\"value\":\"");
        s.push_str(&set.value);
        s.push_str("\"}");
    }
    s.push_str("],\"emit\":[");
    for (i, em) in b.emits.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        s.push_str("{\"effect\":\"");
        s.push_str(&em.effect);
        s.push_str("\",\"args\":{}}");
    }
    s.push_str("]}");
    s
}

pub fn validate(spec: &MachineSpec) -> Result<(), Vec<Finding>> {
    let mut errs = Vec::new();
    let mut names = BTreeSet::new();
    let mut by_name: BTreeMap<String, &StateNode> = BTreeMap::new();
    let mut parent: BTreeMap<String, Option<String>> = BTreeMap::new();
    fn collect<'a>(
        nodes: &'a [StateNode],
        par: Option<String>,
        names: &mut BTreeSet<String>,
        by_name: &mut BTreeMap<String, &'a StateNode>,
        parent: &mut BTreeMap<String, Option<String>>,
        errs: &mut Vec<Finding>,
        depth: u32,
    ) {
        if depth > limits::MAX_NESTING {
            errs.push(Finding::err(
                "def/limit_depth",
                "/states",
                "nesting exceeds 12",
                "flatten the tree",
            ));
        }
        for n in nodes {
            if !names.insert(n.name.clone()) {
                errs.push(Finding::err(
                    "def/dup_name",
                    format!("/states/{}", n.name),
                    format!("duplicate name {}", n.name),
                    "rename one of the nodes",
                ));
            }
            if n.name.starts_with('$') {
                errs.push(Finding::err(
                    "def/reserved_ident",
                    format!("/states/{}", n.name),
                    "$-prefixed names are reserved",
                    "remove the $ prefix",
                ));
            }
            by_name.insert(n.name.clone(), n);
            parent.insert(n.name.clone(), par.clone());
            collect(
                &n.states,
                Some(n.name.clone()),
                names,
                by_name,
                parent,
                errs,
                depth + 1,
            );
        }
    }
    collect(
        &spec.states,
        None,
        &mut names,
        &mut by_name,
        &mut parent,
        &mut errs,
        1,
    );
    if names.len() > limits::MAX_STATES {
        errs.push(Finding::err(
            "def/limit_states",
            "/states",
            "more than 256 state nodes",
            "reduce the machine",
        ));
    }
    let mut hist_count = 0usize;
    for (n, node) in &by_name {
        if node.history.is_some() {
            hist_count += 1;
            if !node.states.is_empty() || node.terminal || node.initial.is_some() {
                // history is a leaf-like pseudostate
            }
        }
        if node.history.is_none() && !node.states.is_empty() {
            match &node.initial {
                None => errs.push(Finding::err(
                    "def/one_initial",
                    format!("/states/{n}"),
                    "compound needs initial",
                    "set initial to a direct child",
                )),
                Some(init) => {
                    let child = node.states.iter().find(|c| c.name == *init);
                    match child {
                        None => {
                            if by_name.contains_key(init) {
                                errs.push(Finding::err(
                                    "def/initial_not_child",
                                    format!("/states/{n}/initial"),
                                    "initial is not a direct child",
                                    "name a direct real child",
                                ));
                            } else {
                                errs.push(Finding::err(
                                    "def/unknown_state",
                                    format!("/states/{n}/initial"),
                                    format!("unknown initial {init}"),
                                    "name a declared state",
                                ));
                            }
                        }
                        Some(c) if c.history.is_some() => {
                            errs.push(Finding::err(
                                "def/initial_is_history",
                                format!("/states/{n}/initial"),
                                "initial cannot be a history pseudostate",
                                "name a real child",
                            ));
                        }
                        Some(_) => {}
                    }
                }
            }
            let hists: Vec<_> = node.states.iter().filter(|c| c.history.is_some()).collect();
            if hists.len() > 1 {
                errs.push(Finding::err(
                    "def/multiple_history",
                    format!("/states/{n}"),
                    "at most one history per compound",
                    "remove extra history nodes",
                ));
            }
        }
        if node.terminal && !node.states.is_empty() {
            errs.push(Finding::err(
                "def/terminal_not_leaf",
                format!("/states/{n}"),
                "terminal must be a leaf",
                "remove children or terminal",
            ));
        }
    }
    if hist_count > limits::MAX_HISTORY {
        errs.push(Finding::err(
            "def/limit_history",
            "/states",
            "more than 32 history nodes",
            "reduce history",
        ));
    }
    if !by_name.contains_key(&spec.initial) {
        errs.push(Finding::err(
            "def/unknown_state",
            "/initial",
            format!("unknown initial {}", spec.initial),
            "name a top-level state",
        ));
    } else {
        // creation chain leaf must not be terminal
        let mut cur = spec.initial.as_str();
        loop {
            let node = by_name[cur];
            if node.states.is_empty() {
                if node.terminal {
                    errs.push(Finding::err(
                        "def/initial_terminal",
                        "/initial",
                        "creation chain lands on a terminal",
                        "start in a non-terminal leaf",
                    ));
                }
                break;
            }
            match &node.initial {
                Some(i) if by_name.contains_key(i) => cur = i,
                _ => break,
            }
        }
    }
    let event_names: BTreeSet<_> = spec.events.iter().map(|e| e.name.as_str()).collect();
    let effect_names: BTreeSet<_> = spec.effects.iter().map(|e| e.name.as_str()).collect();
    for e in &spec.events {
        if e.name.starts_with('$') {
            errs.push(Finding::err(
                "def/reserved_ident",
                format!("/events/{}", e.name),
                "$-prefixed identifiers are reserved",
                "remove the $ prefix",
            ));
        }
    }
    if spec.events.len() > limits::MAX_EVENTS {
        errs.push(Finding::err(
            "def/limit_events",
            "/events",
            "more than 128 events",
            "reduce events",
        ));
    }
    if spec.enums.len() > limits::MAX_ENUMS {
        errs.push(Finding::err(
            "def/limit_enums",
            "/enums",
            "more than 32 enums",
            "reduce enums",
        ));
    }
    for (en, vars) in &spec.enums {
        if vars.len() > limits::MAX_VARIANTS {
            errs.push(Finding::err(
                "def/limit_variants",
                format!("/enums/{en}"),
                "more than 64 variants",
                "reduce variants",
            ));
        }
    }
    if spec.context.len() > limits::MAX_CTX_VARS {
        errs.push(Finding::err(
            "def/limit_ctx",
            "/context",
            "more than 64 context variables",
            "reduce context",
        ));
    }
    if spec.transitions.len() > limits::MAX_TRANSITIONS {
        errs.push(Finding::err(
            "def/limit_transitions",
            "/transitions",
            "more than 2048 transitions",
            "reduce transitions",
        ));
    }
    if spec.invariants.len() > limits::MAX_INVARIANTS {
        errs.push(Finding::err(
            "def/limit_invariants",
            "/invariants",
            "more than 64 invariants",
            "reduce invariants",
        ));
    }
    let mut cell: BTreeMap<(String, String), usize> = BTreeMap::new();
    for (i, t) in spec.transitions.iter().enumerate() {
        let p = format!("/transitions/{i}");
        if !by_name.contains_key(&t.from) {
            errs.push(Finding::err(
                "def/unknown_state",
                format!("{p}/from"),
                format!("unknown from {}", t.from),
                "name a declared state",
            ));
        } else {
            let src = by_name[&t.from];
            if src.terminal {
                errs.push(Finding::err(
                    "def/terminal_has_transitions",
                    format!("{p}/from"),
                    "terminal cannot be a source",
                    "do not transition from a terminal",
                ));
            }
            if src.history.is_some() {
                errs.push(Finding::err(
                    "def/from_history",
                    format!("{p}/from"),
                    "history cannot be a source",
                    "use the owner compound",
                ));
            }
        }
        if !event_names.contains(t.on.as_str()) {
            errs.push(Finding::err(
                "def/unknown_event",
                format!("{p}/on"),
                format!("unknown event {}", t.on),
                "declare the event",
            ));
        }
        if let Some(to) = &t.to {
            if !by_name.contains_key(to) {
                errs.push(Finding::err(
                    "def/unknown_state",
                    format!("{p}/to"),
                    format!("unknown to {to}"),
                    "name a declared state",
                ));
            } else if let Some(h) = by_name[to].history {
                let _ = h;
                // owner is parent of history node
                let owner = parent.get(to).and_then(|p| p.clone());
                if let Some(own) = owner {
                    // source must be outside owner
                    let mut walk = Some(t.from.clone());
                    let mut inside = false;
                    while let Some(cur) = walk {
                        if cur == own {
                            inside = true;
                            break;
                        }
                        walk = parent.get(&cur).and_then(|p| p.clone());
                    }
                    if inside {
                        errs.push(Finding::err(
                            "def/history_target_from_inside",
                            format!("{p}/to"),
                            "history may only be targeted from outside its owner",
                            "target a real child instead",
                        ));
                    }
                }
            }
        }
        for em in &t.emits {
            if !effect_names.contains(em.effect.as_str()) {
                errs.push(Finding::err(
                    "def/unknown_effect",
                    format!("{p}/emit"),
                    format!("unknown effect {}", em.effect),
                    "declare the effect",
                ));
            }
        }
        if t.sets.len() > limits::MAX_SETS_PER_BLOCK {
            errs.push(Finding::err(
                "def/limit_sets",
                p.clone(),
                "more than 32 sets in one block",
                "split the block",
            ));
        }
        *cell.entry((t.from.clone(), t.on.clone())).or_insert(0) += 1;
    }
    for ((from, on), n) in cell {
        if n > limits::MAX_TRANSITIONS_PER_CELL {
            errs.push(Finding::err(
                "def/limit_cell",
                format!("/transitions/{from}/{on}"),
                "more than 32 transitions per (state, event)",
                "collapse handlers",
            ));
        }
    }
    for ev in &spec.events {
        if ev.fields.len() > limits::MAX_FIELDS {
            errs.push(Finding::err(
                "def/limit_fields",
                format!("/events/{}", ev.name),
                "more than 32 fields",
                "reduce fields",
            ));
        }
    }
    // enum refs in context
    for c in &spec.context {
        if let TySpec::Enum { of } = &c.ty {
            if !spec.enums.contains_key(of) {
                errs.push(Finding::err(
                    "def/unknown_enum",
                    format!("/context/{}", c.name),
                    format!("unknown enum {of}"),
                    "declare the enum",
                ));
            }
        }
    }
    if errs.is_empty() { Ok(()) } else { Err(errs) }
}

pub fn compile(spec: MachineSpec) -> Result<CompiledMachine, Vec<Finding>> {
    validate(&spec)?;
    let mut errs = Vec::new();
    let mut compiled_exprs = Vec::new();
    let ctx_tys: BTreeMap<String, Ty> = spec
        .context
        .iter()
        .map(|c| (c.name.clone(), c.ty.to_ty()))
        .collect();
    let enums = spec.enums.clone();
    let event_map: BTreeMap<String, BTreeMap<String, Ty>> = spec
        .events
        .iter()
        .map(|e| {
            (
                e.name.clone(),
                e.fields
                    .iter()
                    .map(|f| (f.name.clone(), f.ty.to_ty()))
                    .collect(),
            )
        })
        .collect();
    let effect_map: BTreeMap<String, BTreeMap<String, Ty>> = spec
        .effects
        .iter()
        .map(|e| {
            (
                e.name.clone(),
                e.fields
                    .iter()
                    .map(|f| (f.name.clone(), f.ty.to_ty()))
                    .collect(),
            )
        })
        .collect();

    let bind = |src: &str,
                scope: &Scope<'_>,
                path: &str,
                compiled_exprs: &mut Vec<CompiledExpr>,
                errs: &mut Vec<Finding>|
     -> Option<Ty> {
        match parser::parse(src) {
            Ok(e) => match typecheck(&e, scope) {
                Ok((ty, _)) => {
                    compiled_exprs.push(CompiledExpr {
                        source: src.to_string(),
                        ty: ty.clone(),
                    });
                    Some(ty)
                }
                Err(err) => {
                    let mut f = Finding::err(err.code, path, err.message, err.hint);
                    f.span = Some(err.span);
                    errs.push(f);
                    None
                }
            },
            Err(err) => {
                let mut f = Finding::err(err.code, path, err.message, err.hint);
                f.span = Some(err.span);
                errs.push(f);
                None
            }
        }
    };

    let check_block = |block: &Block,
                       scope: &Scope<'_>,
                       path: &str,
                       compiled_exprs: &mut Vec<CompiledExpr>,
                       errs: &mut Vec<Finding>| {
        let mut seen = BTreeSet::new();
        for (i, set) in block.sets.iter().enumerate() {
            if !seen.insert(&set.target) {
                errs.push(Finding::err(
                    "def/dup_set",
                    format!("{path}/do/{i}"),
                    format!("duplicate set {}", set.target),
                    "set each target at most once per block",
                ));
            }
            let Some(rhs) = bind(
                &set.value,
                scope,
                &format!("{path}/do/{i}/value"),
                compiled_exprs,
                errs,
            ) else {
                continue;
            };
            match ctx_tys.get(&set.target) {
                Some(want) if *want == rhs => {}
                Some(want) => {
                    errs.push(Finding::err(
                        "def/assign_type",
                        format!("{path}/do/{i}"),
                        format!("cannot assign {rhs} to {} ({want})", set.target),
                        "make the scale and class match exactly",
                    ));
                }
                None => {
                    errs.push(Finding::err(
                        "def/unknown_state",
                        format!("{path}/do/{i}/target"),
                        format!("unknown target {}", set.target),
                        "set a declared context variable",
                    ));
                }
            }
        }
        for (i, em) in block.emits.iter().enumerate() {
            let fields = effect_map.get(&em.effect);
            for (k, src) in &em.args {
                let Some(got) = bind(
                    src,
                    scope,
                    &format!("{path}/emit/{i}/args/{k}"),
                    compiled_exprs,
                    errs,
                ) else {
                    continue;
                };
                if let Some(fs) = fields {
                    if let Some(want) = fs.get(k) {
                        if *want != got {
                            errs.push(Finding::err(
                                "expr/type_mismatch",
                                format!("{path}/emit/{i}/args/{k}"),
                                format!("have {got}, want {want}"),
                                "match the effect field type",
                            ));
                        }
                    }
                }
            }
        }
    };

    // entry/exit blocks
    fn walk_blocks(
        nodes: &[StateNode],
        check_block: &dyn Fn(&Block, &Scope<'_>, &str, &mut Vec<CompiledExpr>, &mut Vec<Finding>),
        scope: &Scope<'_>,
        compiled_exprs: &mut Vec<CompiledExpr>,
        errs: &mut Vec<Finding>,
    ) {
        for n in nodes {
            if let Some(b) = &n.entry {
                check_block(
                    b,
                    scope,
                    &format!("/states/{}/entry", n.name),
                    compiled_exprs,
                    errs,
                );
            }
            if let Some(b) = &n.exit {
                check_block(
                    b,
                    scope,
                    &format!("/states/{}/exit", n.name),
                    compiled_exprs,
                    errs,
                );
            }
            walk_blocks(&n.states, check_block, scope, compiled_exprs, errs);
        }
    }
    let block_scope = Scope {
        kind: ScopeKind::Block,
        ctx: &ctx_tys,
        evt: None,
        enums: &enums,
    };
    walk_blocks(
        &spec.states,
        &check_block,
        &block_scope,
        &mut compiled_exprs,
        &mut errs,
    );

    let inv_scope = Scope {
        kind: ScopeKind::Invariant,
        ctx: &ctx_tys,
        evt: None,
        enums: &enums,
    };
    for inv in &spec.invariants {
        bind(
            &inv.expr,
            &inv_scope,
            &format!("/invariants/{}", inv.name),
            &mut compiled_exprs,
            &mut errs,
        );
    }

    let mut transitions_by: BTreeMap<(String, String), Vec<usize>> = BTreeMap::new();
    for (i, t) in spec.transitions.iter().enumerate() {
        transitions_by
            .entry((t.from.clone(), t.on.clone()))
            .or_default()
            .push(i);
        let evt_tys = event_map.get(&t.on);
        let empty: BTreeMap<String, Ty> = BTreeMap::new();
        let guard_scope = Scope {
            kind: ScopeKind::Guard,
            ctx: &ctx_tys,
            evt: evt_tys,
            enums: &enums,
        };
        if let Some(g) = &t.guard {
            bind(
                g,
                &guard_scope,
                &format!("/transitions/{i}/if"),
                &mut compiled_exprs,
                &mut errs,
            );
        }
        let action_scope = Scope {
            kind: ScopeKind::TransitionAction,
            ctx: &ctx_tys,
            evt: evt_tys,
            enums: &enums,
        };
        let block = Block {
            sets: t.sets.clone(),
            emits: t.emits.clone(),
        };
        check_block(
            &block,
            &action_scope,
            &format!("/transitions/{i}"),
            &mut compiled_exprs,
            &mut errs,
        );
        let _ = empty;
    }

    if !errs.is_empty() {
        return Err(errs);
    }
    let canonical = crate::canon::canon_bytes(&spec.to_value());
    let machine_id = crate::hashes::machine_id(&spec.to_value());
    Ok(CompiledMachine {
        machine_id,
        spec,
        canonical,
        transitions_by,
        compiled_exprs,
    })
}

pub fn load_machine_json(bytes: &[u8]) -> Result<MachineSpec, Vec<Finding>> {
    if bytes.len() > limits::MAX_DEF_BYTES {
        return Err(vec![Finding::err(
            "def/limit_bytes",
            "/",
            "definition exceeds 256 KiB",
            "shrink the document",
        )]);
    }
    let v = crate::json::parse(bytes, &crate::json::JsonLimits::DEFAULT)
        .map_err(|e| vec![Finding::err("def/shape", "/", e.message, "fix the JSON")])?;
    parse_machine(&v)
}
