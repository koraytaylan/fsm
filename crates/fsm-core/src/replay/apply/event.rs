//! Replaying the event records: applied, rejected, and ignored.
//!
//! Split out of `apply.rs` when composition's applier pushed the file
//! past the workspace's 1000-line ceiling; the seams were already here.

use std::collections::BTreeMap;

use crate::expr::eval::Budget;
use crate::hashes::configuration_value;
use crate::json::Value;
use crate::machine::{InstanceState, Status};
use crate::record::{Record, RecordKind};
use crate::step::{DeadlineOutcome, Outcome, create, poll_deadline, step};
use crate::tree::Tree;

use super::super::ctx::{claim_request_id, overrides_from};
use super::super::deadline::{
    record_deadline, record_next_deadline, verify_deadline, verify_deadline_transition,
};
use super::super::report::enabled_reports_value;
use super::super::verify::{
    expected_deadline_rejected_details, expected_event_rejected_details,
    expected_request_rejected_details, verify_microsteps, verify_record_state_hash,
    verify_rejection,
};
use super::super::{
    DefinitionCompileMode, ReplayError, STATE_ROOT_FORMAT, StoreState, StoredMachine,
    legacy_state_root_at, state_hash_for_record, state_root_at,
};

pub(super) fn apply_event_applied(st: &mut StoreState, rec: &Record) -> Result<(), ReplayError> {
    let iid = rec
        .body
        .get("instance_id")
        .and_then(Value::as_str)
        .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
    let ev = rec.body.get("event").and_then(Value::as_str).unwrap_or("");
    let payload = rec
        .body
        .get("payload")
        .cloned()
        .unwrap_or(Value::Obj(BTreeMap::new()));
    let mid = st
        .instance_machines
        .get(iid)
        .cloned()
        .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
    let m = st
        .machines
        .get(&mid)
        .ok_or(ReplayError::UnknownMachine { seq: rec.seq })?;
    let inst = st
        .instances
        .get(iid)
        .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?
        .clone();
    // A live write ran under the macrostep budget; replaying under the
    // standard one would fail a legitimately deep macrostep and surface as a
    // mismatch on a healthy store, the worst diagnosis this system can give.
    let mut bud = Budget::new(crate::limits::MACROSTEP_EVAL_TICKS);
    match step(&m.compiled, &m.tree, &inst, ev, &payload, rec.ts, &mut bud) {
        Outcome::Applied(a) => {
            let want = rec.body.get("exited").and_then(Value::as_arr).ok_or(
                ReplayError::FieldMismatch {
                    seq: rec.seq,
                    field: "exited",
                },
            )?;
            let got: Vec<_> = a.exited.iter().map(|s| Value::Str(s.clone())).collect();
            if got != *want {
                return Err(ReplayError::FieldMismatch {
                    seq: rec.seq,
                    field: "exited",
                });
            }
            let want = rec.body.get("entered").and_then(Value::as_arr).ok_or(
                ReplayError::FieldMismatch {
                    seq: rec.seq,
                    field: "entered",
                },
            )?;
            let got: Vec<_> = a.entered.iter().map(|s| Value::Str(s.clone())).collect();
            if got != *want {
                return Err(ReplayError::FieldMismatch {
                    seq: rec.seq,
                    field: "entered",
                });
            }
            let want = rec.body.get("source_state").and_then(Value::as_str).ok_or(
                ReplayError::FieldMismatch {
                    seq: rec.seq,
                    field: "source_state",
                },
            )?;
            if want != a.source_state {
                return Err(ReplayError::FieldMismatch {
                    seq: rec.seq,
                    field: "source_state",
                });
            }
            verify_microsteps(rec, &a.trace.microsteps)?;
            let mut pending = inst.pending.clone();
            pending.extend(
                a.effects
                    .iter()
                    .map(|e| format!("{iid}/{}/{}", rec.seq, e.k)),
            );
            let new = InstanceState {
                status: a.status_after,
                configuration: a.configuration_after,
                ctx: a.ctx_after,
                history: a.history_after,
                deadlines: a.deadlines_after,
                pending,
                invocations: a.invocations_after,
                signals: BTreeMap::new(),
            };
            let want = rec.body.get("state_hash").and_then(Value::as_str).ok_or(
                ReplayError::FieldMismatch {
                    seq: rec.seq,
                    field: "state_hash",
                },
            )?;
            let got =
                state_hash_for_record(rec, &mid, iid, &new).ok_or(ReplayError::FieldMismatch {
                    seq: rec.seq,
                    field: "state_format",
                })?;
            if got != want {
                return Err(ReplayError::StateHashMismatch {
                    seq: rec.seq,
                    expected: want.into(),
                    found: got,
                });
            }
            st.instances.insert(iid.into(), new);
        }
        _ => {
            return Err(ReplayError::FieldMismatch {
                seq: rec.seq,
                field: "outcome",
            });
        }
    }
    claim_request_id(st, rec)?;
    Ok(())
}

