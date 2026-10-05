//! Full handler contract identity; never sanitized compatibility-report identity.

use super::{Advance, HandlerKind, HandlerSpec};
use fsm_core::hashes::domain_hash;
use fsm_core::json::Value;
use fsm_core::sha256::to_hex;
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn fingerprint(handler: &HandlerSpec) -> String {
    format!(
        "sha256:{}",
        to_hex(&domain_hash("fsm:handler-contract:1", &material(handler)))
    )
}

pub(super) fn material(handler: &HandlerSpec) -> Value {
    let (tool, arguments) = match &handler.kind {
        HandlerKind::Process => (Value::Null, Value::Null),
        HandlerKind::Mcp { tool, arguments } => (Value::Str(tool.clone()), arguments.clone()),
    };
    let classes: BTreeSet<_> = handler.retry.on.iter().cloned().collect();
    object([
        ("format", Value::Str("fsm.handler-contract/1".into())),
        ("effect", Value::Str(handler.effect.clone())),
        ("kind", Value::Str(handler.kind.as_str().into())),
        ("argv", strings(handler.argv.iter().cloned())),
        ("tool", tool),
        ("arguments", arguments),
        ("timeout_ms", Value::Num(handler.timeout_ms.to_string())),
        ("on_ok", advance(handler.on_ok.as_ref())),
        ("on_failed", advance(handler.on_failed.as_ref())),
        (
            "retry",
            object([
                ("attempts", Value::Num(handler.retry.attempts.to_string())),
                (
                    "backoff_ms",
                    Value::Num(handler.retry.backoff_ms.to_string()),
                ),
                (
                    "max_backoff_ms",
                    Value::Num(handler.retry.max_backoff_ms.to_string()),
                ),
                ("on", strings(classes)),
            ]),
        ),
    ])
}

fn advance(value: Option<&Advance>) -> Value {
    value.map_or(Value::Null, |value| {
        object([
            ("event", Value::Str(value.event.clone())),
            ("payload", value.payload.clone()),
            ("stamps", strings(value.stamps.iter().cloned())),
        ])
    })
}

fn strings(values: impl IntoIterator<Item = String>) -> Value {
    Value::Arr(values.into_iter().map(Value::Str).collect())
}

fn object<const N: usize>(values: [(&str, Value); N]) -> Value {
    Value::Obj(BTreeMap::from(
        values.map(|(name, value)| (name.into(), value)),
    ))
}

pub(super) fn recover(
    value: &Value,
    expected: &str,
) -> Result<HandlerSpec, crate::error::ExecError> {
    use crate::error::ExecError;
    use fsm_core::json::{JsonLimits, parse};
    let refused = || {
        ExecError::new(
            "exec/config",
            "immutable handler contract is invalid or differs from its fingerprint",
        )
    };
    let bytes = crate::value_limits::canonical(value, JsonLimits::DEFAULT.max_bytes)
        .map_err(|_| refused())?;
    parse(&bytes, &JsonLimits::DEFAULT).map_err(|_| refused())?;
    let fields = value.as_obj().ok_or_else(refused)?;
    let keys = [
        "format",
        "effect",
        "kind",
        "argv",
        "tool",
        "arguments",
        "timeout_ms",
        "on_ok",
        "on_failed",
        "retry",
    ];
    if fields.len() != keys.len()
        || keys.iter().any(|key| !fields.contains_key(*key))
        || fields.get("format").and_then(Value::as_str) != Some("fsm.handler-contract/1")
    {
        return Err(refused());
    }
    let mut declaration = fields.clone();
    declaration.remove("format");
    if fields.get("kind").and_then(Value::as_str) == Some("process") {
        for key in ["tool", "arguments"] {
            if declaration.remove(key) != Some(Value::Null) {
                return Err(refused());
            }
        }
    }
    for key in ["on_ok", "on_failed"] {
        if declaration.get(key) == Some(&Value::Null) {
            declaration.remove(key);
        }
    }
    let handler = super::parse_handler(0, &Value::Obj(declaration))?;
    if material(&handler) != *value || fingerprint(&handler) != expected {
        return Err(refused());
    }
    Ok(handler)
}
