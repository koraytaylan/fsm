use std::collections::BTreeMap;

use fsm_core::json::Value;
use fsm_core::record::RecordKind;

use crate::store::{ErrorObj, Store};

impl Store {
    pub fn annotate(
        &mut self,
        instance_id: &str,
        request_id: &str,
        note: &str,
    ) -> Result<Value, ErrorObj> {
        self.ensure_writable()?;
        if let Some(r) = self.claim_request(request_id, Self::fp_annotate(instance_id, note))? {
            return r;
        }
        Self::check_journalled_size("note", &Value::Str(note.into()), request_id)?;
        if !self.state.instances.contains_key(instance_id) {
            return Err(ErrorObj::new("req/instance_not_found", instance_id).request_id(request_id));
        }
        let mut body = BTreeMap::new();
        body.insert("instance_id".into(), Value::Str(instance_id.into()));
        body.insert("request_id".into(), Value::Str(request_id.into()));
        body.insert("note".into(), Value::Str(note.into()));
        let rec = self.append_rec(
            RecordKind::Annotated,
            Value::Obj(body),
            &mut crate::clock::GlobalClock,
        )?;
        self.note_record(&rec);
        self.history
            .entry(instance_id.into())
            .or_default()
            .push(rec.seq);
        let mut m = BTreeMap::new();
        m.insert("ok".into(), Value::Str("true".into()));
        m.insert("note".into(), Value::Str(note.into()));
        m.insert("request_id".into(), Value::Str(request_id.into()));
        let resp = Value::Obj(m);
        self.commit_dedup(request_id, resp.clone(), rec.seq);
        self.finish_commit();
        Ok(resp)
    }
}
