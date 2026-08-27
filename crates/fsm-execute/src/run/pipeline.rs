//! The one component that writes.
//!
//! Split from the runner when `run.rs` passed the workspace's thousand-line
//! ceiling, along the seam its own module doc already named: the runner owns
//! no policy and spawns processes, and this owns no processes and maps an
//! outcome onto journaled reality through the store's own idempotent mutators.
//!
//! Plan 0016 task 7702.

use fsm_core::json::Value;
use fsm_store::clock::Clock;
use fsm_store::store::Store;

use crate::config::{Advance, HandlerSpec};
use crate::effect::PendingEffect;
use crate::error::ExecError;
use crate::rid::{ack_rid, attempt_rid, event_rid, poll_rid};

use super::{Exhaustion, RunOutcome};

/// What settling one outcome did to the journal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettleOutcome {
    /// Acked, and the declared advance event was sent.
    Advanced,
    /// Acked, and no advance was sent — none declared, or not enabled.
    AckedNoAdvance,
    /// Another path had already settled the effect.
    AlreadySettled,
}

/// The one component that writes.
///
/// It holds no state: everything it needs to decide is either journaled or
/// handed to it, which is why a fresh `Pipeline` after a restart behaves
/// exactly like the one that died.
pub struct Pipeline;

impl Pipeline {
    /// Ack one outcome, then send the declared advance event when the engine
    /// says that event is enabled.
    ///
    /// Ack first, always. The ack is what clears the effect from the outbox,
    /// so a kill between the two writes leaves a journal that says "this ran,
    /// its advance did not" — which the next executor can read and finish.
    pub fn settle(
        &mut self,
        store: &mut Store,
        clock: &mut dyn Clock,
        effect: &PendingEffect,
        outcome: RunOutcome,
        handler: &HandlerSpec,
        exhausted: Option<Exhaustion>,
    ) -> Result<SettleOutcome, ExecError> {
        let acked = if outcome.succeeded() { "ok" } else { "failed" };
        // Exhaustion is a failure like any other from here on: same ack, same
        // outcome word, same declared advance. Only the `result` says how it
        // got here, which is what leaves an existing `on_failed` path working
        // unchanged and makes the dead-letter report derivable.
        let result = match exhausted {
            Some(exhaustion) => outcome.exhausted_ack_result(exhaustion.attempts, exhaustion.class),
            None => outcome.ack_result(),
        };
        let ack_seq = match store.ack_effect_outcome_on(
            clock,
            &effect.instance_id,
            &effect.effect_id,
            &ack_rid(&effect.effect_id),
            acked,
            Some(result),
        ) {
            Ok(response) => response
                .get("seq")
                .and_then(Value::as_num)
                .and_then(|seq| seq.parse::<u64>().ok()),
            // The store journals a `request_rejected` record for an ack of an
            // effect that is not pending and returns this exact code. Another
            // path already settled it; that is benign, and the rejection also
            // claims the derived key so a later re-issue replays it.
            Err(error) if error.code == "req/field_unknown" => {
                return Ok(SettleOutcome::AlreadySettled);
            }
            Err(error) => return Err(ExecError::store(&error)),
        };
        let declared = if outcome.succeeded() {
            handler.on_ok.as_ref()
        } else {
            handler.on_failed.as_ref()
        };
        // No declared advance is a deliberate stall, not an omission: the
        // instance waits for a deadline or an external event.
        let Some(advance) = declared else {
            return Ok(SettleOutcome::AckedNoAdvance);
        };
        self.advance(
            store,
            clock,
            &effect.effect_id,
            &effect.instance_id,
            advance,
            ack_seq,
        )
    }

    /// Journal one failed attempt, leaving the effect pending.
    ///
    /// The counterpart to [`Pipeline::settle`] for a failure the policy will
    /// try again: nothing is acked, nothing is advanced, and the effect stays
    /// in the outbox where the next scan finds it. The record is the whole
    /// point — it is what makes the count and the backoff deadline survive a
    /// restart, since a process that dies between the failure and the retry
    /// remembers nothing.
    ///
    /// The run's capture goes into the record so an operator reading a
    /// dead letter can see why each earlier attempt failed, not only the last.
    ///
    /// `Ok(false)` means another writer had already settled the effect — the
    /// same benign race [`Pipeline::settle`] reports as
    /// [`SettleOutcome::AlreadySettled`].
    pub fn attempt(
        &mut self,
        store: &mut Store,
        clock: &mut dyn Clock,
        effect: &PendingEffect,
        outcome: &RunOutcome,
        attempt: u32,
    ) -> Result<bool, ExecError> {
        match store.attempt_effect_on(
            clock,
            &effect.instance_id,
            &effect.effect_id,
            &attempt_rid(&effect.effect_id, attempt),
            u64::from(attempt),
            Some(outcome.ack_result()),
        ) {
            Ok(_) => Ok(true),
            // The store journals a `request_rejected` for an attempt against
            // an effect that is not pending and returns this exact code,
            // exactly as it does for an ack of one.
            Err(error) if error.code == "req/field_unknown" => Ok(false),
            Err(error) => Err(ExecError::store(&error)),
        }
    }

