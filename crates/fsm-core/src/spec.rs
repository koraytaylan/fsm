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
use crate::machine::{CompiledExpr, CompiledMachine, EnforceMode, ExprSlot};

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
        check_keys(obj, &["name", "ty", "init"], &path, errs);
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
        if name.starts_with('$') {
            errs.push(Finding::err(
                "def/reserved_ident",
                format!("{path}/name"),
                "$-prefixed identifiers are reserved",
                "remove the $ prefix",
            ));
        }
        if let Some(ty) = ty {
            out.push(CtxVar { name, ty, init });
        }
    }
    out
}

fn parse_fields(v: Option<&Value>, path: &str, errs: &mut Vec<Finding>) -> Vec<FieldDecl> {
    let Some(v) = v else {
        return Vec::new();
    };
    let Some(arr) = v.as_arr() else {
        errs.push(Finding::err(
            "def/shape",
            path,
            "fields must be an array",
            "use an array of field objects",
        ));
        return Vec::new();
    };
    let mut out = Vec::new();
    for (i, item) in arr.iter().enumerate() {
        let p = format!("{path}/{i}");
        let Some(obj) = item.as_obj() else {
            errs.push(Finding::err(
                "def/shape",
                &p,
                "field must be an object",
                "use an object",
            ));
            continue;
        };
        check_keys(obj, &["name", "ty"], &p, errs);
        let name = req_str(obj, "name", &format!("{p}/name"), errs)
            .unwrap_or("")
            .to_string();
        if let Some(ty) = obj
            .get("ty")
            .and_then(|t| parse_ty_spec(t, &format!("{p}/ty"), errs))
        {
            out.push(FieldDecl { name, ty });
        } else if obj.get("ty").is_none() {
            errs.push(Finding::err(
                "def/shape",
                format!("{p}/ty"),
                "ty is required",
                "declare a type",
            ));
        }
    }
    out
}

fn parse_named_list(
    v: Option<&Value>,
    path: &str,
    errs: &mut Vec<Finding>,
) -> Option<Vec<(String, Vec<FieldDecl>)>> {
    match v {
        None => Some(Vec::new()),
        Some(x) => match x.as_arr() {
            None => {
                errs.push(Finding::err(
                    "def/shape",
                    path,
                    format!("{path} must be an array"),
                    "use an array",
                ));
                None
            }
            Some(arr) => {
                let mut out = Vec::new();
                for (i, item) in arr.iter().enumerate() {
                    let p = format!("{path}/{i}");
                    let Some(obj) = item.as_obj() else {
                        errs.push(Finding::err(
                            "def/shape",
                            &p,
                            "entry must be an object",
                            "use an object",
                        ));
                        continue;
                    };
                    check_keys(obj, &["name", "fields"], &p, errs);
                    let name = req_str(obj, "name", &format!("{p}/name"), errs)
                        .unwrap_or("")
                        .to_string();
                    if name.starts_with('$') {
                        errs.push(Finding::err(
                            "def/reserved_ident",
                            format!("{p}/name"),
                            "$-prefixed identifiers are reserved",
                            "remove the $ prefix",
                        ));
                    }
                    let fields = parse_fields(obj.get("fields"), &format!("{p}/fields"), errs);
                    for f in &fields {
                        if f.name.starts_with('$') {
                            errs.push(Finding::err(
                                "def/reserved_ident",
                                format!("{p}/fields"),
                                "$-prefixed identifiers are reserved",
                                "remove the $ prefix",
                            ));
                        }
                    }
                    out.push((name, fields));
                }
                Some(out)
            }
        },
    }
}

fn parse_events(v: Option<&Value>, errs: &mut Vec<Finding>) -> Vec<EventDecl> {
    parse_named_list(v, "/events", errs)
        .unwrap_or_default()
        .into_iter()
        .map(|(name, fields)| EventDecl { name, fields })
        .collect()
}

