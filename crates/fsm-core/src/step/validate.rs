use std::collections::BTreeMap;

use crate::expr::eval::Val;
use crate::json::Value;
use crate::machine::CompiledMachine;
use crate::spec::TySpec;
use crate::trace::DecisionTrace;

use super::Rejection;

pub fn validate_event(
    m: &CompiledMachine,
    name: &str,
    payload: &Value,
) -> Result<BTreeMap<String, Val>, Rejection> {
    let ev = m
        .spec
        .events
        .iter()
        .find(|e| e.name == name)
        .ok_or_else(|| {
            let mut r = reject("req/event_unknown", name);
            if let Some(s) =
                crate::ident::suggest(name, m.spec.events.iter().map(|e| e.name.as_str()))
            {
                r.hint = format!("did you mean `{s}`?");
            }
            r
        })?;
    let obj = match payload {
        Value::Obj(o) => o.clone(),
        _ => {
            return Err(reject("req/field_type", "payload must be an object"));
        }
    };
    let mut out = BTreeMap::new();
    for f in &ev.fields {
        let Some(raw) = obj.get(&f.name) else {
            return Err(reject("req/field_missing", &f.name));
        };
        if raw.as_num().is_some() {
            return Err(reject("req/number_token", &f.name));
        }
        let v = parse_typed(raw, &f.ty).map_err(|c| reject(c, &f.name))?;
        if let Val::Enum { ty, variant } = &v {
            let allowed = m.spec.enums.get(ty).cloned().unwrap_or_default();
            if !allowed.iter().any(|x| x == variant) {
                return Err(reject("req/field_type", &f.name));
            }
        }
        if let (Val::Dec(d), TySpec::Dec { scale }) = (&v, &f.ty) {
            if d.scale != *scale {
                return Err(reject("req/field_scale", &f.name));
            }
        }
        out.insert(f.name.clone(), v);
    }
    for k in obj.keys() {
        if !ev.fields.iter().any(|f| f.name == *k) {
            return Err(reject("req/field_unknown", k));
        }
    }
    Ok(out)
}

pub(super) fn reject(code: &'static str, what: &str) -> Rejection {
    Rejection {
        code,
        message: format!("{code}: {what}"),
        hint: what.into(),
        source_state: None,
        transition_idx: None,
        block: None,
        span: None,
        trace: DecisionTrace::default(),
        cause: None,
    }
}

pub(super) fn invalid_state_rejection(detail: &str) -> Rejection {
    let mut rejection = reject("run/configuration_invalid", detail);
    rejection.hint = "reconstruct the state from a trusted create/step/poll result".into();
    rejection
}

pub(super) fn parse_typed(raw: &Value, ty: &TySpec) -> Result<Val, &'static str> {
    match ty {
        TySpec::Bool => raw.as_bool().map(Val::Bool).ok_or("req/field_type"),
        TySpec::Str => raw
            .as_str()
            .map(|s| Val::Str(s.into()))
            .ok_or("req/field_type"),
        TySpec::Int => {
            let s = raw.as_str().ok_or("req/field_type")?;
            s.parse::<i64>().map(Val::Int).map_err(|_| "req/field_type")
        }
        TySpec::Ts => {
            let s = raw.as_str().ok_or("req/field_type")?;
            s.parse::<i64>().map(Val::Ts).map_err(|_| "req/field_type")
        }
        TySpec::Dur => {
            let s = raw.as_str().ok_or("req/field_type")?;
            s.parse::<i64>().map(Val::Dur).map_err(|_| "req/field_type")
        }
        TySpec::Dec { scale } => {
            let s = raw.as_str().ok_or("req/field_type")?;
            match crate::decimal::Dec::parse(s, *scale) {
                Ok(d) => Ok(Val::Dec(d)),
                Err(crate::decimal::DecError::Parse) => {
                    // too many fraction digits
                    if s.contains('.')
                        && s.split('.').nth(1).map(|f| f.len()).unwrap_or(0) > *scale as usize
                    {
                        Err("req/field_scale")
                    } else {
                        Err("req/field_type")
                    }
                }
                Err(_) => Err("req/field_type"),
            }
        }
        TySpec::Enum { of } => {
            let s = raw.as_str().ok_or("req/field_type")?;
            Ok(Val::Enum {
                ty: of.clone(),
                variant: s.into(),
            })
        }
    }
}
