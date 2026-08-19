use std::collections::BTreeMap;

use fsm_core::json::Value;
use fsm_core::record::RecordKind;
use fsm_core::replay::STATE_ROOT_FORMAT;

use super::{ErrorObj, Store};

impl Store {
    pub fn maybe_snapshot(&self) -> Result<(), ErrorObj> {
        self.ensure_writable()?;
        if self.journal.last_seq > 0 && self.journal.last_seq.is_multiple_of(10_000) {
            crate::snapshot::write_snapshot(&self.data_dir, &self.state)?;
        }
        Ok(())
    }

    fn checkpoint_for_snapshot_on(
        &mut self,
        clock: &mut dyn crate::clock::Clock,
    ) -> Result<(), ErrorObj> {
        let current_root = crate::snapshot::materialize_state_root(&self.state);
        if self
            .records
            .last()
            .filter(|rec| rec.kind == RecordKind::StateCheckpoint)
            .and_then(|rec| rec.body.get("state_root").and_then(Value::as_str))
            == Some(current_root.as_str())
        {
            return Ok(());
        }
        let seq = self.journal.last_seq.saturating_add(1);
        let root = crate::snapshot::materialize_state_root_at(&self.state, seq);
        let rec = self
            .journal
            .append_at(
                RecordKind::StateCheckpoint,
                Value::Obj(BTreeMap::from([
                    ("state_root".into(), Value::Str(root)),
                    (
                        "state_root_format".into(),
                        Value::Str(STATE_ROOT_FORMAT.into()),
                    ),
                ])),
                clock.now_ms(),
            )
            .map_err(|error| Self::journal_write_error(error, None))?;
        self.note_record(&rec);
        Ok(())
    }

    pub fn shutdown_snapshot_on(
        &mut self,
        clock: &mut dyn crate::clock::Clock,
    ) -> Result<(), ErrorObj> {
        self.ensure_writable()?;
        if self.journal.is_memory() || self.journal.last_seq == 0 {
            return Ok(());
        }
        self.checkpoint_for_snapshot_on(clock)?;
        crate::snapshot::write_snapshot(&self.data_dir, &self.state)?;
        Ok(())
    }

    pub fn shutdown_snapshot(&mut self) -> Result<(), ErrorObj> {
        self.shutdown_snapshot_on(&mut crate::clock::GlobalClock)
    }
}
