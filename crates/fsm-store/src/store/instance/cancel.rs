use std::collections::BTreeMap;

use fsm_core::hashes::child_instance_id;
use fsm_core::hashes::{STATE_FORMAT, state_hash};
use fsm_core::json::Value;
use fsm_core::machine::{InvokeStatus, Status};
use fsm_core::record::RecordKind;

use crate::store::{ErrorObj, Store};

/// Why a child was cancelled by its parent leaving the invoking state.
pub(crate) fn parent_exit_reason(parent_id: &str, slot: &str) -> String {
    format!("parent-exit:{parent_id}/{slot}")
}

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
        // Cancelling a parent cancels every running child, depth-first and
        // bounded by the graph's admitted depth. A child that already
        // settled is skipped, not re-cancelled.
        self.cascade_cancel(
            clock,
            instance_id,
            request_id,
            &format!("parent-cancel:{instance_id}"),
        );
        let resp = self.instance_view(instance_id, Some(request_id), Some(false))?;
        self.commit_dedup(request_id, resp.clone(), rec.seq);
        self.finish_commit();
        Ok(resp)
    }

    /// Cancel the running children of `instance_id`, depth-first.
    ///
    /// Each cancellation is its own record, because a cancellation is
    /// idempotent and state-independent: a crash partway leaves children
    /// running-but-unreferenced, which the orphan sweep finishes, and that is
    /// a better trade than a group commit that would change the
    /// one-fsync-per-record durability claim for one recoverable window.
    pub(crate) fn cascade_cancel(
        &mut self,
        clock: &mut dyn crate::clock::Clock,
        instance_id: &str,
        request_id: &str,
        reason: &str,
    ) {
        let mut frontier = vec![(instance_id.to_string(), 0usize)];
        let mut index = 0;
        while let Some((parent_id, depth)) = frontier.pop() {
            if depth >= fsm_core::limits::MAX_INVOKE_DEPTH {
                continue;
            }
            let running: Vec<(String, String)> = self
                .state
                .instances
                .get(&parent_id)
                .map(|parent| {
                    parent
                        .invocations
                        .iter()
                        .filter(|(_, invocation)| invocation.status == InvokeStatus::Running)
                        .map(|(slot, _)| (slot.clone(), child_instance_id(&parent_id, slot)))
                        .collect()
                })
                .unwrap_or_default();
            for (slot, child_id) in running {
                let settled = self
                    .state
                    .instances
                    .get(&child_id)
                    .is_none_or(|child| child.status != Status::Running);
                if settled {
                    continue;
                }
                index += 1;
                let _ = self.cancel_child_record(
                    clock,
                    &child_id,
                    &format!("{request_id}/cascade-{index}"),
                    reason,
                );
                let _ = slot;
                frontier.push((child_id, depth + 1));
            }
        }
    }

    /// One journaled cancellation of a child, with no cascade of its own and
    /// no idempotency claim of the caller's key.
    pub(crate) fn cancel_child_record(
        &mut self,
        clock: &mut dyn crate::clock::Clock,
        instance_id: &str,
        request_id: &str,
        reason: &str,
    ) -> Result<(), ErrorObj> {
        let Some(instance) = self.state.instances.get(instance_id) else {
            return Ok(());
        };
        if instance.status != Status::Running {
            return Ok(());
        }
        let mid = self
            .state
            .instance_machines
            .get(instance_id)
            .cloned()
            .unwrap_or_default();
        let mut post = instance.clone();
        post.status = Status::Cancelled;
        post.deadlines.clear();
        let mut body = BTreeMap::new();
        body.insert("instance_id".into(), Value::Str(instance_id.into()));
        body.insert("request_id".into(), Value::Str(request_id.into()));
        body.insert("reason".into(), Value::Str(reason.into()));
        body.insert(
            "state_hash".into(),
            Value::Str(state_hash(
                &mid,
                instance_id,
                self.journal.last_seq + 1,
                &post,
            )),
        );
        body.insert("state_format".into(), Value::Str(STATE_FORMAT.into()));
        let record = self.append_rec(RecordKind::InstanceCancelled, Value::Obj(body), clock)?;
        self.state.instances.insert(instance_id.into(), post);
        self.note_record(&record);
        self.history
            .entry(instance_id.into())
            .or_default()
            .push(record.seq);
        Ok(())
    }

    /// Cancel the children an applied transition left behind: the core
    /// reported them when it removed their slots.
    pub(crate) fn cancel_exited_children(
        &mut self,
        clock: &mut dyn crate::clock::Clock,
        parent_id: &str,
        cancelled: &[fsm_core::machine::CancelledChild],
    ) {
        for (index, child) in cancelled.iter().enumerate() {
            let child_id = child_instance_id(parent_id, &child.slot);
            let _ = self.cancel_child_record(
                clock,
                &child_id,
                &format!("{parent_id}/{}/exit-{index}", child.slot),
                &parent_exit_reason(parent_id, &child.slot),
            );
        }
    }

    /// Every running child whose parent is gone, settled, or no longer holds
    /// its slot: work nobody will ever consume.
    ///
    /// Reported, never repaired here: an open must not write, and that rule
    /// holds for the writer path as firmly as for `open_read_only`.
    pub fn orphaned_children(&self) -> Vec<Value> {
        let mut orphans = Vec::new();
        for (child_id, child) in &self.state.instances {
            if child.status != Status::Running {
                continue;
            }
            let Some((parent_id, slot)) = self.parent_of(child_id) else {
                continue;
            };
            let referenced = self.state.instances.get(&parent_id).is_some_and(|parent| {
                parent.status == Status::Running
                    && parent
                        .invocations
                        .get(&slot)
                        .is_some_and(|invocation| invocation.status == InvokeStatus::Running)
            });
            if !referenced {
                orphans.push(Value::Obj(BTreeMap::from([
                    ("instance_id".into(), Value::Str(child_id.clone())),
                    ("parent_instance_id".into(), Value::Str(parent_id)),
                    ("slot".into(), Value::Str(slot)),
                ])));
            }
        }
        orphans
    }

    /// The parent and slot an instance was invoked from.
    ///
    /// Read from the index the journal built, never inferred from the id: an
    /// instance whose id merely looks derived is not a child.
    pub fn parent_of(&self, child_id: &str) -> Option<(String, String)> {
        self.parents.get(child_id).cloned()
    }

    /// Cancel every orphan, one record each. Explicit: never at open.
    pub fn cancel_orphans_on(
        &mut self,
        clock: &mut dyn crate::clock::Clock,
        request_id: &str,
    ) -> Result<Vec<String>, ErrorObj> {
        self.ensure_writable()?;
        let orphans: Vec<String> = self
            .orphaned_children()
            .into_iter()
            .filter_map(|orphan| {
                orphan
                    .get("instance_id")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .collect();
        for (index, child_id) in orphans.iter().enumerate() {
            self.cancel_child_record(
                clock,
                child_id,
                &format!("{request_id}/orphan-{index}"),
                "orphan",
            )?;
        }
        if !orphans.is_empty() {
            self.finish_commit();
        }
        Ok(orphans)
    }
}