    /// Send an advance for an effect already acknowledged in a previous life.
    ///
    /// The ack is already in the journal, so there is no `expect_seq` to hold
    /// anything still; the derived key makes a send that did land replay as
    /// `duplicate: true` rather than transition a second time.
    pub fn advance_only(
        &mut self,
        store: &mut Store,
        clock: &mut dyn Clock,
        effect_id: &str,
        instance_id: &str,
        advance: &Advance,
    ) -> Result<SettleOutcome, ExecError> {
        self.advance(store, clock, effect_id, instance_id, advance, None)
    }

    /// Poll one due deadline under a derived key.
    ///
    /// A `NotDue` observation is journaled and claims its key, exactly as SPEC
    /// describes, so a repeat of the same observation replays rather than
    /// polling again.
    pub fn poll(
        &mut self,
        store: &mut Store,
        clock: &mut dyn Clock,
        instance_id: &str,
        deadline: &str,
        due_ms: i64,
    ) -> Result<Value, ExecError> {
        store
            .poll_instance_deadline_on(
                clock,
                instance_id,
                &poll_rid(instance_id, deadline, due_ms),
                None,
            )
            .map_err(|error| ExecError::store(&error))
    }

    fn advance(
        &mut self,
        store: &mut Store,
        clock: &mut dyn Clock,
        effect_id: &str,
        instance_id: &str,
        advance: &Advance,
        expect_seq: Option<u64>,
    ) -> Result<SettleOutcome, ExecError> {
        if !advance_is_enabled(store, instance_id, advance)? {
            return Ok(SettleOutcome::AckedNoAdvance);
        }
        let request_id = event_rid(effect_id, &advance.event);
        match send(store, clock, instance_id, advance, &request_id, expect_seq) {
            Ok(()) => Ok(SettleOutcome::Advanced),
            // Something else advanced the instance between the ack and the
            // send. SPEC excludes `expect_seq` from the fingerprint and leaves
            // the key unconsumed on a mismatch, so the same request_id is
            // retried against the current seq.
            Err(error) if error.code == "req/seq_mismatch" => {
                let current = store.journal.last_seq;
                if !advance_is_enabled(store, instance_id, advance)? {
                    return Ok(SettleOutcome::AckedNoAdvance);
                }
                send(
                    store,
                    clock,
                    instance_id,
                    advance,
                    &request_id,
                    Some(current),
                )
                .map(|()| SettleOutcome::Advanced)
                .map_err(|error| ExecError::store(&error))
            }
            Err(error) => Err(ExecError::store(&error)),
        }
    }
}

fn send(
    store: &mut Store,
    clock: &mut dyn Clock,
    instance_id: &str,
    advance: &Advance,
    request_id: &str,
    expect_seq: Option<u64>,
) -> Result<(), fsm_store::store::ErrorObj> {
    let stamps: Vec<&str> = advance.stamps.iter().map(String::as_str).collect();
    // The store stamps into the payload it is given, so each attempt starts
    // from the table's own value. The request fingerprint is taken before
    // stamping, which is what lets a re-issue after a restart match even
    // though the stamped timestamp differs.
    let mut payload = advance.payload.clone();
    store
        .send_event_stamp_on(
            clock,
            instance_id,
            &advance.event,
            &mut payload,
            request_id,
            expect_seq,
            &stamps,
        )
        .map(|_| ())
}

/// Whether the engine would accept this advance right now.
///
/// Two conditions, and neither is redundant. Presence in `enabled_events` is
/// not a gate at all — every declared event appears there with a status. And
/// the status alone is not enough either, because `enabled_events` reasons
/// from the configuration rather than the lifecycle: cancelling an instance
/// leaves its configuration in place, so a cancelled instance still reports
/// its events as enabled and only `step` refuses — by journaling an
/// `event_rejected` that burns the derived key for good.
fn advance_is_enabled(
    store: &Store,
    instance_id: &str,
    advance: &Advance,
) -> Result<bool, ExecError> {
    let view = store
        .instance_view(instance_id, None, None)
        .map_err(|error| ExecError::store(&error))?;
    if view.get("status").and_then(Value::as_str) != Some("running") {
        return Ok(false);
    }
    let Some(events) = view.get("enabled_events").and_then(Value::as_arr) else {
        return Ok(false);
    };
    let Some(entry) = events
        .iter()
        .find(|event| event.get("event").and_then(Value::as_str) == Some(advance.event.as_str()))
    else {
        return Ok(false);
    };
    Ok(match entry.get("status").and_then(Value::as_str) {
        Some("enabled") => true,
        // A guard that reads the payload cannot be decided without one, so an
        // advance that carries fields is worth attempting and one that carries
        // nothing is not.
        Some("depends_on_payload") => {
            !advance.stamps.is_empty()
                || advance
                    .payload
                    .as_obj()
                    .is_some_and(|fields| !fields.is_empty())
        }
        _ => false,
    })
}
