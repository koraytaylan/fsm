//! Domain-separated content hashes and machine identity.

#![allow(unused_imports)]

use std::collections::BTreeMap;

use crate::canon::canon_bytes;
use crate::json::{Value, write_canonical};
use crate::machine::{ActiveConfiguration, InstanceState, Status};
use crate::sha256::{sha256, to_hex};

pub fn domain_hash(tag: &str, v: &Value) -> [u8; 32] {
    let mut buf = Vec::new();
    buf.extend_from_slice(tag.as_bytes());
    buf.push(0x0A);
    buf.extend_from_slice(&canon_bytes(v));
    sha256(&buf)
}

pub fn machine_id(canonical_def: &Value) -> String {
    let name = canonical_def
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("machine");
    let hex = to_hex(&domain_hash("fsm:machine:1", canonical_def));
    format!("{name}@sha256:{hex}")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolveError {
    TooShort,
    Ambiguous(Vec<String>),
    NotFound,
}

pub fn resolve_machine_ref<'a>(
    ids: impl Iterator<Item = &'a str>,
    query: &str,
) -> Result<String, ResolveError> {
    let all: Vec<&'a str> = ids.collect();
    if let Some(exact) = all.iter().find(|id| **id == query) {
        return Ok((*exact).to_string());
    }
    if let Some((name, rest)) = query.split_once('@') {
        let prefix = rest.strip_prefix("sha256:").unwrap_or(rest);
        if prefix.len() < 12 && !prefix.is_empty() {
            return Err(ResolveError::TooShort);
        }
        let hits: Vec<String> = all
            .iter()
            .filter_map(|id| {
                let (n, h) = id.split_once("@sha256:")?;
                if n == name && h.starts_with(prefix) {
                    Some((*id).to_string())
                } else {
                    None
                }
            })
            .collect();
        return match hits.len() {
            0 => Err(ResolveError::NotFound),
            1 => Ok(hits[0].clone()),
            _ => Err(ResolveError::Ambiguous(hits)),
        };
    }
    // unique hash prefix (at least 12 hex digits) — short hex-looking tokens are names
    if query.len() >= 12 && query.chars().all(|c| c.is_ascii_hexdigit()) {
        let hits: Vec<String> = all
            .iter()
            .filter_map(|id| {
                let h = id.split_once("@sha256:")?.1;
                if h.starts_with(query) {
                    Some((*id).to_string())
                } else {
                    None
                }
            })
            .collect();
        return match hits.len() {
            0 => Err(ResolveError::NotFound),
            1 => Ok(hits[0].clone()),
            _ => Err(ResolveError::Ambiguous(hits)),
        };
    }
    // bare name
    let hits: Vec<String> = all
        .iter()
        .filter(|id| id.split('@').next() == Some(query))
        .map(|s| (*s).to_string())
        .collect();
    match hits.len() {
        0 => {
            if query.chars().all(|c| c.is_ascii_hexdigit()) && query.len() < 12 {
                Err(ResolveError::TooShort)
            } else {
                Err(ResolveError::NotFound)
            }
        }
        1 => Ok(hits[0].clone()),
        _ => Err(ResolveError::Ambiguous(hits)),
    }
}

/// Fingerprint of a request's *content*, for idempotency-key checking.
///
/// A `request_id` is an idempotency key: resending it must replay the original
/// outcome. That is only sound while the request is the same request. This
/// digest binds everything that decides the outcome — the operation, the
/// instance it targets, and the operation's own arguments — so that reusing a
/// key for different content is detectable and can be rejected
/// (`req/request_id_conflict`) instead of silently replaying an unrelated
/// result.
///
/// Deliberately excluded: `expect_seq`, which is a concurrency precondition
/// rather than request content, and the wall clock, which must not make an
/// honest retry look like a new request.
pub fn request_fp(operation: &str, fields: &BTreeMap<String, Value>) -> String {
    let obj = BTreeMap::from([
        ("operation".into(), Value::Str(operation.into())),
        ("fields".into(), Value::Obj(fields.clone())),
    ]);
    format!(
        "sha256:{}",
        to_hex(&domain_hash("fsm:request-fp:1", &Value::Obj(obj)))
    )
}

/// Format discriminator for newly written logical instance-state hashes.
pub const STATE_FORMAT: &str = "fsm.state/2";
/// Domain-separation tag paired with [`STATE_FORMAT`].
pub const STATE_DOMAIN: &str = "fsm:state:2";

