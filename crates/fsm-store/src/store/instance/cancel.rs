use std::collections::BTreeMap;

use fsm_core::hashes::{STATE_FORMAT, state_hash};
use fsm_core::json::Value;
use fsm_core::machine::Status;
use fsm_core::record::RecordKind;

use crate::store::{ErrorObj, Store};

impl Store {
    pub fn cancel_instance(
        &mut self,
        instance_id: &str,
        request_id: &str,
    ) -> Result<Value, ErrorObj> {
        self.cancel_instance_reason(instance_id, request_id, "")
    }

    pub fn cancel_instance_reason(
        &mut self,
        instance_id: &str,
        request_id: &str,
        reason: &str,
    ) -> Result<Value, ErrorObj> {
        self.cancel_instance_reason_on(
            &mut crate::clock::GlobalClock,
            instance_id,
            request_id,
            reason,
        )
    }

    pub fn cancel_instance_reason_on(
        &mut self,
        clock: &mut dyn crate::clock::Clock,
        instance_id: &str,
        request_id: &str,
        reason: &str,
    ) -> Result<Value, ErrorObj> {
        self.ensure_writable()?;
        if let Some(r) = self.claim_request(request_id, Self::fp_cancel(instance_id, reason))? {
            return r;
        }
        if !self.state.instances.contains_key(instance_id) {
            return Err(ErrorObj::new("req/instance_not_found", instance_id).request_id(request_id));
        }
        let mid = self
            .state
            .instance_machines
            .get(instance_id)
            .cloned()
            .ok_or_else(|| {
                ErrorObj::new("req/instance_not_found", instance_id).request_id(request_id)
            })?;
        let mut post = self.state.instances.get(instance_id).unwrap().clone();
        post.status = Status::Cancelled;
        post.deadlines.clear();
        let sh = state_hash(&mid, instance_id, self.journal.last_seq + 1, &post);
        let mut body = BTreeMap::new();
        body.insert("instance_id".into(), Value::Str(instance_id.into()));
        body.insert("request_id".into(), Value::Str(request_id.into()));
        body.insert("reason".into(), Value::Str(reason.into()));
        body.insert("state_hash".into(), Value::Str(sh));
        body.insert("state_format".into(), Value::Str(STATE_FORMAT.into()));
        let rec = self.append_rec(RecordKind::InstanceCancelled, Value::Obj(body), clock)?;
        self.state.instances.insert(instance_id.into(), post);
        self.note_record(&rec);
        self.history
            .entry(instance_id.into())
            .or_default()
            .push(rec.seq);
        let resp = self.instance_view(instance_id, Some(request_id), Some(false))?;
        self.commit_dedup(request_id, resp.clone(), rec.seq);
        self.finish_commit();
        Ok(resp)
    }
}
