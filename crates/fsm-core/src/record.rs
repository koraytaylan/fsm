//! Journal record envelope: seal, verify, and record kinds.

#![allow(clippy::should_implement_trait, clippy::byte_char_slices, unused_mut)]

use std::collections::BTreeMap;

use crate::hashes::domain_hash;
use crate::json::{JsonLimits, Value, parse};
use crate::sha256::to_hex;
use crate::trace::{MicrostepTrace, MicrostepTrigger};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RecordKind {
    Genesis,
    MachineDefined,
    InstanceCreated,
    EventApplied,
    EventRejected,
    EventIgnored,
    /// A due deadline atomically applied its transition pipeline.
    DeadlineApplied,
    /// A selected deadline was rejected without changing instance state.
    DeadlineRejected,
    /// An explicit poll found no due deadline but durably claimed its request id.
    DeadlineNotDue,
    EffectAcked,
    /// One record creates a child instance for an invocation slot: the
    /// child's whole existence is derived from this record's body and `ts`,
    /// so there is no separate `instance_created` for it.
    InstanceInvoked,
    RequestRejected,
    InstanceCancelled,
    Annotated,
    StateCheckpoint,
}

/// Every instance a record is about, in the order a reader should see them.
///
/// The composition records name their instances `parent_instance_id` and
/// `child_instance_id`; **none of them has a field called `instance_id`**, so
/// every `body.get("instance_id")` probe would silently drop them and a
/// parent's history would show the event that entered the invoking state and
/// then nothing about the child ever existing. Matching exhaustively over the
/// kinds, here where they are defined, is what forces each later plan to
/// answer this question for the kinds it adds.
pub fn instances_touched(record: &Record) -> Vec<&str> {
    let field = |name: &str| record.body.get(name).and_then(Value::as_str);
    match record.kind {
        RecordKind::Genesis | RecordKind::MachineDefined | RecordKind::StateCheckpoint => {
            Vec::new()
        }
        RecordKind::InstanceCreated
        | RecordKind::EventApplied
        | RecordKind::EventRejected
        | RecordKind::EventIgnored
        | RecordKind::DeadlineApplied
        | RecordKind::DeadlineRejected
        | RecordKind::DeadlineNotDue
        | RecordKind::EffectAcked
        | RecordKind::RequestRejected
        | RecordKind::InstanceCancelled
        | RecordKind::Annotated => field("instance_id").into_iter().collect(),
        RecordKind::InstanceInvoked => field("parent_instance_id")
            .into_iter()
            .chain(field("child_instance_id"))
            .collect(),
    }
}

impl RecordKind {
    pub fn as_str(self) -> &'static str {
        match self {
            RecordKind::Genesis => "genesis",
            RecordKind::MachineDefined => "machine_defined",
            RecordKind::InstanceCreated => "instance_created",
            RecordKind::EventApplied => "event_applied",
            RecordKind::EventRejected => "event_rejected",
            RecordKind::EventIgnored => "event_ignored",
            RecordKind::DeadlineApplied => "deadline_applied",
            RecordKind::DeadlineRejected => "deadline_rejected",
            RecordKind::DeadlineNotDue => "deadline_not_due",
            RecordKind::EffectAcked => "effect_acked",
            RecordKind::InstanceInvoked => "instance_invoked",
            RecordKind::RequestRejected => "request_rejected",
            RecordKind::InstanceCancelled => "instance_cancelled",
            RecordKind::Annotated => "annotated",
            RecordKind::StateCheckpoint => "state_checkpoint",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        Some(match s {
            "genesis" => Self::Genesis,
            "machine_defined" => Self::MachineDefined,
            "instance_created" => Self::InstanceCreated,
            "event_applied" => Self::EventApplied,
            "event_rejected" => Self::EventRejected,
            "event_ignored" => Self::EventIgnored,
            "deadline_applied" => Self::DeadlineApplied,
            "deadline_rejected" => Self::DeadlineRejected,
            "deadline_not_due" => Self::DeadlineNotDue,
            "effect_acked" => Self::EffectAcked,
            "instance_invoked" => Self::InstanceInvoked,
            "request_rejected" => Self::RequestRejected,
            "instance_cancelled" => Self::InstanceCancelled,
            "annotated" => Self::Annotated,
            "state_checkpoint" => Self::StateCheckpoint,
            _ => return None,
        })
    }