/// Encode an active configuration in the canonical tagged JSON shape.
pub fn configuration_value(configuration: &ActiveConfiguration) -> Value {
    match configuration {
        ActiveConfiguration::Sequential { leaf } => Value::Obj(BTreeMap::from([
            ("kind".into(), Value::Str("sequential".into())),
            ("leaf".into(), Value::Str(leaf.clone())),
        ])),
        ActiveConfiguration::Parallel { leaves } => Value::Obj(BTreeMap::from([
            ("kind".into(), Value::Str("parallel".into())),
            (
                "leaves".into(),
                Value::Obj(
                    leaves
                        .iter()
                        .map(|(region, leaf)| (region.clone(), Value::Str(leaf.clone())))
                        .collect(),
                ),
            ),
        ])),
    }
}

/// Hash a complete logical instance state using the current state format.
///
/// The hash binds the active configuration and absolute deadline schedules in
/// addition to context, history, lifecycle status, and pending effects.
pub fn state_hash(machine_id: &str, instance_id: &str, seq: u64, st: &InstanceState) -> String {
    let mut ctx = BTreeMap::new();
    for (k, v) in &st.ctx {
        ctx.insert(k.clone(), Value::Str(v.canonical_string()));
    }
    let mut hist = BTreeMap::new();
    for (k, v) in &st.history {
        hist.insert(k.clone(), Value::Str(v.clone()));
    }
    let mut pending: Vec<String> = st.pending.clone();
    pending.sort();
    let deadlines = st
        .deadlines
        .iter()
        .map(|(name, due_ms)| (name.clone(), Value::Num(due_ms.to_string())))
        .collect();
    let mut obj = BTreeMap::new();
    obj.insert("format".into(), Value::Str(STATE_FORMAT.into()));
    obj.insert("machine_id".into(), Value::Str(machine_id.into()));
    obj.insert("instance_id".into(), Value::Str(instance_id.into()));
    obj.insert("seq".into(), Value::Num(seq.to_string()));
    obj.insert("status".into(), Value::Str(st.status.as_str().into()));
    obj.insert(
        "configuration".into(),
        configuration_value(&st.configuration),
    );
    obj.insert("ctx".into(), Value::Obj(ctx));
    obj.insert("history".into(), Value::Obj(hist));
    obj.insert("deadlines".into(), Value::Obj(deadlines));
    obj.insert(
        "pending".into(),
        Value::Arr(pending.into_iter().map(Value::Str).collect()),
    );
    let hex = to_hex(&domain_hash(STATE_DOMAIN, &Value::Obj(obj)));
    format!("sha256:{hex}")
}

/// Historical state hash used only to verify records written before store
/// VERSION 8. New writes always use [`state_hash`].
pub fn legacy_state_hash(
    machine_id: &str,
    instance_id: &str,
    seq: u64,
    state: &InstanceState,
) -> Option<String> {
    let ActiveConfiguration::Sequential { leaf } = &state.configuration else {
        return None;
    };
    let context = state
        .ctx
        .iter()
        .map(|(name, value)| (name.clone(), Value::Str(value.canonical_string())))
        .collect();
    let history = state
        .history
        .iter()
        .map(|(owner, bound)| (owner.clone(), Value::Str(bound.clone())))
        .collect();
    let mut pending = state.pending.clone();
    pending.sort();
    let material = Value::Obj(BTreeMap::from([
        ("format".into(), Value::Str("fsm.state/1".into())),
        ("machine_id".into(), Value::Str(machine_id.into())),
        ("instance_id".into(), Value::Str(instance_id.into())),
        ("seq".into(), Value::Num(seq.to_string())),
        ("status".into(), Value::Str(state.status.as_str().into())),
        ("state".into(), Value::Str(leaf.clone())),
        ("ctx".into(), Value::Obj(context)),
        ("history".into(), Value::Obj(history)),
        (
            "pending".into(),
            Value::Arr(pending.into_iter().map(Value::Str).collect()),
        ),
    ]));
    Some(format!(
        "sha256:{}",
        to_hex(&domain_hash("fsm:state:1", &material))
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::json::{JsonLimits, parse};

    #[test]
    fn domain_separation_tiny() {
        // Framing: tag ‖ 0x0A ‖ canon("{}") = 7b 7d
        let v = parse(b"{}", &JsonLimits::DEFAULT).unwrap();
        let a = domain_hash("fsm:machine:1", &v);
        let b = domain_hash("fsm:state:1", &v);
        assert_ne!(a, b);
        let mut framed = b"fsm:machine:1".to_vec();
        framed.push(0x0A);
        framed.extend_from_slice(b"{}");
        assert_eq!(a, crate::sha256::sha256(&framed));
    }
}
