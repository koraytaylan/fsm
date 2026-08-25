---
id: ack-and-advance-pipeline
title: "Ack And Advance Pipeline"
workstream: "0038"
kind: task
depends_on:
  - subprocess-runner
  - request-id-and-idempotency
gated: false
touches:
  - crates/fsm-execute/src/run.rs
  - crates/fsm-execute/src/service.rs
  - crates/fsm-execute/tests/pipeline.rs
status: planned
merged_as: ""
---
# Ack And Advance Pipeline

The pipeline is the one component holding the **writer** store: it maps a `RunOutcome` to journaled reality through the store's own idempotent mutators — ack, then (only on `ok` and only if the machine declared a `success_event`) the advance `send_event` — so every outcome lands in the tamper-evident chain with a derived `request_id`.

**Steps:**

1. Implement `struct Pipeline { clock: Box<dyn Clock> }` plus a method `fn settle(&mut self, store: &mut Store, instance_id: &str, effect: &PendingEffect, outcome: RunOutcome, handler: &HandlerSpec, expect_seq: u64) -> Result<SettleOutcome, ExecError>` in `run.rs`.
2. Ack first: `store.ack_effect_outcome_on(clock, instance_id, effect_id, &ack_rid(effect_id), outcome_str, Some(result))` where `outcome_str = "ok"` iff `Completed` with `status == 0`, else `"failed"`; `result = {"status": i32, "stdout": "...", "stderr": "..."}` from the bounded capture. An ack rejected as not-pending (another path settled the effect) is benign — log and stop, return `SettleOutcome::AlreadySettled`.
3. On `ok` with `handler.success_event = Some(ev)`: read the post-ack `enabled_events`; send `ev` only if present, via `store.send_event_stamp_on(..., &ev, payload, &event_rid(effect_id, ev), Some(expect_seq = ack's returned seq), &[])`; if `ev` is *not* enabled, send nothing and return `SettleOutcome::AckedNoAdvance` (never fire a deliberate `run/not_enabled`). On `ok` with no `success_event`, likewise `AckedNoAdvance`. On `failed`, send `failure_event` if declared and enabled, else halt.
4. A `req/seq_mismatch` on the advance send → re-read the instance and retry the *same* `request_id` once (exact-once retry rule), else surface `exec/store`.
5. Implement the deadline arm `fn poll(&mut self, store: &mut Store, instance_id: &str, deadline: &str, due_ms: i64) -> Result<Value, ExecError>` → `store.poll_instance_deadline_on(clock, instance_id, &poll_rid(...), None)`, mapping `NotDue` to a journaled observation and errors to `exec/store`.
6. Implement `service::tick(watcher, scheduler, runner, pipeline, store: &mut Store, now_ms) -> Vec<String>` in `service.rs` composing one full pass — scan → on_observation → spawn/poll/kill → settle/poll — returning one human line per action for the golden trace and the CLI. Writer acquisition (`Store::open` → use → drop) happens inside the tick's settle phase only, per architecture §0039.

**Tests:**

- Happy path: a `Completed status:0` outcome acks `ok` with the stored `request_id == exec-ack-{effect_id}`, then sends the declared `success_event`; the instance leaves the emitting state; the journal shows `effect_acked` then `event_applied` in that order.
- Non-zero exit acks `failed` and, with a declared+enabled `failure_event`, sends it; without one, sends nothing and leaves the instance in place.
- `success_event` declared but *not* enabled post-ack → no send, `AckedNoAdvance`, no `run/not_enabled` rejection in the journal.
- Ack of an already-settled effect → `AlreadySettled`, no second `effect_acked` record, no panic.
- Restart determinism: settle once, construct a *fresh* `Pipeline`, re-settle with the same inputs → second ack returns `duplicate: true`, journal unchanged.
- Deadline: a due deadline poll transitions the instance per the machine; `NotDue` is journaled and its `request_id` claimed so a repeat is a replay.
- `service::tick` on a fabricated schedule returns the exact ordered action lines.

- **Done when:** `cargo test -p fsm-execute --test pipeline` passes every row including ordering (ack-before-advance), restart-duplicate, and seq-mismatch retry, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
