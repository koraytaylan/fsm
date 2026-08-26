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
pub const STATE_FORMAT_V2: &str = "fsm.state/2";
/// Domain-separation tag paired with [`STATE_FORMAT_V2`].
pub const STATE_DOMAIN_V2: &str = "fsm:state:2";

/// The state format every new record carries: the v2 payload plus
/// `invocations` and `signals`, both always present even when empty.
///
/// One version string means one payload for its whole life. Records written
/// before the bump keep `fsm.state/2` and verify under the v2 function
/// forever; nothing is ever rewritten, and no reader guesses from a record's
/// age — the discriminator is in the record.
pub const STATE_FORMAT: &str = "fsm.state/3";
/// Domain-separation tag paired with [`STATE_FORMAT`].
pub const STATE_DOMAIN: &str = "fsm:state:3";

/// Alias of [`STATE_FORMAT`] for readers that name the version explicitly.
pub const STATE_FORMAT_V3: &str = STATE_FORMAT;
/// Alias of [`STATE_DOMAIN`].
pub const STATE_DOMAIN_V3: &str = STATE_DOMAIN;

/// The 64-hex digest half of a `machine_id`, which is what an `invoke` slot
/// names: `name@sha256:<digest>`.
pub fn digest_of(machine_id: &str) -> Option<&str> {
    machine_id.rsplit_once("sha256:").map(|(_, digest)| digest)
}

/// Domain tag of the derived child instance id.
pub const CHILD_DOMAIN: &str = "fsm:child:1";

/// The instance id an invocation slot's child will have: a pure function of
/// the parent's id and the slot name.
///
/// Deriving rather than allocating is what makes enactment idempotent without
/// consulting anything — re-running it computes the same id and the store
/// answers `duplicate: true` — and lets any reader compute a child's id from
/// its parent's id and the definition alone. The framing follows this
/// workspace's domain-separation convention (tag, then `0x0A`), with a `0x00`
/// between the two variable-length parts so no `(parent, slot)` pair can be
/// read as another.
pub fn child_instance_id(parent_instance_id: &str, slot: &str) -> String {
    let mut material = CHILD_DOMAIN.as_bytes().to_vec();
    material.push(0x0A);
    material.extend_from_slice(parent_instance_id.as_bytes());
    material.push(0x00);
    material.extend_from_slice(slot.as_bytes());
    format!("inst-{}", &to_hex(&crate::sha256::sha256(&material))[..24])
}

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
    state_hash_v3(machine_id, instance_id, seq, st)
}

/// The `fsm.state/2` identity, kept for the records that declare it.
pub fn state_hash_v2(machine_id: &str, instance_id: &str, seq: u64, st: &InstanceState) -> String {
    let material = state_material(STATE_FORMAT_V2, machine_id, instance_id, seq, st);
    let hex = to_hex(&domain_hash(STATE_DOMAIN_V2, &material));
    format!("sha256:{hex}")
}

/// [`STATE_FORMAT`] state identity: the v2 payload plus `invocations` and
/// `signals`, each canonically ordered and always present.
///
/// The empty maps are written, not omitted. An omit-when-empty rule would be
/// a second thing about the format to remember and buys nothing: unlike the
/// record's `microsteps` key, whose absence had to preserve bytes an earlier
/// build wrote, this format is new and nothing depends on its silence.
pub fn state_hash_v3(machine_id: &str, instance_id: &str, seq: u64, st: &InstanceState) -> String {
    let mut material = state_material(STATE_FORMAT, machine_id, instance_id, seq, st);
    if let Value::Obj(obj) = &mut material {
        obj.insert("invocations".into(), invocations_value(st));
        obj.insert("signals".into(), signals_value(st));
    }
    let hex = to_hex(&domain_hash(STATE_DOMAIN, &material));
    format!("sha256:{hex}")
}

/// Invocation slots as canonical material, ordered by slot id.
///
/// The child's instance id is not among the committed fields: it is
/// [`child_instance_id`] of `instance_id` and the slot key, both of which the
/// payload already commits, so hashing it again would commit nothing new.
pub fn invocations_value(st: &InstanceState) -> Value {
    Value::Obj(
        st.invocations
            .iter()
            .map(|(slot, invocation)| {
                let overrides = invocation
                    .overrides
                    .iter()
                    .map(|(name, value)| (name.clone(), Value::Str(value.canonical_string())))
                    .collect();
                (
                    slot.clone(),
                    Value::Obj(BTreeMap::from([
                        (
                            "child_machine_id".into(),
                            Value::Str(invocation.child_machine_id.clone()),
                        ),
                        (
                            "status".into(),
                            Value::Str(invocation.status.as_str().into()),
                        ),
                        ("overrides".into(), Value::Obj(overrides)),
                    ])),
                )
            })
            .collect(),
    )
}

/// Undelivered signals as canonical material, ordered by signal id.
pub fn signals_value(st: &InstanceState) -> Value {
    Value::Obj(
        st.signals
            .iter()
            .map(|(id, signal)| {
                let payload = signal
                    .payload
                    .iter()
                    .map(|(name, value)| (name.clone(), Value::Str(value.canonical_string())))
                    .collect();
                (
                    id.clone(),
                    Value::Obj(BTreeMap::from([
                        (
                            "target_instance_id".into(),
                            Value::Str(signal.target_instance_id.clone()),
                        ),
                        ("event".into(), Value::Str(signal.event.clone())),
                        ("payload".into(), Value::Obj(payload)),
                    ])),
                )
            })
            .collect(),
    )
}

/// The keys `fsm.state/2` and `fsm.state/3` share, in canonical form.
fn state_material(
    format: &str,
    machine_id: &str,
    instance_id: &str,
    seq: u64,
    st: &InstanceState,
) -> Value {
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
    obj.insert("format".into(), Value::Str(format.into()));
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
    Value::Obj(obj)
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
