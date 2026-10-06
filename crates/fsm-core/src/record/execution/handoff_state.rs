//! Bounded atomic acknowledgement-to-event obligations, independent of owners.

use std::collections::BTreeMap;

use crate::hashes::request_fp;
use crate::json::Value;
use crate::record::{Record, RecordKind};

use super::bounded::{MAX_ENTRIES, block_size};
use super::{AcknowledgedHandoff, ShapeError};

/// Original acknowledged event obligations; decoding alone proves no publication.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HandoffState {
    entries: BTreeMap<u64, AcknowledgedHandoff>,
}

impl HandoffState {
    /// Borrow outstanding obligations in original run order.
    pub fn outstanding(&self) -> impl Iterator<Item = &AcknowledgedHandoff> {
        self.entries.values()
    }

    /// Install only after the caller matches actual atomic acknowledgement inputs.
    pub fn install(&mut self, handoff: AcknowledgedHandoff) -> Result<(), ShapeError> {
        if self.entries.len() >= MAX_ENTRIES {
            return Err(ShapeError("entries"));
        }
        let run = handoff.claim().run_id();
        if self.entries.contains_key(&run)
            || self
                .entries
                .values()
                .any(|existing| existing.claim().effect() == handoff.claim().effect())
        {
            return Err(ShapeError("handoff_duplicate"));
        }
        let mut next = self.clone();
        next.entries.insert(run, handoff);
        block_size(&next.to_value())?;
        *self = next;
        Ok(())
    }

    /// Encode the complete bounded obligations in canonical original run order.
    pub fn to_value(&self) -> Value {
        Value::Arr(
            self.entries
                .values()
                .map(AcknowledgedHandoff::to_value)
                .collect(),
        )
    }

    /// Decode bounded sorted material; an authenticated root must still bind it.
    pub fn from_value(value: &Value) -> Result<Self, ShapeError> {
        block_size(value)?;
        let values = value.as_arr().ok_or(ShapeError("handoffs"))?;
        if values.len() > MAX_ENTRIES {
            return Err(ShapeError("entries"));
        }
        let mut state = Self::default();
        let mut previous = 0;
        for value in values {
            let handoff = AcknowledgedHandoff::from_value(value)?;
            let run = handoff.claim().run_id();
            if run <= previous {
                return Err(ShapeError("run_order"));
            }
            previous = run;
            // The input's aggregate size has already been charged before copying.
            if state
                .entries
                .values()
                .any(|existing| existing.claim().effect() == handoff.claim().effect())
            {
                return Err(ShapeError("handoff_duplicate"));
            }
            state.entries.insert(run, handoff);
        }
        Ok(state)
    }

    /// Retire only from an actual accepted event with all original request material.
    /// The caller MUST verify that the record is the actual accepted transition;
    /// this method does not authenticate arbitrary caller-supplied record bytes.
    pub fn accept_event_record(&mut self, record: &Record) {
        if record.kind != RecordKind::EventApplied {
            return;
        }
        self.entries
            .retain(|_, handoff| !matches_event(handoff, record));
    }
}

fn matches_event(handoff: &AcknowledgedHandoff, record: &Record) -> bool {
    let contract = handoff.handler_contract();
    let advance = contract.get(if handoff.outcome().status() == "ok" {
        "on_ok"
    } else {
        "on_failed"
    });
    let Some(advance) = advance else {
        return false;
    };
    let Some(event) = advance.get("event").and_then(Value::as_str) else {
        return false;
    };
    let Some(payload) = advance.get("payload") else {
        return false;
    };
    let fingerprint = request_fp(
        "send",
        &BTreeMap::from([
            (
                "instance_id".into(),
                Value::Str(handoff.claim().effect().0.into()),
            ),
            ("event".into(), Value::Str(event.into())),
            ("payload".into(), payload.clone()),
        ]),
    );
    let mut stamped = payload.clone();
    if let (Value::Obj(fields), Some(Value::Arr(stamps))) = (&mut stamped, advance.get("stamps")) {
        for stamp in stamps.iter().filter_map(Value::as_str) {
            fields
                .entry(stamp.into())
                .or_insert_with(|| Value::Str(record.ts.to_string()));
        }
    }
    record.seq > handoff.acknowledgement_seq()
        && record.body.get("instance_id").and_then(Value::as_str)
            == Some(handoff.claim().effect().0)
        && record.body.get("request_id").and_then(Value::as_str) == Some(handoff.event_request_id())
        && record.body.get("event").and_then(Value::as_str) == Some(event)
        && record.body.get("request_fp").and_then(Value::as_str) == Some(fingerprint.as_str())
        && record.body.get("payload") == Some(&stamped)
}