fn parse_effects(v: Option<&Value>, errs: &mut Vec<Finding>) -> Vec<EffectDecl> {
    parse_named_list(v, "/effects", errs)
        .unwrap_or_default()
        .into_iter()
        .map(|(name, fields)| EffectDecl { name, fields })
        .collect()
}

fn parse_block(v: Option<&Value>, path: &str, errs: &mut Vec<Finding>) -> Option<Block> {
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

fn parse_transitions(v: Option<&Value>, errs: &mut Vec<Finding>) -> Vec<TransitionSpec> {
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
        check_keys(obj, &["from", "on", "if", "do", "emit", "to"], &p, errs);
        if obj.get("on").is_none() {
            errs.push(Finding::err(
                "def/shape",
                format!("{p}/on"),
                "transition missing on",
                "set on",
            ));
        }
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
                Value::Obj(b)
            }),
            &p,
            errs,
        )
        .unwrap_or(Block {
            sets: Vec::new(),
            emits: Vec::new(),
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
            on: req_str(obj, "on", &format!("{p}/on"), errs)
                .unwrap_or("")
                .to_string(),
            guard,
            sets: block.sets,
            emits: block.emits,
            to,
        });
    }
    out
}

fn parse_invariants(v: Option<&Value>, errs: &mut Vec<Finding>) -> Vec<InvariantSpec> {
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

fn v_str(s: impl Into<String>) -> Value {
    Value::Str(s.into())
}

fn v_obj(pairs: impl IntoIterator<Item = (String, Value)>) -> Value {
    Value::Obj(pairs.into_iter().collect())
}

fn ty_spec_value(ty: &TySpec) -> Value {
    match ty {
        TySpec::Int => v_str("int"),
        TySpec::Str => v_str("str"),
        TySpec::Bool => v_str("bool"),
        TySpec::Ts => v_str("timestamp"),
        TySpec::Dur => v_str("duration"),
        TySpec::Dec { scale } => v_obj([("decimal".into(), v_str(scale.to_string()))]),
        TySpec::Enum { of } => v_obj([("enum".into(), v_str(of.clone()))]),
    }
}

fn field_value(f: &FieldDecl) -> Value {
    v_obj([
        ("name".into(), v_str(f.name.clone())),
        ("ty".into(), ty_spec_value(&f.ty)),
    ])
}

fn block_value(b: &Block) -> Value {
    let mut m = BTreeMap::new();
    if !b.sets.is_empty() {
        m.insert(
            "do".into(),
            Value::Arr(
                b.sets
                    .iter()
                    .map(|s| {
                        v_obj([
                            ("target".into(), v_str(s.target.clone())),
                            ("value".into(), v_str(s.value.clone())),
                        ])
                    })
                    .collect(),
            ),
        );
    }
    if !b.emits.is_empty() {
        m.insert(
            "emit".into(),
            Value::Arr(
                b.emits
                    .iter()
                    .map(|e| {
                        let mut o = BTreeMap::new();
                        o.insert("effect".into(), v_str(e.effect.clone()));
                        if !e.args.is_empty() {
                            o.insert(
                                "args".into(),
                                v_obj(e.args.iter().map(|(k, v)| (k.clone(), v_str(v.clone())))),
                            );
                        }
                        Value::Obj(o)
                    })
                    .collect(),
            ),
        );
    }
    Value::Obj(m)
}

fn states_value(nodes: &[StateNode]) -> Value {
    Value::Arr(
        nodes
            .iter()
            .map(|n| {
                let mut m = BTreeMap::new();
                m.insert("name".into(), v_str(n.name.clone()));
                if n.terminal {
                    m.insert("terminal".into(), Value::Bool(true));
                }
                if let Some(h) = n.history {
                    m.insert(
                        "history".into(),
                        v_str(match h {
                            HistoryKind::Deep => "deep",
                            HistoryKind::Shallow => "shallow",
                        }),
                    );
                }
                if let Some(init) = &n.initial {
                    m.insert("initial".into(), v_str(init.clone()));
                }
                if !n.states.is_empty() {
                    m.insert("states".into(), states_value(&n.states));
                }
                if let Some(e) = &n.entry {
                    m.insert("entry".into(), block_value(e));
                }
                if let Some(e) = &n.exit {
                    m.insert("exit".into(), block_value(e));
                }
                Value::Obj(m)
            })
            .collect(),
    )
}

impl MachineSpec {
    pub fn to_value(&self) -> Value {
        let mut m = BTreeMap::new();
        m.insert("format".into(), v_str(self.format.clone()));
        m.insert("name".into(), v_str(self.name.clone()));
        if let Some(d) = &self.description {
            m.insert("description".into(), v_str(d.clone()));
        }
        if !self.enums.is_empty() {
            m.insert(
                "enums".into(),
                v_obj(self.enums.iter().map(|(k, vars)| {
                    (
                        k.clone(),
                        Value::Arr(vars.iter().cloned().map(v_str).collect()),
                    )
                })),
            );
        }
        m.insert(
            "context".into(),
            Value::Arr(
                self.context
                    .iter()
                    .map(|c| {
                        v_obj([
                            ("name".into(), v_str(c.name.clone())),
                            ("ty".into(), ty_spec_value(&c.ty)),
                            ("init".into(), v_str(c.init.clone())),
                        ])
                    })
                    .collect(),
            ),
        );
        m.insert(
            "events".into(),
            Value::Arr(
                self.events
                    .iter()
                    .map(|e| {
                        v_obj([
                            ("name".into(), v_str(e.name.clone())),
                            (
                                "fields".into(),
                                Value::Arr(e.fields.iter().map(field_value).collect()),
                            ),
                        ])
                    })
                    .collect(),
            ),
        );
        m.insert(
            "effects".into(),
            Value::Arr(
                self.effects
                    .iter()
                    .map(|e| {
                        v_obj([
                            ("name".into(), v_str(e.name.clone())),
                            (
                                "fields".into(),
                                Value::Arr(e.fields.iter().map(field_value).collect()),
                            ),
                        ])
                    })
                    .collect(),
            ),
        );
        m.insert("states".into(), states_value(&self.states));
        m.insert("initial".into(), v_str(self.initial.clone()));
        m.insert(
            "on_unhandled".into(),
            v_str(match self.on_unhandled {
                Unhandled::Reject => "reject",
                Unhandled::Ignore => "ignore",
            }),
        );
        m.insert(
            "transitions".into(),
            Value::Arr(
                self.transitions
                    .iter()
                    .map(|t| {
                        let mut o = BTreeMap::new();
                        o.insert("from".into(), v_str(t.from.clone()));
                        o.insert("on".into(), v_str(t.on.clone()));
                        if let Some(g) = &t.guard {
                            o.insert("if".into(), v_str(g.clone()));
                        }
                        if let Some(to) = &t.to {
                            o.insert("to".into(), v_str(to.clone()));
                        }
                        if !t.sets.is_empty() {
                            o.insert(
                                "do".into(),
                                Value::Arr(
                                    t.sets
                                        .iter()
                                        .map(|s| {
                                            v_obj([
                                                ("target".into(), v_str(s.target.clone())),
                                                ("value".into(), v_str(s.value.clone())),
                                            ])
                                        })
                                        .collect(),
                                ),
                            );
                        }
                        if !t.emits.is_empty() {
                            o.insert(
                                "emit".into(),
                                Value::Arr(
                                    t.emits
                                        .iter()
                                        .map(|e| {
                                            let mut em = BTreeMap::new();
                                            em.insert("effect".into(), v_str(e.effect.clone()));
                                            if !e.args.is_empty() {
                                                em.insert(
                                                    "args".into(),
                                                    v_obj(e.args.iter().map(|(k, v)| {
                                                        (k.clone(), v_str(v.clone()))
                                                    })),
                                                );
                                            }
                                            Value::Obj(em)
                                        })
                                        .collect(),
                                ),
                            );
                        }
                        Value::Obj(o)
                    })
                    .collect(),
            ),
        );
        m.insert(
            "invariants".into(),
            Value::Arr(
                self.invariants
                    .iter()
                    .map(|inv| {
                        v_obj([
                            ("name".into(), v_str(inv.name.clone())),
                            ("expr".into(), v_str(inv.expr.clone())),
                            (
                                "mode".into(),
                                v_str(match inv.mode {
                                    EnforceMode::Enforce => "enforce",
                                    EnforceMode::Monitor => "monitor",
                                }),
                            ),
                        ])
                    })
                    .collect(),
            ),
        );
        Value::Obj(m)
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

fn check_block_limits(
    sets: &[SetSpec],
    emits: &[EmitSpec],
    effect_names: &BTreeSet<&str>,
    path: &str,
    errs: &mut Vec<Finding>,
) {
    if sets.len() > limits::MAX_SETS_PER_BLOCK {
        errs.push(Finding::err(
            "def/limit_sets",
            path,
            "more than 32 sets in one block",
            "split the block",
        ));
    }
    if emits.len() > limits::MAX_EMITS_PER_BLOCK {
        errs.push(Finding::err(
            "def/limit_emits",
            path,
            "more than 8 emits in one block",
            "split the block",
        ));
    }
    for em in emits {
        if !effect_names.contains(em.effect.as_str()) {
            errs.push(Finding::err(
                "def/unknown_effect",
                format!("{path}/emit"),
                format!("unknown effect {}", em.effect),
                "declare the effect",
            ));
        }
    }
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
    } else if !spec.states.iter().any(|s| s.name == spec.initial) {
        errs.push(Finding::err(
            "def/initial_not_child",
            "/initial",
            "initial is not a top-level state",
            "name a direct top-level child",
        ));
    } else if spec
        .states
        .iter()
        .find(|s| s.name == spec.initial)
        .and_then(|s| s.history)
        .is_some()
    {
        errs.push(Finding::err(
            "def/initial_is_history",
            "/initial",
            "initial cannot be a history pseudostate",
            "name a real top-level state",
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
    let mut seen_fx = BTreeSet::new();
    for ev in &spec.effects {
        if !seen_fx.insert(ev.name.as_str()) {
            errs.push(Finding::err(
                "def/dup_name",
                format!("/effects/{}", ev.name),
                format!("duplicate effect {}", ev.name),
                "rename one of the effects",
            ));
        }
        let mut seen_f = BTreeSet::new();
        for f in &ev.fields {
            if !seen_f.insert(f.name.as_str()) {
                errs.push(Finding::err(
                    "def/dup_name",
                    format!("/effects/{}/{}", ev.name, f.name),
                    format!("duplicate field {}", f.name),
                    "rename one of the fields",
                ));
            }
        }
    }
    let effect_names: BTreeSet<_> = spec.effects.iter().map(|e| e.name.as_str()).collect();
    let mut seen_ctx = BTreeSet::new();
    for c in &spec.context {
        if !seen_ctx.insert(c.name.as_str()) {
            errs.push(Finding::err(
                "def/dup_name",
                format!("/context/{}", c.name),
                format!("duplicate context {}", c.name),
                "rename one of the variables",
            ));
        }
    }
    let mut seen_ev = BTreeSet::new();
    for e in &spec.events {
        if !seen_ev.insert(e.name.as_str()) {
            errs.push(Finding::err(
                "def/dup_name",
                format!("/events/{}", e.name),
                format!("duplicate event {}", e.name),
                "rename one of the events",
            ));
        }
        let mut seen_f = BTreeSet::new();
        for f in &e.fields {
            if !seen_f.insert(f.name.as_str()) {
                errs.push(Finding::err(
                    "def/dup_name",
                    format!("/events/{}/{}", e.name, f.name),
                    format!("duplicate field {}", f.name),
                    "rename one of the fields",
                ));
            }
        }
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
        check_block_limits(&t.sets, &t.emits, &effect_names, &p, &mut errs);
        *cell.entry((t.from.clone(), t.on.clone())).or_insert(0) += 1;
    }
    fn walk_state_blocks(
        nodes: &[StateNode],
        path: &str,
        effect_names: &BTreeSet<&str>,
        errs: &mut Vec<Finding>,
    ) {
        for (i, n) in nodes.iter().enumerate() {
            let p = format!("{path}/{i}");
            if let Some(b) = &n.entry {
                check_block_limits(&b.sets, &b.emits, effect_names, &format!("{p}/entry"), errs);
            }
            if let Some(b) = &n.exit {
                check_block_limits(&b.sets, &b.emits, effect_names, &format!("{p}/exit"), errs);
            }
            walk_state_blocks(&n.states, &format!("{p}/states"), effect_names, errs);
        }
    }
    walk_state_blocks(&spec.states, "/states", &effect_names, &mut errs);
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
    for ev in &spec.effects {
        if ev.fields.len() > limits::MAX_FIELDS {
            errs.push(Finding::err(
                "def/limit_fields",
                format!("/effects/{}", ev.name),
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
            } else if !spec.enums[of].iter().any(|v| v == &c.init) {
                errs.push(Finding::err(
                    "def/shape",
                    format!("/context/{}/init", c.name),
                    format!("unknown variant {}", c.init),
                    "use a declared variant",
                ));
            }
        }
    }
    for ev in &spec.events {
        for f in &ev.fields {
            if let TySpec::Enum { of } = &f.ty {
                if !spec.enums.contains_key(of) {
                    errs.push(Finding::err(
                        "def/unknown_enum",
                        format!("/events/{}/fields/{}", ev.name, f.name),
                        format!("unknown enum {of}"),
                        "declare the enum",
                    ));
                }
            }
        }
    }
    for ev in &spec.effects {
        for f in &ev.fields {
            if let TySpec::Enum { of } = &f.ty {
                if !spec.enums.contains_key(of) {
                    errs.push(Finding::err(
                        "def/unknown_enum",
                        format!("/effects/{}/fields/{}", ev.name, f.name),
                        format!("unknown enum {of}"),
                        "declare the enum",
                    ));
                }
            }
        }
    }
    if errs.is_empty() { Ok(()) } else { Err(errs) }
}

pub fn compile(spec: MachineSpec) -> Result<CompiledMachine, Vec<Finding>> {
    validate(&spec)?;
    let mut errs = Vec::new();
    let mut compiled_exprs = BTreeMap::new();
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
                slot: ExprSlot,
                compiled_exprs: &mut BTreeMap<ExprSlot, CompiledExpr>,
                errs: &mut Vec<Finding>|
     -> Option<Ty> {
        match parser::parse(src) {
            Ok(e) => match typecheck(&e, scope) {
                Ok((ty, annotated, _)) => {
                    compiled_exprs.insert(
                        slot,
                        CompiledExpr {
                            source: src.to_string(),
                            ty: ty.clone(),
                            expr: annotated,
                        },
                    );
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

    enum BlockOwner {
        Transition(usize),
        Entry(String),
        Exit(String),
    }
    let set_slot = |owner: &BlockOwner, i: usize| -> ExprSlot {
        match owner {
            BlockOwner::Transition(t) => ExprSlot::TransitionSet(*t, i),
            BlockOwner::Entry(n) => ExprSlot::StateEntrySet(n.clone(), i),
            BlockOwner::Exit(n) => ExprSlot::StateExitSet(n.clone(), i),
        }
    };
    let emit_slot = |owner: &BlockOwner, i: usize, arg: &str| -> ExprSlot {
        match owner {
            BlockOwner::Transition(t) => ExprSlot::TransitionEmitArg(*t, i, arg.into()),
            BlockOwner::Entry(n) => ExprSlot::StateEntryEmitArg(n.clone(), i, arg.into()),
            BlockOwner::Exit(n) => ExprSlot::StateExitEmitArg(n.clone(), i, arg.into()),
        }
    };
    let check_block = |block: &Block,
                       scope: &Scope<'_>,
                       path: &str,
                       owner: &BlockOwner,
                       compiled_exprs: &mut BTreeMap<ExprSlot, CompiledExpr>,
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
                set_slot(owner, i),
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
                    emit_slot(owner, i, k),
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
        check_block: &dyn Fn(
            &Block,
            &Scope<'_>,
            &str,
            &BlockOwner,
            &mut BTreeMap<ExprSlot, CompiledExpr>,
            &mut Vec<Finding>,
        ),
        scope: &Scope<'_>,
        compiled_exprs: &mut BTreeMap<ExprSlot, CompiledExpr>,
        errs: &mut Vec<Finding>,
    ) {
        for n in nodes {
            if let Some(b) = &n.entry {
                check_block(
                    b,
                    scope,
                    &format!("/states/{}/entry", n.name),
                    &BlockOwner::Entry(n.name.clone()),
                    compiled_exprs,
                    errs,
                );
            }
            if let Some(b) = &n.exit {
                check_block(
                    b,
                    scope,
                    &format!("/states/{}/exit", n.name),
                    &BlockOwner::Exit(n.name.clone()),
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
    for (i, inv) in spec.invariants.iter().enumerate() {
        if let Some(ty) = bind(
            &inv.expr,
            &inv_scope,
            &format!("/invariants/{}", inv.name),
            ExprSlot::Invariant(i),
            &mut compiled_exprs,
            &mut errs,
        ) {
            if ty != Ty::Bool {
                errs.push(Finding::err(
                    "expr/type_mismatch",
                    format!("/invariants/{}", inv.name),
                    format!("invariant has type {ty}, expected bool"),
                    "write a boolean expression",
                ));
            }
        }
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
            if let Some(ty) = bind(
                g,
                &guard_scope,
                &format!("/transitions/{i}/if"),
                ExprSlot::TransitionGuard(i),
                &mut compiled_exprs,
                &mut errs,
            ) {
                if ty != Ty::Bool {
                    errs.push(Finding::err(
                        "expr/type_mismatch",
                        format!("/transitions/{i}/if"),
                        format!("guard has type {ty}, expected bool"),
                        "write a boolean expression",
                    ));
                }
            }
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
            &BlockOwner::Transition(i),
            &mut compiled_exprs,
            &mut errs,
        );
        let _ = empty;
    }

    if !errs.is_empty() {
        return Err(errs);
    }
    let canonical_def = spec.to_value();
    let canonical = crate::canon::canon_bytes(&canonical_def);
    let machine_id = crate::hashes::machine_id(&canonical_def);
    Ok(CompiledMachine {
        machine_id,
        spec,
        canonical,
        transitions_by,
        compiled_exprs,
    })
}

/// Compile a definition using the accepted source document as the identity input.
pub fn compile_accepted(source: &Value) -> Result<CompiledMachine, Vec<Finding>> {
    if crate::canon::canon_bytes(source).len() > limits::MAX_DEF_BYTES {
        return Err(vec![Finding::err(
            "def/limit_bytes",
            "/",
            "definition exceeds 256 KiB",
            "shrink the document",
        )]);
    }
    let spec = parse_machine(source)?;
    let mut compiled = compile(spec)?;
    compiled.canonical = crate::canon::canon_bytes(source);
    compiled.machine_id = crate::hashes::machine_id(source);
    Ok(compiled)
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
