use crate::json::Value;
use crate::record::Record;
use crate::step::PendingDeadline;

use super::ReplayError;

pub(super) fn record_deadline(record: &Record) -> Result<PendingDeadline, ReplayError> {
    let name =
        record
            .body
            .get("deadline")
            .and_then(Value::as_str)
            .ok_or(ReplayError::FieldMismatch {
                seq: record.seq,
                field: "deadline",
            })?;
    let deadline_idx = record
        .body
        .get("deadline_idx")
        .and_then(Value::as_num)
        .and_then(|raw| raw.parse().ok())
        .ok_or(ReplayError::FieldMismatch {
            seq: record.seq,
            field: "deadline_idx",
        })?;
    let due_ms = record
        .body
        .get("due_ms")
        .and_then(Value::as_num)
        .and_then(|raw| raw.parse().ok())
        .ok_or(ReplayError::FieldMismatch {
            seq: record.seq,
            field: "due_ms",
        })?;
    Ok(PendingDeadline {
        name: name.into(),
        deadline_idx,
        due_ms,
    })
}

pub(super) fn record_next_deadline(
    record: &Record,
) -> Result<Option<PendingDeadline>, ReplayError> {
    let Some(name) = record.body.get("next_deadline") else {
        if record.body.get("next_deadline_idx").is_some()
            || record.body.get("next_due_ms").is_some()
        {
            return Err(ReplayError::FieldMismatch {
                seq: record.seq,
                field: "next_deadline",
            });
        }
        return Ok(None);
    };
    let name = name.as_str().ok_or(ReplayError::FieldMismatch {
        seq: record.seq,
        field: "next_deadline",
    })?;
    let deadline_idx = record
        .body
        .get("next_deadline_idx")
        .and_then(Value::as_num)
        .and_then(|raw| raw.parse().ok())
        .ok_or(ReplayError::FieldMismatch {
            seq: record.seq,
            field: "next_deadline_idx",
        })?;
    let due_ms = record
        .body
        .get("next_due_ms")
        .and_then(Value::as_num)
        .and_then(|raw| raw.parse().ok())
        .ok_or(ReplayError::FieldMismatch {
            seq: record.seq,
            field: "next_due_ms",
        })?;
    Ok(Some(PendingDeadline {
        name: name.into(),
        deadline_idx,
        due_ms,
    }))
}

pub(super) fn verify_deadline(
    record: &Record,
    expected: &PendingDeadline,
    actual: &PendingDeadline,
    next: bool,
) -> Result<(), ReplayError> {
    if expected.name != actual.name {
        return Err(ReplayError::FieldMismatch {
            seq: record.seq,
            field: if next { "next_deadline" } else { "deadline" },
        });
    }
    if expected.deadline_idx != actual.deadline_idx {
        return Err(ReplayError::FieldMismatch {
            seq: record.seq,
            field: if next {
                "next_deadline_idx"
            } else {
                "deadline_idx"
            },
        });
    }
    if expected.due_ms != actual.due_ms {
        return Err(ReplayError::FieldMismatch {
            seq: record.seq,
            field: if next { "next_due_ms" } else { "due_ms" },
        });
    }
    Ok(())
}

pub(super) fn verify_deadline_transition(
    record: &Record,
    applied: &crate::step::Applied,
) -> Result<(), ReplayError> {
    let exited =
        record
            .body
            .get("exited")
            .and_then(Value::as_arr)
            .ok_or(ReplayError::FieldMismatch {
                seq: record.seq,
                field: "exited",
            })?;
    let actual_exited: Vec<_> = applied.exited.iter().cloned().map(Value::Str).collect();
    if exited != actual_exited {
        return Err(ReplayError::FieldMismatch {
            seq: record.seq,
            field: "exited",
        });
    }

    let entered =
        record
            .body
            .get("entered")
            .and_then(Value::as_arr)
            .ok_or(ReplayError::FieldMismatch {
                seq: record.seq,
                field: "entered",
            })?;
    let actual_entered: Vec<_> = applied.entered.iter().cloned().map(Value::Str).collect();
    if entered != actual_entered {
        return Err(ReplayError::FieldMismatch {
            seq: record.seq,
            field: "entered",
        });
    }

    if record.body.get("source_state").and_then(Value::as_str)
        != Some(applied.source_state.as_str())
    {
        return Err(ReplayError::FieldMismatch {
            seq: record.seq,
            field: "source_state",
        });
    }
    Ok(())
}
