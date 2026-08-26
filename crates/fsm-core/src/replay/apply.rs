use std::collections::BTreeMap;

use crate::expr::eval::Budget;
use crate::hashes::configuration_value;
use crate::json::Value;
use crate::machine::{InstanceState, Status};
use crate::record::{Record, RecordKind};
use crate::step::{DeadlineOutcome, Outcome, create, poll_deadline, step};
use crate::tree::Tree;

use super::ctx::{claim_request_id, overrides_from};
use super::deadline::{
    record_deadline, record_next_deadline, verify_deadline, verify_deadline_transition,
};
use super::report::enabled_reports_value;
use super::verify::{
    expected_deadline_rejected_details, expected_event_rejected_details,
    expected_request_rejected_details, verify_microsteps, verify_record_state_hash,
    verify_rejection,
};
use super::{
    DefinitionCompileMode, ReplayError, STATE_ROOT_FORMAT, StoreState, StoredMachine,
    legacy_state_root_at, state_hash_for_record, state_root_at,
};

pub(super) fn apply(
    st: &mut StoreState,
    rec: &Record,
    compile_mode: DefinitionCompileMode,
) -> Result<(), ReplayError> {
    let applied = match rec.kind {
        RecordKind::Genesis => Ok(()),
        RecordKind::MachineDefined => apply_machine_defined(st, rec, compile_mode),
        RecordKind::InstanceCreated => apply_instance_created(st, rec),
        RecordKind::EventApplied => apply_event_applied(st, rec),
        RecordKind::EventRejected | RecordKind::EventIgnored => {
            apply_event_rejected_or_ignored(st, rec, compile_mode)
        }
        RecordKind::DeadlineApplied => apply_deadline_applied(st, rec),
        RecordKind::DeadlineRejected => apply_deadline_rejected(st, rec),
        RecordKind::DeadlineNotDue => apply_deadline_not_due(st, rec),
        RecordKind::EffectAcked => apply_effect_acked(st, rec),
        RecordKind::RequestRejected => apply_request_rejected(st, rec),
        RecordKind::InstanceCancelled => apply_instance_cancelled(st, rec),
        RecordKind::Annotated => apply_annotated(st, rec),
        RecordKind::StateCheckpoint => Ok(()),
    };
    applied?;
    if let Some(root) = rec.body.get("state_root") {
        let want = root.as_str().ok_or(ReplayError::FieldMismatch {
            seq: rec.seq,
            field: "state_root",
        })?;
        let found = match rec.body.get("state_root_format").and_then(Value::as_str) {
            Some(STATE_ROOT_FORMAT) => state_root_at(st, rec.seq),
            None => legacy_state_root_at(st, rec.seq),
            Some(_) => {
                return Err(ReplayError::FieldMismatch {
                    seq: rec.seq,
                    field: "state_root_format",
                });
            }
        };
        if want != found {
            return Err(ReplayError::FieldMismatch {
                seq: rec.seq,
                field: "state_root",
            });
        }
    }
    Ok(())
}

fn apply_machine_defined(
    st: &mut StoreState,
    rec: &Record,
    compile_mode: DefinitionCompileMode,
) -> Result<(), ReplayError> {
    let def = rec
        .body
        .get("def")
        .cloned()
        .ok_or(ReplayError::UnknownMachine { seq: rec.seq })?;
    let compiled = match compile_mode {
        DefinitionCompileMode::Current => crate::spec::compile_accepted(&def),
        DefinitionCompileMode::HistoricalPersistence => {
            crate::spec::compile_accepted_historical_unchecked(&def)
        }
    }
    .map_err(|_| ReplayError::UnknownMachine { seq: rec.seq })?;
    let tree = Tree::for_machine(&compiled.spec);
    let id = rec
        .body
        .get("machine_id")
        .and_then(Value::as_str)
        .unwrap_or(&compiled.machine_id)
        .to_string();
    if id != compiled.machine_id {
        return Err(ReplayError::FieldMismatch {
            seq: rec.seq,
            field: "machine_id",
        });
    }
    st.machines.insert(
        id,
        StoredMachine {
            def,
            compiled,
            tree,
        },
    );
    Ok(())
}

