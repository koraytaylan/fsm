//! Full handler contract identity; never sanitized compatibility-report identity.

use super::{Advance, HandlerKind, HandlerSpec};
use fsm_core::hashes::domain_hash;
use fsm_core::json::Value;
use fsm_core::sha256::to_hex;
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn fingerprint(handler: &HandlerSpec) -> String {
    let (tool, arguments) = match &handler.kind {
        HandlerKind::Process => (Value::Null, Value::Null),
        HandlerKind::Mcp { tool, arguments } => (Value::Str(tool.clone()), arguments.clone()),
    };
    let classes: BTreeSet<_> = handler.retry.on.iter().cloned().collect();
    let material = object([
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
    ]);
    format!(
        "sha256:{}",
        to_hex(&domain_hash("fsm:handler-contract:1", &material))
    )
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
