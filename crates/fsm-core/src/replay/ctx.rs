use std::collections::BTreeMap;

use crate::expr::eval::Val;
use crate::json::Value;
use crate::record::Record;
use crate::spec::{TySpec, compile, parse_machine};

use super::{ReplayError, RequestSlot, StoreState};

/// Serialize a context value in the **persistence form**: always a string.
///
/// This is the exact inverse of [`parse_ctx_val`] for every declared type, and
/// it is the form the engine's own snapshots and `state_root` hashing use. An
/// embedder persisting [`crate::machine::InstanceState`] in its own store wants this pair.
pub fn ctx_val_string(v: &Val) -> String {
    v.canonical_string()
}

/// Read a context value back from its [`ctx_val_string`] form.
///
/// Returns `None` when `raw` does not denote a value of the declared type.
pub fn parse_ctx_val(ty: &TySpec, raw: &str) -> Option<Val> {
    parse_override(ty, raw)
}

/// Serialize a context value in the **API form**: booleans become JSON
/// booleans, every other type becomes its canonical string.
///
/// This is the shape that appears in tool responses and CLI output. It is
/// deliberately *not* the inverse of [`parse_ctx_val`], which reads strings
/// only — read this form back with [`parse_ctx_json`], or persist with the
/// [`ctx_val_string`]/[`parse_ctx_val`] pair instead.
pub fn ctx_val_json(v: &Val) -> Value {
    match v {
        Val::Bool(b) => Value::Bool(*b),
        other => Value::Str(other.canonical_string()),
    }
}

/// Read a context value back from its [`ctx_val_json`] form.
///
/// Accepts the JSON boolean that [`ctx_val_json`] emits for [`TySpec::Bool`],
/// and otherwise defers to [`parse_ctx_val`]. Returns `None` when `v` does not
/// denote a value of the declared type.
pub fn parse_ctx_json(ty: &TySpec, v: &Value) -> Option<Val> {
    match (ty, v) {
        (TySpec::Bool, Value::Bool(b)) => Some(Val::Bool(*b)),
        (_, Value::Str(s)) => parse_ctx_val(ty, s),
        _ => None,
    }
}

fn parse_override(ty: &TySpec, raw: &str) -> Option<Val> {
    match ty {
        TySpec::Int => raw.parse().ok().map(Val::Int),
        TySpec::Bool => match raw {
            "true" => Some(Val::Bool(true)),
            "false" => Some(Val::Bool(false)),
            _ => None,
        },
        TySpec::Str => Some(Val::Str(raw.into())),
        TySpec::Ts => raw.parse().ok().map(Val::Ts),
        TySpec::Dur => raw.parse().ok().map(Val::Dur),
        TySpec::Dec { scale } => crate::decimal::Dec::parse(raw, *scale).ok().map(Val::Dec),
        // `canonical_string` writes enums qualified (`tier.premium`), so the
        // inverse must strip the type prefix — parsing the whole string as the
        // variant re-qualifies it on every round-trip and silently drifts the
        // value. The bare form is accepted too, since that is what a caller
        // supplying an override by hand writes. Identifiers cannot contain a
        // dot, so the split is unambiguous.
        TySpec::Enum { of } => {
            let variant = match raw.split_once('.') {
                Some((ty, v)) if ty == of => v,
                Some(_) => return None,
                None => raw,
            };
            Some(Val::Enum {
                ty: of.clone(),
                variant: variant.into(),
            })
        }
    }
}

pub(super) fn overrides_from(
    ctx: &[crate::spec::CtxVar],
    raw: Option<&Value>,
) -> Option<BTreeMap<String, Val>> {
    let Some(v) = raw else {
        return Some(BTreeMap::new());
    };
    let obj = v.as_obj()?;
    let mut out = BTreeMap::new();
    for (k, val) in obj {
        let decl = ctx.iter().find(|c| c.name == *k)?;
        let s = val.as_str()?;
        out.insert(k.clone(), parse_override(&decl.ty, s)?);
    }
    Some(out)
}

pub(super) fn claim_request_id(st: &mut StoreState, rec: &Record) -> Result<(), ReplayError> {
    if let Some(rid) = rec.body.get("request_id").and_then(Value::as_str) {
        if st.dedup.contains_key(rid) {
            return Err(ReplayError::FieldMismatch {
                seq: rec.seq,
                field: "request_id",
            });
        }
        let fp = rec
            .body
            .get("request_fp")
            .and_then(Value::as_str)
            .map(str::to_string);
        st.dedup
            .insert(rid.into(), RequestSlot { seq: rec.seq, fp });
    }
    Ok(())
}
