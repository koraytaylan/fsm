//! Native acknowledgement is an atomic settlement, without a legacy ack append.

use super::*;
use fsm_core::record::RecordKind;

pub(super) fn assert_ack_only(store: &Store, instance_id: &str, effect_id: &str) {
    // SPEC Native execution settlement: acked applies acknowledgement exactly
    // once inside ExecutionSettled, with the unchanged derived request key.
    let acknowledgements: Vec<_> = store
        .records
        .iter()
        .filter(|record| {
            record.kind == RecordKind::ExecutionSettled
                && record.body.get("disposition").and_then(Value::as_str) == Some("acked")
        })
        .collect();
    assert_eq!(acknowledgements.len(), 1);
    let acknowledgement = acknowledgements[0];
    assert_eq!(
        acknowledgement
            .body
            .get("instance_id")
            .and_then(Value::as_str),
        Some(instance_id)
    );
    assert_eq!(
        acknowledgement
            .body
            .get("effect_id")
            .and_then(Value::as_str),
        Some(effect_id)
    );
    let request_id = fsm_execute::rid::ack_rid(effect_id);
    assert_eq!(
        acknowledgement
            .body
            .get("request_id")
            .and_then(Value::as_str),
        Some(request_id.as_str())
    );
    let slot = store.state.dedup.get(&request_id).unwrap();
    assert_eq!(slot.seq, acknowledgement.seq);
    assert!(slot.fp.is_some());
    assert_eq!(
        acknowledgement.body.get("outcome").and_then(Value::as_str),
        Some("ok")
    );
    assert!(store.records.iter().all(|record| !matches!(
        record.kind,
        RecordKind::EffectAcked | RecordKind::EffectAttempted | RecordKind::EventApplied
    )));
    assert!(
        !store
            .state
            .dedup
            .contains_key(&fsm_execute::rid::event_rid(effect_id, "done"))
    );
    assert_eq!(store.state.execution_handoffs.outstanding().count(), 0);
}