fn apply_instance_created(st: &mut StoreState, rec: &Record) -> Result<(), ReplayError> {
    let mid = rec
        .body
        .get("machine_id")
        .and_then(Value::as_str)
        .ok_or(ReplayError::UnknownMachine { seq: rec.seq })?;
    let iid = rec
        .body
        .get("instance_id")
        .and_then(Value::as_str)
        .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
    let m = st
        .machines
        .get(mid)
        .ok_or(ReplayError::UnknownMachine { seq: rec.seq })?;
    let overrides = match overrides_from(&m.compiled.spec.context, rec.body.get("overrides")) {
        Some(o) => o,
        None => {
            return Err(ReplayError::FieldMismatch {
                seq: rec.seq,
                field: "overrides",
            });
        }
    };
    let a = create(&m.compiled, &m.tree, &overrides, rec.ts)
        .map_err(|_| ReplayError::UnknownInstance { seq: rec.seq })?;
    let inst = InstanceState {
        status: a.status_after,
        configuration: a.configuration_after,
        ctx: a.ctx_after,
        history: a.history_after,
        deadlines: a.deadlines_after,
        pending: a
            .effects
            .iter()
            .map(|e| format!("{iid}/0/{}", e.k))
            .collect(),
    };
    if let Some(want) = rec.body.get("state_hash").and_then(Value::as_str) {
        let got =
            state_hash_for_record(rec, mid, iid, &inst).ok_or(ReplayError::FieldMismatch {
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
    }
    if let Some(want) = rec.body.get("leaf").and_then(Value::as_str) {
        if inst.configuration.leaf(None) != Some(want) {
            return Err(ReplayError::FieldMismatch {
                seq: rec.seq,
                field: "leaf",
            });
        }
    }
    if let Some(want) = rec.body.get("configuration") {
        if want != &configuration_value(&inst.configuration) {
            return Err(ReplayError::FieldMismatch {
                seq: rec.seq,
                field: "configuration",
            });
        }
    }
    verify_microsteps(rec, &a.trace.microsteps)?;
    st.instances.insert(iid.into(), inst);
    st.instance_machines.insert(iid.into(), mid.into());
    claim_request_id(st, rec)?;
    Ok(())
}

fn apply_event_applied(st: &mut StoreState, rec: &Record) -> Result<(), ReplayError> {
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

fn apply_event_rejected_or_ignored(
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
    let out =
        super::replay_sealed_step(&m.compiled, &m.tree, inst, ev, &payload, rec.ts, &rec.body);
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

fn apply_deadline_applied(st: &mut StoreState, rec: &Record) -> Result<(), ReplayError> {
    let iid = rec
        .body
        .get("instance_id")
        .and_then(Value::as_str)
        .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
    let mid = st
        .instance_machines
        .get(iid)
        .cloned()
        .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
    let machine = st
        .machines
        .get(&mid)
        .ok_or(ReplayError::UnknownMachine { seq: rec.seq })?;
    let instance = st
        .instances
        .get(iid)
        .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?
        .clone();
    let expected_deadline = record_deadline(rec)?;
    let mut budget = Budget::new(crate::limits::MACROSTEP_EVAL_TICKS);
    match poll_deadline(
        &machine.compiled,
        &machine.tree,
        &instance,
        rec.ts,
        &mut budget,
    ) {
        DeadlineOutcome::Applied(applied) => {
            verify_deadline(rec, &expected_deadline, &applied.deadline, false)?;
            verify_deadline_transition(rec, &applied.transition)?;
            verify_microsteps(rec, &applied.transition.trace.microsteps)?;
            let mut pending = instance.pending.clone();
            pending.extend(
                applied
                    .transition
                    .effects
                    .iter()
                    .map(|effect| format!("{iid}/{}/{}", rec.seq, effect.k)),
            );
            let new = InstanceState {
                status: applied.transition.status_after,
                configuration: applied.transition.configuration_after,
                ctx: applied.transition.ctx_after,
                history: applied.transition.history_after,
                deadlines: applied.transition.deadlines_after,
                pending,
            };
            verify_record_state_hash(rec, &mid, iid, &new)?;
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

fn apply_deadline_rejected(st: &mut StoreState, rec: &Record) -> Result<(), ReplayError> {
    let iid = rec
        .body
        .get("instance_id")
        .and_then(Value::as_str)
        .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
    let mid = st
        .instance_machines
        .get(iid)
        .cloned()
        .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
    let machine = st
        .machines
        .get(&mid)
        .ok_or(ReplayError::UnknownMachine { seq: rec.seq })?;
    let instance = st
        .instances
        .get(iid)
        .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
    verify_record_state_hash(rec, &mid, iid, instance)?;
    let expected_deadline = record_deadline(rec)?;
    let mut budget = Budget::new(crate::limits::MACROSTEP_EVAL_TICKS);
    match poll_deadline(
        &machine.compiled,
        &machine.tree,
        instance,
        rec.ts,
        &mut budget,
    ) {
        DeadlineOutcome::Rejected(rejected) => {
            let selected = rejected
                .deadline
                .as_ref()
                .ok_or(ReplayError::FieldMismatch {
                    seq: rec.seq,
                    field: "deadline",
                })?;
            verify_deadline(rec, &expected_deadline, selected, false)?;
            let request_id = rec.body.get("request_id").and_then(Value::as_str);
            let details = expected_deadline_rejected_details(&rejected.rejection, request_id);
            verify_rejection(rec, &rejected.rejection, &details)?;
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

fn apply_deadline_not_due(st: &mut StoreState, rec: &Record) -> Result<(), ReplayError> {
    let iid = rec
        .body
        .get("instance_id")
        .and_then(Value::as_str)
        .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
    let mid = st
        .instance_machines
        .get(iid)
        .cloned()
        .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
    let machine = st
        .machines
        .get(&mid)
        .ok_or(ReplayError::UnknownMachine { seq: rec.seq })?;
    let instance = st
        .instances
        .get(iid)
        .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
    verify_record_state_hash(rec, &mid, iid, instance)?;
    let expected_next = record_next_deadline(rec)?;
    let mut budget = Budget::new(crate::limits::MACROSTEP_EVAL_TICKS);
    match poll_deadline(
        &machine.compiled,
        &machine.tree,
        instance,
        rec.ts,
        &mut budget,
    ) {
        DeadlineOutcome::NotDue { next } => match (&expected_next, &next) {
            (None, None) => {}
            (Some(expected), Some(actual)) => {
                verify_deadline(rec, expected, actual, true)?;
            }
            _ => {
                return Err(ReplayError::FieldMismatch {
                    seq: rec.seq,
                    field: "next_deadline",
                });
            }
        },
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

fn apply_effect_acked(st: &mut StoreState, rec: &Record) -> Result<(), ReplayError> {
    let iid = rec
        .body
        .get("instance_id")
        .and_then(Value::as_str)
        .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
    let eid =
        rec.body
            .get("effect_id")
            .and_then(Value::as_str)
            .ok_or(ReplayError::FieldMismatch {
                seq: rec.seq,
                field: "effect_id",
            })?;
    let inst = st
        .instances
        .get_mut(iid)
        .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
    if !inst.pending.iter().any(|p| p == eid) {
        return Err(ReplayError::FieldMismatch {
            seq: rec.seq,
            field: "effect_id",
        });
    }
    inst.pending.retain(|p| p != eid);
    let mid = st
        .instance_machines
        .get(iid)
        .cloned()
        .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
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
    claim_request_id(st, rec)?;
    Ok(())
}

fn apply_request_rejected(st: &mut StoreState, rec: &Record) -> Result<(), ReplayError> {
    let iid = rec
        .body
        .get("instance_id")
        .and_then(Value::as_str)
        .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
    let inst = st
        .instances
        .get(iid)
        .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
    let mid = st
        .instance_machines
        .get(iid)
        .cloned()
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
    match rec.body.get("operation").and_then(Value::as_str) {
        Some("ack") => {
            let eid = rec.body.get("effect_id").and_then(Value::as_str).ok_or(
                ReplayError::FieldMismatch {
                    seq: rec.seq,
                    field: "effect_id",
                },
            )?;
            if inst.pending.iter().any(|p| p == eid) {
                return Err(ReplayError::FieldMismatch {
                    seq: rec.seq,
                    field: "effect_id",
                });
            }
            if rec.body.get("code").and_then(Value::as_str) != Some("req/field_unknown") {
                return Err(ReplayError::FieldMismatch {
                    seq: rec.seq,
                    field: "code",
                });
            }
            if rec.body.get("message").and_then(Value::as_str) != Some("unknown effect id") {
                return Err(ReplayError::FieldMismatch {
                    seq: rec.seq,
                    field: "message",
                });
            }
            if rec.body.get("hint").and_then(Value::as_str)
                != Some("use an id from effects_pending")
            {
                return Err(ReplayError::FieldMismatch {
                    seq: rec.seq,
                    field: "hint",
                });
            }
            if rec.body.get("span").is_some() {
                return Err(ReplayError::FieldMismatch {
                    seq: rec.seq,
                    field: "span",
                });
            }
            let details = rec.body.get("details").and_then(Value::as_obj).ok_or(
                ReplayError::FieldMismatch {
                    seq: rec.seq,
                    field: "details",
                },
            )?;
            let rid = rec.body.get("request_id").and_then(Value::as_str);
            let want = expected_request_rejected_details(rid, &inst.pending);
            if details != &want {
                return Err(ReplayError::FieldMismatch {
                    seq: rec.seq,
                    field: "details",
                });
            }
        }
        Some("poll_deadline") => {
            let machine = st
                .machines
                .get(&mid)
                .ok_or(ReplayError::UnknownMachine { seq: rec.seq })?;
            let mut budget = Budget::new(crate::limits::MACROSTEP_EVAL_TICKS);
            match poll_deadline(&machine.compiled, &machine.tree, inst, rec.ts, &mut budget) {
                DeadlineOutcome::Rejected(rejected) if rejected.deadline.is_none() => {
                    let request_id = rec.body.get("request_id").and_then(Value::as_str);
                    let details =
                        expected_deadline_rejected_details(&rejected.rejection, request_id);
                    verify_rejection(rec, &rejected.rejection, &details)?;
                }
                _ => {
                    return Err(ReplayError::FieldMismatch {
                        seq: rec.seq,
                        field: "outcome",
                    });
                }
            }
        }
        _ => {
            return Err(ReplayError::FieldMismatch {
                seq: rec.seq,
                field: "operation",
            });
        }
    }
    claim_request_id(st, rec)?;
    Ok(())
}

fn apply_instance_cancelled(st: &mut StoreState, rec: &Record) -> Result<(), ReplayError> {
    let iid = rec
        .body
        .get("instance_id")
        .and_then(Value::as_str)
        .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
    let inst = st
        .instances
        .get_mut(iid)
        .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
    inst.status = Status::Cancelled;
    inst.deadlines.clear();
    let mid = st
        .instance_machines
        .get(iid)
        .cloned()
        .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
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
    claim_request_id(st, rec)?;
    Ok(())
}

fn apply_annotated(st: &mut StoreState, rec: &Record) -> Result<(), ReplayError> {
    let iid =
        rec.body
            .get("instance_id")
            .and_then(Value::as_str)
            .ok_or(ReplayError::FieldMismatch {
                seq: rec.seq,
                field: "instance_id",
            })?;
    if !st.instances.contains_key(iid) {
        return Err(ReplayError::UnknownInstance { seq: rec.seq });
    }
    if rec.body.get("note").and_then(Value::as_str).is_none() {
        return Err(ReplayError::FieldMismatch {
            seq: rec.seq,
            field: "note",
        });
    }
    claim_request_id(st, rec)?;
    Ok(())
}
