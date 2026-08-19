use std::collections::BTreeMap;

use fsm_core::analyze::EventStatus;
use fsm_core::expr::eval::Val;
use fsm_core::json::Value;
use fsm_core::spec::{MachineSpec, TySpec};

use super::ErrorObj;

pub fn enabled_json(evs: &[fsm_core::analyze::EventReport]) -> Value {
    Value::Arr(
        evs.iter()
            .map(|e| {
                let mut m = BTreeMap::new();
                m.insert("event".into(), Value::Str(e.event.clone()));
                m.insert(
                    "status".into(),
                    Value::Str(
                        match e.status {
                            EventStatus::Enabled => "enabled",
                            EventStatus::Disabled => "disabled",
                            EventStatus::DependsOnPayload => "depends_on_payload",
                            EventStatus::Preempted => "preempted",
                            EventStatus::PreemptedMaybe => "preempted_maybe",
                        }
                        .into(),
                    ),
                );
                if !e.payload_fields.is_empty() {
                    m.insert(
                        "payload_fields".into(),
                        Value::Arr(e.payload_fields.iter().cloned().map(Value::Str).collect()),
                    );
                }
                Value::Obj(m)
            })
            .collect(),
    )
}

pub fn context_not_object(got: &str) -> ErrorObj {
    let mut details = BTreeMap::new();
    details.insert("field".into(), Value::Str("context".into()));
    details.insert("expected".into(), Value::Str("object".into()));
    details.insert("got".into(), Value::Str(got.into()));
    ErrorObj::new("req/args_invalid", "expected object")
        .hint("set context to object")
        .details(Value::Obj(details))
}

pub fn number_token_error(field: &str) -> ErrorObj {
    ErrorObj::new("req/number_token", field).hint(format!("send {field} as a JSON string"))
}

pub fn apply_context_overrides(
    spec: &MachineSpec,
    ctx: &BTreeMap<String, Value>,
) -> Result<BTreeMap<String, Val>, ErrorObj> {
    let mut overrides = BTreeMap::new();
    for (k, val) in ctx {
        let raw = match val {
            Value::Str(s) => s.clone(),
            Value::Num(_) => return Err(number_token_error(k)),
            Value::Bool(b) => b.to_string(),
            _ => return Err(ErrorObj::new("req/field_type", k.clone())),
        };
        let decl = spec
            .context
            .iter()
            .find(|c| c.name == *k)
            .ok_or_else(|| ErrorObj::new("req/field_unknown", k.clone()))?;
        overrides.insert(k.clone(), coerce_ctx_override(&decl.ty, k, &raw)?);
    }
    Ok(overrides)
}

pub fn coerce_ctx_override(ty: &TySpec, key: &str, raw: &str) -> Result<Val, ErrorObj> {
    match ty {
        TySpec::Bool => match raw {
            "true" => Ok(Val::Bool(true)),
            "false" => Ok(Val::Bool(false)),
            _ => Err(ErrorObj::new("req/field_type", key)),
        },
        TySpec::Int => raw
            .parse::<i64>()
            .map(Val::Int)
            .map_err(|_| ErrorObj::new("req/field_type", key)),
        TySpec::Str => Ok(Val::Str(raw.into())),
        TySpec::Ts => raw
            .parse::<i64>()
            .map(Val::Ts)
            .map_err(|_| ErrorObj::new("req/field_type", key)),
        TySpec::Dur => raw
            .parse::<i64>()
            .map(Val::Dur)
            .map_err(|_| ErrorObj::new("req/field_type", key)),
        TySpec::Dec { scale } => match fsm_core::decimal::Dec::parse(raw, *scale) {
            Ok(d) => Ok(Val::Dec(d)),
            Err(_) => {
                if raw.contains('.')
                    && raw.split('.').nth(1).map(|f| f.len()).unwrap_or(0) > *scale as usize
                {
                    Err(ErrorObj::new("req/field_scale", key)
                        .hint(format!("use exactly {scale} fraction digits")))
                } else {
                    Err(ErrorObj::new("req/field_type", key))
                }
            }
        },
        // Shares the reader in core so a hand-supplied override and a
        // journalled one parse identically; accepts `premium` and `tier.premium`.
        TySpec::Enum { .. } => fsm_core::replay::parse_ctx_val(ty, raw)
            .ok_or_else(|| ErrorObj::new("req/field_type", key)),
    }
}

#[allow(dead_code)]
fn obj(pairs: &[(&str, &str)]) -> Value {
    let mut m = BTreeMap::new();
    for (k, v) in pairs {
        m.insert((*k).into(), Value::Str((*v).into()));
    }
    Value::Obj(m)
}
