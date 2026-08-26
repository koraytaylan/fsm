use crate::json::Value;

use super::super::{CtxVar, EffectDecl, EventDecl, FieldDecl, Finding, SupersedesSpec};
use super::transitions::parse_ty_spec;
use super::{check_keys, req_str};

pub(super) fn parse_ctx(v: Option<&Value>, errs: &mut Vec<Finding>) -> Vec<CtxVar> {
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

/// Which declaration list is being parsed; only events accept `internal`.
#[derive(Clone, Copy)]
enum Declared {
    Events,
    Effects,
}

struct NamedEntry {
    name: String,
    fields: Vec<FieldDecl>,
    internal: bool,
}

fn parse_named_list(
    v: Option<&Value>,
    path: &str,
    declared: Declared,
    errs: &mut Vec<Finding>,
) -> Option<Vec<NamedEntry>> {
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
                    match declared {
                        Declared::Events => {
                            check_keys(obj, &["name", "fields", "internal"], &p, errs)
                        }
                        Declared::Effects => check_keys(obj, &["name", "fields"], &p, errs),
                    }
                    let name = req_str(obj, "name", &format!("{p}/name"), errs)
                        .unwrap_or("")
                        .to_string();
                    let internal = match obj.get("internal") {
                        None => false,
                        Some(Value::Bool(internal)) => *internal,
                        Some(_) => {
                            errs.push(Finding::err(
                                "def/shape",
                                format!("{p}/internal"),
                                "internal must be a boolean",
                                "use true for an event only the machine raises, or omit it",
                            ));
                            false
                        }
                    };
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
                    out.push(NamedEntry {
                        name,
                        fields,
                        internal,
                    });
                }
                Some(out)
            }
        },
    }
}

/// The optional top-level `supersedes` block.
///
/// `machine` is required; `states` and `context` are optional objects and an
/// empty one is legal — a mapping that covers nothing migrates nothing, which
/// is a coherent thing for an author to say.
pub(super) fn parse_supersedes(
    v: Option<&Value>,
    errs: &mut Vec<Finding>,
) -> Option<SupersedesSpec> {
    let v = v?;
    let Some(obj) = v.as_obj() else {
        errs.push(Finding::err(
            "def/shape",
            "/supersedes",
            "supersedes must be an object",
            "use {machine, states?, context?}",
        ));
        return None;
    };
    super::check_keys(obj, &["machine", "states", "context"], "/supersedes", errs);
    let machine = super::req_str(obj, "machine", "/supersedes/machine", errs)
        .unwrap_or("")
        .to_string();
    let pairs = |key: &str, errs: &mut Vec<Finding>| -> Vec<(String, String)> {
        let Some(value) = obj.get(key) else {
            return Vec::new();
        };
        let Some(map) = value.as_obj() else {
            errs.push(Finding::err(
                "def/shape",
                format!("/supersedes/{key}"),
                format!("{key} must be an object"),
                "map each name to a string",
            ));
            return Vec::new();
        };
        let mut out = Vec::new();
        for (from, to) in map {
            match to {
                Value::Str(text) => out.push((from.clone(), text.clone())),
                _ => errs.push(Finding::err(
                    "def/shape",
                    format!("/supersedes/{key}/{from}"),
                    "value must be a string",
                    "quote it",
                )),
            }
        }
        out
    };
    let states = pairs("states", errs);
    let context = pairs("context", errs);
    Some(SupersedesSpec {
        machine,
        states,
        context,
    })
}

pub(super) fn parse_events(v: Option<&Value>, errs: &mut Vec<Finding>) -> Vec<EventDecl> {
    parse_named_list(v, "/events", Declared::Events, errs)
        .unwrap_or_default()
        .into_iter()
        .map(|entry| EventDecl {
            name: entry.name,
            fields: entry.fields,
            internal: entry.internal,
        })
        .collect()
}

pub(super) fn parse_effects(v: Option<&Value>, errs: &mut Vec<Finding>) -> Vec<EffectDecl> {
    parse_named_list(v, "/effects", Declared::Effects, errs)
        .unwrap_or_default()
        .into_iter()
        .map(|entry| EffectDecl {
            name: entry.name,
            fields: entry.fields,
        })
        .collect()
}