pub(super) fn apply_event_rejected_or_ignored(
    st: &mut StoreState,
    rec: &Record,
    compile_mode: DefinitionCompileMode,
) -> Result<(), ReplayError> {
    let iid = rec
        .body
        .get("instance_id")
        .and_then(Value::as_str)
        .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
    let ev = rec.body.get("event").and_then(Value::as_str).unwrap_or("");
    let payload = rec
        .body
        .get("payload")
        .cloned()
        .unwrap_or(Value::Obj(BTreeMap::new()));
    let mid = st
        .instance_machines
        .get(iid)
        .cloned()
        .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
    let m = st
        .machines
        .get(&mid)
        .ok_or(ReplayError::UnknownMachine { seq: rec.seq })?;
    let inst = st
        .instances
        .get(iid)
        .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
    let want =
        rec.body
            .get("state_hash")
            .and_then(Value::as_str)
            .ok_or(ReplayError::FieldMismatch {
                seq: rec.seq,
                field: "state_hash",
            })?;
    let got = state_hash_for_record(rec, &mid, iid, inst).ok_or(ReplayError::FieldMismatch {
        seq: rec.seq,
        field: "state_format",
    })?;
    if got != want {
        return Err(ReplayError::StateHashMismatch {
            seq: rec.seq,
            expected: want.into(),
            found: got,
        });
    }
    let out = super::super::replay_sealed_step(
        &m.compiled,
        &m.tree,
        inst,
        ev,
        &payload,
        rec.ts,
        &rec.body,
    );
    match (rec.kind, &out) {
        (RecordKind::EventRejected, Outcome::Rejected(r)) => {
            let code =
                rec.body
                    .get("code")
                    .and_then(Value::as_str)
                    .ok_or(ReplayError::FieldMismatch {
                        seq: rec.seq,
                        field: "code",
                    })?;
            if code != r.code {
                return Err(ReplayError::FieldMismatch {
                    seq: rec.seq,
                    field: "code",
                });
            }
            let msg = rec.body.get("message").and_then(Value::as_str).ok_or(
                ReplayError::FieldMismatch {
                    seq: rec.seq,
                    field: "message",
                },
            )?;
            if msg != r.message {
                return Err(ReplayError::FieldMismatch {
                    seq: rec.seq,
                    field: "message",
                });
            }
            let hint =
                rec.body
                    .get("hint")
                    .and_then(Value::as_str)
                    .ok_or(ReplayError::FieldMismatch {
                        seq: rec.seq,
                        field: "hint",
                    })?;
            if hint != r.hint {
                return Err(ReplayError::FieldMismatch {
                    seq: rec.seq,
                    field: "hint",
                });
            }
            let details = rec.body.get("details").and_then(Value::as_obj).ok_or(
                ReplayError::FieldMismatch {
                    seq: rec.seq,
                    field: "details",
                },
            )?;
            let mut bud2 = Budget::new(crate::limits::MAX_EVAL_TICKS);
            let evs = crate::analyze::enabled_events(&m.compiled, &m.tree, inst, &mut bud2);
            let rid = rec.body.get("request_id").and_then(Value::as_str);
            let want = expected_event_rejected_details(r, rid, enabled_reports_value(&evs));
            let historical_match = if details != &want
                && compile_mode == DefinitionCompileMode::HistoricalPersistence
            {
                let mut historical_budget = Budget::new(crate::limits::MAX_EVAL_TICKS);
                let historical_events = crate::analyze::enabled_events_historical(
                    &m.compiled,
                    &m.tree,
                    inst,
                    &mut historical_budget,
                );
                let historical = expected_event_rejected_details(
                    r,
                    rid,
                    enabled_reports_value(&historical_events),
                );
                details == &historical
            } else {
                false
            };
            if details != &want && !historical_match {
                return Err(ReplayError::FieldMismatch {
                    seq: rec.seq,
                    field: "details",
                });
            }
            match (rec.body.get("span"), r.span) {
                (None, None) => {}
                (Some(Value::Obj(o)), Some((s, e))) => {
                    if o.get("start").and_then(Value::as_num) != Some(&s.to_string())
                        || o.get("end").and_then(Value::as_num) != Some(&e.to_string())
                    {
                        return Err(ReplayError::FieldMismatch {
                            seq: rec.seq,
                            field: "span",
                        });
                    }
                }
                _ => {
                    return Err(ReplayError::FieldMismatch {
                        seq: rec.seq,
                        field: "span",
                    });
                }
            }
        }
        (RecordKind::EventIgnored, Outcome::Ignored) => {}
        _ => {
            return Err(ReplayError::FieldMismatch {
                seq: rec.seq,
                field: "outcome",
            });
        }
    }
    claim_request_id(st, rec)?;
    Ok(())
}
