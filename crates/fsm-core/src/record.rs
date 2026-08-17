//! Journal record envelope: seal, verify, ten kinds.

#![allow(clippy::should_implement_trait, clippy::byte_char_slices, unused_mut)]

use std::collections::BTreeMap;

use crate::hashes::domain_hash;
use crate::json::{JsonLimits, Value, parse};
use crate::sha256::to_hex;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RecordKind {
    Genesis,
    MachineDefined,
    InstanceCreated,
    EventApplied,
    EventRejected,
    EventIgnored,
    EffectAcked,
    RequestRejected,
    InstanceCancelled,
    Annotated,
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
            RecordKind::EffectAcked => "effect_acked",
            RecordKind::RequestRejected => "request_rejected",
            RecordKind::InstanceCancelled => "instance_cancelled",
            RecordKind::Annotated => "annotated",
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
            "effect_acked" => Self::EffectAcked,
            "request_rejected" => Self::RequestRejected,
            "instance_cancelled" => Self::InstanceCancelled,
            "annotated" => Self::Annotated,
            _ => return None,
        })
    }

    pub fn all() -> [RecordKind; 10] {
        [
            Self::Genesis,
            Self::MachineDefined,
            Self::InstanceCreated,
            Self::EventApplied,
            Self::EventRejected,
            Self::EventIgnored,
            Self::EffectAcked,
            Self::RequestRejected,
            Self::InstanceCancelled,
            Self::Annotated,
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

fn req_arr(body: &Value, k: &str) -> bool {
    body.get(k).and_then(Value::as_arr).is_some()
}

fn body_ok(kind: RecordKind, body: &Value) -> bool {
    match kind {
        RecordKind::Genesis => {
            body.get("format").and_then(Value::as_str) == Some("fsm.journal/1")
                && body.get("limits").is_some()
        }
        RecordKind::MachineDefined => req_str(body, "machine_id") && body.get("def").is_some(),
        RecordKind::InstanceCreated => {
            req_str(body, "instance_id")
                && req_str(body, "machine_id")
                && req_str(body, "request_id")
                && req_str(body, "leaf")
                && is_state_hash(body.get("state_hash"))
        }
        RecordKind::EventApplied => {
            req_str(body, "instance_id")
                && req_str(body, "event")
                && body.get("payload").is_some()
                && req_str(body, "request_id")
                && is_state_hash(body.get("state_hash"))
                && req_arr(body, "exited")
                && req_arr(body, "entered")
                && req_str(body, "source_state")
        }
        RecordKind::EventRejected => {
            req_str(body, "instance_id")
                && req_str(body, "request_id")
                && req_str(body, "event")
                && body.get("payload").is_some()
                && is_state_hash(body.get("state_hash"))
                && req_str(body, "code")
        }
        RecordKind::EventIgnored => {
            req_str(body, "instance_id")
                && req_str(body, "request_id")
                && req_str(body, "event")
                && body.get("payload").is_some()
                && is_state_hash(body.get("state_hash"))
        }
        RecordKind::EffectAcked => {
            req_str(body, "instance_id")
                && req_str(body, "effect_id")
                && req_str(body, "request_id")
        }
        RecordKind::RequestRejected => {
            req_str(body, "request_id") && req_str(body, "instance_id") && req_str(body, "code")
        }
        RecordKind::InstanceCancelled => {
            req_str(body, "instance_id") && req_str(body, "request_id")
        }
        RecordKind::Annotated => {
            req_str(body, "instance_id") && req_str(body, "request_id") && req_str(body, "note")
        }
    }
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
}
