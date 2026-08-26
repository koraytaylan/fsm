//! Moving one instance onto a corrected definition, as one record.
//!
//! A migration that cannot be re-verified from the journal is a hole in the
//! audit posture, not a feature — so the record carries both machine ids and
//! the report's claims, and replay recomputes every one of them.
//!
//! Plan 0011 task 5501.

use std::collections::BTreeMap;

use fsm_core::hashes::{STATE_FORMAT, configuration_value, state_hash};
use fsm_core::json::Value;
use fsm_core::migrate::apply::{migrate, rescheduled_value};
use fsm_core::record::{RecordKind, microsteps_value};

use crate::store::{ErrorObj, Store};

impl Store {
    pub fn migrate_instance(
        &mut self,
        instance_id: &str,
        to_machine: &str,
        request_id: &str,
    ) -> Result<Value, ErrorObj> {
        self.migrate_instance_on(
            &mut crate::clock::GlobalClock,
            instance_id,
            to_machine,
            request_id,
        )
    }

    pub fn migrate_instance_on(
        &mut self,
        clock: &mut dyn crate::clock::Clock,
        instance_id: &str,
        to_machine: &str,
        request_id: &str,
    ) -> Result<Value, ErrorObj> {
        self.ensure_writable()?;
        if let Some(replay) =
            self.claim_request(request_id, Self::fp_migrate(instance_id, to_machine))?
        {
            return replay;
        }
        let target = self.resolve_machine(to_machine)?.clone();
        let to_machine_id = target.compiled.machine_id.clone();
        let from_machine_id = self
            .state
            .instance_machines
            .get(instance_id)
            .cloned()
            .ok_or_else(|| {
                ErrorObj::new("req/instance_not_found", instance_id).request_id(request_id)
            })?;
        let from = self
            .state
            .machines
            .get(&from_machine_id)
            .cloned()
            .ok_or_else(|| {
                ErrorObj::new("req/machine_not_found", from_machine_id.clone())
                    .request_id(request_id)
            })?;
        let state = self.state.instances[instance_id].clone();

        // The record's own `ts` is the migration's `now_ms`, so the deadline
        // rescheduling it records is reproducible by replay without a clock —
        // the rule every deadline record already follows.
        let commit_ts = clock.now_ms();
        let mut budget = fsm_core::expr::eval::Budget::new(fsm_core::limits::MACROSTEP_EVAL_TICKS);
        let migrated = match migrate(
            &from.compiled,
            &target.compiled,
            &target.tree,
            &state,
            commit_ts,
            &mut budget,
        ) {
            Ok(migrated) => migrated,
            Err(rejection) => {
                // An attempted-and-refused migration is journaled like any
                // other refusal, so it is visible in the audit trail rather
                // than invisible.
                let error = ErrorObj::new(rejection.code, rejection.message.clone())
                    .hint(rejection.hint.clone())
                    .request_id(request_id);
                return Err(self.journal_migration_refusal(
                    instance_id,
                    request_id,
                    error,
                    commit_ts,
                ));
            }
        };

        let seq = self.journal.last_seq + 1;
        let mut body = BTreeMap::new();
        body.insert("instance_id".into(), Value::Str(instance_id.into()));
        body.insert(
            "from_machine_id".into(),
            Value::Str(from_machine_id.clone()),
        );
        body.insert("to_machine_id".into(), Value::Str(to_machine_id.clone()));
        body.insert(
            "configuration_before".into(),
            configuration_value(&state.configuration),
        );
        body.insert(
            "configuration_after".into(),
            configuration_value(&migrated.state.configuration),
        );
        body.insert(
            "dropped_history".into(),
            Value::Arr(
                migrated
                    .report
                    .dropped_history
                    .iter()
                    .cloned()
                    .map(Value::Str)
                    .collect(),
            ),
        );
        body.insert(
            "rescheduled_deadlines".into(),
            rescheduled_value(&migrated.report.rescheduled_deadlines),
        );
        body.insert("request_id".into(), Value::Str(request_id.into()));
        body.insert(
            "state_hash".into(),
            Value::Str(state_hash(
                &to_machine_id,
                instance_id,
                seq,
                &migrated.state,
            )),
        );
        body.insert("state_format".into(), Value::Str(STATE_FORMAT.into()));
        if let Some(microsteps) = microsteps_value(&migrated.report.microsteps) {
            body.insert("microsteps".into(), microsteps);
        }
        let record =
            self.append_at_with_root(RecordKind::InstanceMigrated, Value::Obj(body), commit_ts)?;
        self.state
            .instances
            .insert(instance_id.into(), migrated.state);
        self.state
            .instance_machines
            .insert(instance_id.into(), to_machine_id.clone());
        self.history
            .entry(instance_id.into())
            .or_default()
            .push(record.seq);
        self.note_record(&record);
        let response = migrated_response(
            instance_id,
            &from_machine_id,
            &to_machine_id,
            request_id,
            record.seq,
            false,
        );
        self.commit_dedup(request_id, response.clone(), record.seq);
        self.finish_commit();
        Ok(response)
    }

    /// Journal a refused migration, claiming the key, and hand the caller its
    /// error. An attempt somebody made and the store refused belongs in the
    /// audit trail as much as one that succeeded.
    fn journal_migration_refusal(
        &mut self,
        instance_id: &str,
        request_id: &str,
        error: ErrorObj,
        commit_ts: i64,
    ) -> ErrorObj {
        let mut body = BTreeMap::new();
        body.insert("instance_id".into(), Value::Str(instance_id.into()));
        body.insert("request_id".into(), Value::Str(request_id.into()));
        body.insert("code".into(), Value::Str(error.code.clone()));
        body.insert("message".into(), Value::Str(error.message.clone()));
        body.insert("hint".into(), Value::Str(error.hint.clone()));
        body.insert("details".into(), error.details.clone());
        body.insert("operation".into(), Value::Str("migrate".into()));
        if let Some(instance) = self.state.instances.get(instance_id) {
            let machine_id = self
                .state
                .instance_machines
                .get(instance_id)
                .cloned()
                .unwrap_or_default();
            body.insert(
                "state_hash".into(),
                Value::Str(state_hash(
                    &machine_id,
                    instance_id,
                    self.journal.last_seq + 1,
                    instance,
                )),
            );
            body.insert("state_format".into(), Value::Str(STATE_FORMAT.into()));
        }
        let Ok(record) =
            self.append_at_with_root(RecordKind::RequestRejected, Value::Obj(body), commit_ts)
        else {
            return error;
        };
        self.note_record(&record);
        self.history
            .entry(instance_id.into())
            .or_default()
            .push(record.seq);
        self.last_errors.insert(request_id.into(), error.clone());
        let claimed = self.claimed_slot(record.seq);
        self.state.dedup.insert(request_id.into(), claimed);
        self.finish_commit();
        error
    }
}

/// The response a migration gives, warm or replayed.
pub(crate) fn migrated_response(
    instance_id: &str,
    from_machine_id: &str,
    to_machine_id: &str,
    request_id: &str,
    seq: u64,
    duplicate: bool,
) -> Value {
    Value::Obj(BTreeMap::from([
        ("ok".into(), Value::Str("true".into())),
        ("migrated".into(), Value::Bool(true)),
        ("instance_id".into(), Value::Str(instance_id.into())),
        ("from_machine_id".into(), Value::Str(from_machine_id.into())),
        ("to_machine_id".into(), Value::Str(to_machine_id.into())),
        ("request_id".into(), Value::Str(request_id.into())),
        ("seq".into(), Value::Num(seq.to_string())),
        ("duplicate".into(), Value::Bool(duplicate)),
    ]))
}