    /// Every recognized record kind in stable protocol order.
    pub fn all() -> [RecordKind; 15] {
        [
            Self::Genesis,
            Self::MachineDefined,
            Self::InstanceCreated,
            Self::EventApplied,
            Self::EventRejected,
            Self::EventIgnored,
            Self::DeadlineApplied,
            Self::DeadlineRejected,
            Self::DeadlineNotDue,
            Self::EffectAcked,
            Self::InstanceInvoked,
            Self::RequestRejected,
            Self::InstanceCancelled,
            Self::Annotated,
            Self::StateCheckpoint,
        ]
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    pub seq: u64,
    pub ts: i64,
    pub kind: RecordKind,
    pub body: Value,
    pub prev: String,
    pub hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordError {
    Parse { offset: usize },
    NonCanonical { seq: u64, offset: usize },
    SeqGap { seq: u64, expected: u64 },
    PrevMismatch { seq: u64 },
    HashMismatch { seq: u64 },
    BodyInvalid { seq: u64 },
}

pub fn zeros() -> String {
    "0".repeat(64)
}

/// Exact resource-ceiling object written into new genesis records.
///
/// Verification additionally recognizes the historical object that predates
/// the current definition ceilings, without rewriting its hash material.
pub fn limits_value() -> Value {
    let mut m = BTreeMap::new();
    m.insert(
        "max_states".into(),
        Value::Num(crate::limits::MAX_STATES.to_string()),
    );
    m.insert(
        "max_nesting".into(),
        Value::Num(crate::limits::MAX_NESTING.to_string()),
    );
    m.insert(
        "max_history".into(),
        Value::Num(crate::limits::MAX_HISTORY.to_string()),
    );
    m.insert(
        "max_regions".into(),
        Value::Num(crate::limits::MAX_REGIONS.to_string()),
    );
    m.insert(
        "max_deadlines".into(),
        Value::Num(crate::limits::MAX_DEADLINES.to_string()),
    );
    m.insert(
        "max_eval_ticks".into(),
        Value::Num(crate::limits::MAX_EVAL_TICKS.to_string()),
    );
    m.insert(
        "max_events".into(),
        Value::Num(crate::limits::MAX_EVENTS.to_string()),
    );
    m.insert(
        "max_enums".into(),
        Value::Num(crate::limits::MAX_ENUMS.to_string()),
    );
    m.insert(
        "max_variants".into(),
        Value::Num(crate::limits::MAX_VARIANTS.to_string()),
    );
    m.insert(
        "max_transitions".into(),
        Value::Num(crate::limits::MAX_TRANSITIONS.to_string()),
    );
    m.insert(
        "max_transitions_per_cell".into(),
        Value::Num(crate::limits::MAX_TRANSITIONS_PER_CELL.to_string()),
    );
    m.insert(
        "max_ctx_vars".into(),
        Value::Num(crate::limits::MAX_CTX_VARS.to_string()),
    );
    m.insert(
        "max_fields".into(),
        Value::Num(crate::limits::MAX_FIELDS.to_string()),
    );
    m.insert(
        "max_sets_per_block".into(),
        Value::Num(crate::limits::MAX_SETS_PER_BLOCK.to_string()),
    );
    m.insert(
        "max_emits_per_block".into(),
        Value::Num(crate::limits::MAX_EMITS_PER_BLOCK.to_string()),
    );
    m.insert(
        "max_invariants".into(),
        Value::Num(crate::limits::MAX_INVARIANTS.to_string()),
    );
    m.insert(
        "max_def_bytes".into(),
        Value::Num(crate::limits::MAX_DEF_BYTES.to_string()),
    );
    Value::Obj(m)
}

fn legacy_limits_value() -> Value {
    let Value::Obj(mut limits) = limits_value() else {
        unreachable!("limits_value always returns an object")
    };
    limits.remove("max_regions");
    limits.remove("max_deadlines");
    limits.remove("max_eval_ticks");
    Value::Obj(limits)
}

/// Whether a candidate genesis body carries the exact legacy limits object.
///
/// This checks only the body value. It does not verify record kind, envelope,
/// hash chain, or persistence provenance. Persistence migration uses it only
/// after those checks to authorize
/// [`crate::spec::compile_accepted_historical_unchecked`]. It is not a
/// definition-admission API; partial or otherwise modified limit tables return
/// false.
#[doc(hidden)]
pub fn genesis_uses_historical_definition_limits(body: &Value) -> bool {
    body.get("limits") == Some(&legacy_limits_value())
}

/// The optional `microsteps` body key of `instance_created`, `event_applied`,
/// and `deadline_applied`: one entry per reaction microstep after the trigger.
///
/// `None` when there were no reaction microsteps. The key is **absent, never
/// empty**: a definition with no eventless transition, no `raise`, and no
/// `final` state produces a body with no `microsteps` key, hence identical
/// canonical bytes, an identical record hash, and an identical chain. This
/// one guard is the whole compatibility story of the reactive plan.
pub fn microsteps_value(microsteps: &[MicrostepTrace]) -> Option<Value> {
    if microsteps.is_empty() {
        return None;
    }
    Some(Value::Arr(microsteps.iter().map(microstep_value).collect()))
}

fn microstep_value(microstep: &MicrostepTrace) -> Value {
    let mut m = BTreeMap::new();
    m.insert("index".into(), Value::Num(microstep.index.to_string()));
    m.insert(
        "trigger".into(),
        Value::Str(microstep.trigger.as_str().into()),
    );
    if let MicrostepTrigger::Internal(event) = &microstep.trigger {
        m.insert("event".into(), Value::Str(event.clone()));
    }
    m.insert(
        "source_state".into(),
        Value::Str(microstep.source_state.clone()),
    );
    m.insert(
        "transition_idx".into(),
        Value::Num(microstep.transition_idx.to_string()),
    );
    m.insert(
        "exited".into(),
        Value::Arr(microstep.exited.iter().cloned().map(Value::Str).collect()),
    );
    m.insert(
        "entered".into(),
        Value::Arr(microstep.entered.iter().cloned().map(Value::Str).collect()),
    );
    Value::Obj(m)
}

/// A journaled `microsteps` array, if present: entries indexed from 1 in
/// order, each `eventless` or `internal` (the latter naming its `event`),
/// with the trigger's fields. An empty array is malformed by the absence
/// rule above.
fn microsteps_ok(body: &Value) -> bool {
    let Some(microsteps) = body.get("microsteps") else {
        return true;
    };
    let Some(entries) = microsteps.as_arr() else {
        return false;
    };
    if entries.is_empty() {
        return false;
    }
    entries.iter().enumerate().all(|(position, entry)| {
        let index_ok = entry
            .get("index")
            .and_then(Value::as_num)
            .and_then(|raw| raw.parse::<u32>().ok())
            == Some(position as u32 + 1);
        let trigger_ok = match entry.get("trigger").and_then(Value::as_str) {
            Some("eventless") => entry.get("event").is_none(),
            Some("internal") => req_str(entry, "event"),
            _ => false,
        };
        index_ok
            && trigger_ok
            && req_str(entry, "source_state")
            && req_u32(entry, "transition_idx")
            && req_str_arr(entry, "exited")
            && req_str_arr(entry, "entered")
    })
}

fn genesis_limits_ok(value: Option<&Value>) -> bool {
    value.is_some_and(|value| value == &limits_value() || value == &legacy_limits_value())
}

fn envelope_minus_hash(seq: u64, ts: i64, kind: RecordKind, body: &Value, prev: &str) -> Value {
    let mut m = BTreeMap::new();
    m.insert("seq".into(), Value::Num(seq.to_string()));
    m.insert("ts".into(), Value::Num(ts.to_string()));
    m.insert("kind".into(), Value::Str(kind.as_str().into()));
    m.insert("body".into(), body.clone());
    m.insert("prev".into(), Value::Str(prev.into()));
    Value::Obj(m)
}

pub fn seal(seq: u64, ts: i64, kind: RecordKind, body: Value, prev: &str) -> Record {
    let env = envelope_minus_hash(seq, ts, kind, &body, prev);
    let hash = to_hex(&domain_hash("fsm:record:1", &env));
    Record {
        seq,
        ts,
        kind,
        body,
        prev: prev.into(),
        hash,
    }
}

impl Record {
    pub fn to_value(&self) -> Value {
        let mut m = BTreeMap::new();
        m.insert("seq".into(), Value::Num(self.seq.to_string()));
        m.insert("ts".into(), Value::Num(self.ts.to_string()));
        m.insert("kind".into(), Value::Str(self.kind.as_str().into()));
        m.insert("body".into(), self.body.clone());
        m.insert("prev".into(), Value::Str(self.prev.clone()));
        m.insert("hash".into(), Value::Str(self.hash.clone()));
        Value::Obj(m)
    }

    pub fn to_line(&self) -> Vec<u8> {
        let mut out = crate::canon::canon_bytes(&self.to_value());
        out.push(b'\n');
        out
    }
}

pub fn verify_line(line: &[u8], expect_seq: u64, expect_prev: &str) -> Result<Record, RecordError> {
    let raw = line.strip_suffix(&[b'\n']).unwrap_or(line);
    let v =
        parse(raw, &JsonLimits::DEFAULT).map_err(|e| RecordError::Parse { offset: e.offset })?;
    let obj = v.as_obj().ok_or(RecordError::Parse { offset: 0 })?;
    let kind_s = obj
        .get("kind")
        .and_then(Value::as_str)
        .ok_or(RecordError::Parse { offset: 0 })?;
    let kind = RecordKind::from_str(kind_s).ok_or(RecordError::Parse { offset: 0 })?;
    let seq: u64 = obj
        .get("seq")
        .and_then(Value::as_num)
        .and_then(|s| s.parse().ok())
        .ok_or(RecordError::Parse { offset: 0 })?;
    let ts: i64 = obj
        .get("ts")
        .and_then(Value::as_num)
        .and_then(|s| s.parse().ok())
        .ok_or(RecordError::Parse { offset: 0 })?;
    let prev = obj
        .get("prev")
        .and_then(Value::as_str)
        .ok_or(RecordError::Parse { offset: 0 })?
        .to_string();
    let hash = obj
        .get("hash")
        .and_then(Value::as_str)
        .ok_or(RecordError::Parse { offset: 0 })?
        .to_string();
    let body = obj
        .get("body")
        .cloned()
        .ok_or(RecordError::BodyInvalid { seq })?;
    if seq != expect_seq {
        return Err(RecordError::SeqGap {
            seq,
            expected: expect_seq,
        });
    }
    if prev != expect_prev {
        return Err(RecordError::PrevMismatch { seq });
    }
    let rec = Record {
        seq,
        ts,
        kind,
        body,
        prev,
        hash: hash.clone(),
    };
    let canon = crate::canon::canon_bytes(&rec.to_value());
    if canon != raw {
        return Err(RecordError::NonCanonical { seq, offset: 0 });
    }
    let env = envelope_minus_hash(seq, ts, kind, &rec.body, &rec.prev);
    let want = to_hex(&domain_hash("fsm:record:1", &env));
    if want != hash {
        return Err(RecordError::HashMismatch { seq });
    }
    if !body_ok(kind, &rec.body) {
        return Err(RecordError::BodyInvalid { seq });
    }
    Ok(rec)
}

fn is_hex64(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

fn is_state_hash(v: Option<&Value>) -> bool {
    v.and_then(Value::as_str)
        .and_then(|s| s.strip_prefix("sha256:"))
        .is_some_and(is_hex64)
}

fn req_str(body: &Value, k: &str) -> bool {
    body.get(k).and_then(Value::as_str).is_some()
}

fn req_i64(body: &Value, k: &str) -> bool {
    body.get(k)
        .and_then(Value::as_num)
        .is_some_and(|raw| raw.parse::<i64>().is_ok())
}

fn req_u32(body: &Value, k: &str) -> bool {
    body.get(k)
        .and_then(Value::as_num)
        .is_some_and(|raw| raw.parse::<u32>().is_ok())
}

fn req_str_arr(body: &Value, k: &str) -> bool {
    body.get(k)
        .and_then(Value::as_arr)
        .is_some_and(|values| values.iter().all(|value| value.as_str().is_some()))
}

fn state_format_ok(body: &Value, required: bool) -> bool {
    match body.get("state_format") {
        Some(Value::Str(format)) => format == crate::hashes::STATE_FORMAT,
        None => !required,
        Some(_) => false,
    }
}

fn root_format_ok(body: &Value) -> bool {
    match body.get("state_root_format") {
        Some(Value::Str(format)) => {
            format == "fsm.state-root/3" && is_state_hash(body.get("state_root"))
        }
        None => true,
        Some(_) => false,
    }
}

fn deadline_identity_ok(body: &Value) -> bool {
    req_str(body, "deadline") && req_u32(body, "deadline_idx") && req_i64(body, "due_ms")
}

fn span_ok(v: Option<&Value>) -> bool {
    match v {
        None => true,
        Some(Value::Obj(o)) => {
            o.get("start")
                .and_then(Value::as_num)
                .is_some_and(|raw| raw.parse::<u32>().is_ok())
                && o.get("end")
                    .and_then(Value::as_num)
                    .is_some_and(|raw| raw.parse::<u32>().is_ok())
        }
        Some(_) => false,
    }
}

fn rejection_ok(body: &Value) -> bool {
    req_str(body, "code")
        && req_str(body, "message")
        && req_str(body, "hint")
        && body.get("details").and_then(Value::as_obj).is_some()
        && span_ok(body.get("span"))
}

fn body_ok(kind: RecordKind, body: &Value) -> bool {
    let shape_ok = match kind {
        RecordKind::Genesis => {
            body.get("format").and_then(Value::as_str) == Some("fsm.journal/1")
                && genesis_limits_ok(body.get("limits"))
                && body.get("created_ts").and_then(Value::as_num).is_some()
        }
        RecordKind::MachineDefined => req_str(body, "machine_id") && body.get("def").is_some(),
        RecordKind::InstanceCreated => {
            req_str(body, "instance_id")
                && req_str(body, "machine_id")
                && req_str(body, "request_id")
                && is_state_hash(body.get("state_hash"))
                && body.get("overrides").and_then(Value::as_obj).is_some()
                && match body.get("state_format") {
                    Some(_) => body.get("configuration").and_then(Value::as_obj).is_some(),
                    None => req_str(body, "leaf"),
                }
                && microsteps_ok(body)
        }
        RecordKind::EventApplied => {
            req_str(body, "instance_id")
                && req_str(body, "event")
                && body.get("payload").is_some()
                && req_str(body, "request_id")
                && is_state_hash(body.get("state_hash"))
                && req_str_arr(body, "exited")
                && req_str_arr(body, "entered")
                && req_str(body, "source_state")
                && microsteps_ok(body)
        }
        RecordKind::EventRejected => {
            req_str(body, "instance_id")
                && req_str(body, "request_id")
                && req_str(body, "event")
                && body.get("payload").is_some()
                && is_state_hash(body.get("state_hash"))
                && rejection_ok(body)
        }
        RecordKind::EventIgnored => {
            req_str(body, "instance_id")
                && req_str(body, "request_id")
                && req_str(body, "event")
                && body.get("payload").is_some()
                && is_state_hash(body.get("state_hash"))
        }
        RecordKind::DeadlineApplied => {
            req_str(body, "instance_id")
                && req_str(body, "request_id")
                && deadline_identity_ok(body)
                && is_state_hash(body.get("state_hash"))
                && req_str_arr(body, "exited")
                && req_str_arr(body, "entered")
                && req_str(body, "source_state")
                && microsteps_ok(body)
        }
        RecordKind::DeadlineRejected => {
            req_str(body, "instance_id")
                && req_str(body, "request_id")
                && deadline_identity_ok(body)
                && is_state_hash(body.get("state_hash"))
                && rejection_ok(body)
        }
        RecordKind::DeadlineNotDue => {
            let next_ok = match (
                body.get("next_deadline"),
                body.get("next_deadline_idx"),
                body.get("next_due_ms"),
            ) {
                (None, None, None) => true,
                (Some(Value::Str(_)), Some(idx), Some(due)) => {
                    idx.as_num().is_some_and(|raw| raw.parse::<u32>().is_ok())
                        && due.as_num().is_some_and(|raw| raw.parse::<i64>().is_ok())
                }
                _ => false,
            };
            req_str(body, "instance_id")
                && req_str(body, "request_id")
                && is_state_hash(body.get("state_hash"))
                && next_ok
        }
        RecordKind::EffectAcked => {
            req_str(body, "instance_id")
                && req_str(body, "effect_id")
                && req_str(body, "request_id")
                && matches!(
                    body.get("outcome").and_then(Value::as_str),
                    Some("ok") | Some("failed")
                )
                && is_state_hash(body.get("state_hash"))
        }
        RecordKind::RequestRejected => {
            req_str(body, "request_id")
                && req_str(body, "instance_id")
                && rejection_ok(body)
                && req_str(body, "operation")
                && is_state_hash(body.get("state_hash"))
                && (body.get("operation").and_then(Value::as_str) != Some("ack")
                    || req_str(body, "effect_id"))
        }
        RecordKind::InstanceCancelled => {
            req_str(body, "instance_id")
                && req_str(body, "request_id")
                && req_str(body, "reason")
                && is_state_hash(body.get("state_hash"))
        }
        RecordKind::InstanceInvoked => {
            req_str(body, "parent_instance_id")
                && req_str(body, "slot")
                && req_str(body, "child_instance_id")
                && req_str(body, "child_machine_id")
                && req_str(body, "request_id")
                && is_state_hash(body.get("state_hash"))
                && is_state_hash(body.get("child_state_hash"))
                && body.get("overrides").and_then(Value::as_obj).is_some()
        }
        RecordKind::Annotated => {
            req_str(body, "instance_id") && req_str(body, "request_id") && req_str(body, "note")
        }
        RecordKind::StateCheckpoint => is_state_hash(body.get("state_root")),
    };

    let current_only = matches!(
        kind,
        RecordKind::DeadlineApplied | RecordKind::DeadlineRejected | RecordKind::DeadlineNotDue
    );
    let current_root_ok = body.get("state_root").is_none()
        || body.get("state_format").is_none()
        || body.get("state_root_format").and_then(Value::as_str) == Some("fsm.state-root/3");
    shape_ok && state_format_ok(body, current_only) && root_format_ok(body) && current_root_ok
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seal_canonical_bytes() {
        let rec = seal(
            1,
            10,
            RecordKind::Annotated,
            Value::Obj(BTreeMap::from([
                ("instance_id".into(), Value::Str("i".into())),
                ("request_id".into(), Value::Str("r".into())),
                ("note".into(), Value::Str("n".into())),
            ])),
            &zeros(),
        );
        let line = rec.to_line();
        let s = std::str::from_utf8(&line).unwrap();
        assert!(s.starts_with('{'));
        assert!(s.contains("\"hash\":"));
        assert!(s.contains("\"kind\":\"annotated\""));
        assert!(s.ends_with('\n'));
        assert!(!s[..s.len() - 1].contains('\n'));
    }

    #[test]
    fn genesis_shape() {
        let mut body = BTreeMap::new();
        body.insert("format".into(), Value::Str("fsm.journal/1".into()));
        body.insert("created_ts".into(), Value::Num("0".into()));
        body.insert("limits".into(), limits_value());
        let rec = seal(0, 0, RecordKind::Genesis, Value::Obj(body), &zeros());
        assert_eq!(rec.seq, 0);
        assert_eq!(rec.prev, zeros());
        assert_eq!(
            rec.body.get("format").and_then(Value::as_str),
            Some("fsm.journal/1")
        );
        assert!(
            rec.body
                .get("limits")
                .and_then(Value::as_obj)
                .unwrap()
                .contains_key("max_states")
        );
    }

    #[test]
    fn verify_errors() {
        assert!(matches!(
            verify_line(b"{", 0, &zeros()),
            Err(RecordError::Parse { .. })
        ));
        let rec = seal(
            0,
            0,
            RecordKind::Genesis,
            {
                let mut b = BTreeMap::new();
                b.insert("format".into(), Value::Str("fsm.journal/1".into()));
                b.insert("created_ts".into(), Value::Num("0".into()));
                b.insert("limits".into(), limits_value());
                Value::Obj(b)
            },
            &zeros(),
        );
        let mut line = rec.to_line();
        assert!(verify_line(&line, 0, &zeros()).is_ok());
        assert!(matches!(
            verify_line(&line, 1, &zeros()),
            Err(RecordError::SeqGap { .. })
        ));
        assert!(matches!(
            verify_line(&line, 0, "11"),
            Err(RecordError::PrevMismatch { .. })
        ));
        // non-canonical space
        let mut spaced = line.clone();
        spaced.insert(1, b' ');
        assert!(matches!(
            verify_line(&spaced, 0, &zeros()),
            Err(RecordError::NonCanonical { .. })
        ));
    }

    fn hex_hash() -> String {
        format!("sha256:{}", "ab".repeat(32))
    }

    fn verify_kind(kind: RecordKind, body: BTreeMap<String, Value>) -> Result<Record, RecordError> {
        let rec = seal(1, 1, kind, Value::Obj(body), &zeros());
        verify_line(&rec.to_line(), 1, &zeros())
    }

    #[test]
    fn body_schema_requires_typed_fields() {
        let mut applied = BTreeMap::new();
        applied.insert("instance_id".into(), Value::Str("i".into()));
        applied.insert("event".into(), Value::Str("go".into()));
        applied.insert("payload".into(), Value::Obj(BTreeMap::new()));
        applied.insert("request_id".into(), Value::Str("r".into()));
        applied.insert("state_hash".into(), Value::Str(hex_hash()));
        applied.insert("exited".into(), Value::Arr(vec![]));
        applied.insert("entered".into(), Value::Arr(vec![]));
        applied.insert("source_state".into(), Value::Str("s".into()));
        assert!(verify_kind(RecordKind::EventApplied, applied.clone()).is_ok());

        let mut missing = applied.clone();
        missing.remove("exited");
        assert!(matches!(
            verify_kind(RecordKind::EventApplied, missing),
            Err(RecordError::BodyInvalid { .. })
        ));

        let mut bad_hash = applied;
        bad_hash.insert(
            "state_hash".into(),
            Value::Str(
                "sha256:zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz".into(),
            ),
        );
        assert!(matches!(
            verify_kind(RecordKind::EventApplied, bad_hash),
            Err(RecordError::BodyInvalid { .. })
        ));

        let mut ack = BTreeMap::new();
        ack.insert("instance_id".into(), Value::Str("i".into()));
        ack.insert("effect_id".into(), Value::Num("7".into()));
        ack.insert("request_id".into(), Value::Str("r".into()));
        assert!(matches!(
            verify_kind(RecordKind::EffectAcked, ack),
            Err(RecordError::BodyInvalid { .. })
        ));

        let mut rejected = BTreeMap::new();
        rejected.insert("instance_id".into(), Value::Str("i".into()));
        rejected.insert("request_id".into(), Value::Str("r".into()));
        rejected.insert("event".into(), Value::Str("go".into()));
        rejected.insert("payload".into(), Value::Obj(BTreeMap::new()));
        rejected.insert("state_hash".into(), Value::Str(hex_hash()));
        assert!(matches!(
            verify_kind(RecordKind::EventRejected, rejected),
            Err(RecordError::BodyInvalid { .. })
        ));

        let mut ann = BTreeMap::new();
        ann.insert("instance_id".into(), Value::Num("1".into()));
        ann.insert("request_id".into(), Value::Str("r".into()));
        ann.insert("note".into(), Value::Str("n".into()));
        assert!(matches!(
            verify_kind(RecordKind::Annotated, ann),
            Err(RecordError::BodyInvalid { .. })
        ));
    }

    #[test]
    fn deadline_body_schemas_require_current_formats_and_complete_identity() {
        let mut applied = BTreeMap::from([
            ("instance_id".into(), Value::Str("i".into())),
            ("request_id".into(), Value::Str("r".into())),
            ("deadline".into(), Value::Str("expire".into())),
            ("deadline_idx".into(), Value::Num("0".into())),
            ("due_ms".into(), Value::Num("42".into())),
            ("state_hash".into(), Value::Str(hex_hash())),
            (
                "state_format".into(),
                Value::Str(crate::hashes::STATE_FORMAT.into()),
            ),
            ("exited".into(), Value::Arr(vec![Value::Str("wait".into())])),
            (
                "entered".into(),
                Value::Arr(vec![Value::Str("done".into())]),
            ),
            ("source_state".into(), Value::Str("wait".into())),
        ]);
        assert!(verify_kind(RecordKind::DeadlineApplied, applied.clone()).is_ok());
        applied.remove("state_format");
        assert!(matches!(
            verify_kind(RecordKind::DeadlineApplied, applied),
            Err(RecordError::BodyInvalid { .. })
        ));

        let mut idle = BTreeMap::from([
            ("instance_id".into(), Value::Str("i".into())),
            ("request_id".into(), Value::Str("r".into())),
            ("state_hash".into(), Value::Str(hex_hash())),
            (
                "state_format".into(),
                Value::Str(crate::hashes::STATE_FORMAT.into()),
            ),
            ("next_deadline".into(), Value::Str("expire".into())),
            ("next_deadline_idx".into(), Value::Num("0".into())),
            ("next_due_ms".into(), Value::Num("42".into())),
        ]);
        assert!(verify_kind(RecordKind::DeadlineNotDue, idle.clone()).is_ok());
        idle.remove("next_due_ms");
        assert!(matches!(
            verify_kind(RecordKind::DeadlineNotDue, idle),
            Err(RecordError::BodyInvalid { .. })
        ));
    }
}
