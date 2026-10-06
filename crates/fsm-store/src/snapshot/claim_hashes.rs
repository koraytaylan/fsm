//! Rebuild replay-only claim anchors from verified records and base context.

use fsm_core::record::{Record, RecordKind};
use fsm_core::replay::{ReplayError, StoreState};

pub(super) fn restore(
    state: &mut StoreState,
    origin: &StoreState,
    records: &[Record],
) -> Result<(), ReplayError> {
    let claims: Vec<_> = state
        .execution
        .unresolved()
        .map(|(claim, _)| claim.clone())
        .collect();
    for claim in claims {
        let material = claim.to_value();
        let live = records.iter().find(|record| {
            record.seq <= state.last_seq
                && record.kind == RecordKind::ExecutionClaimed
                && material.as_obj().is_some_and(|fields| {
                    fields
                        .iter()
                        .all(|(key, value)| record.body.get(key) == Some(value))
                })
        });
        let hash = live
            .map(|record| format!("sha256:{}", record.hash))
            .or_else(|| {
                origin
                    .execution
                    .claim_record_hash(&claim)
                    .map(str::to_owned)
            })
            .ok_or(ReplayError::FieldMismatch {
                seq: state.last_seq,
                field: "claim_hash",
            })?;
        state
            .execution
            .attach_claim_record_hash(&claim, &hash)
            .map_err(|_| ReplayError::FieldMismatch {
                seq: state.last_seq,
                field: "claim_hash",
            })?;
    }
    Ok(())
}
