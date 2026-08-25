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
status: done
merged_as: ""
---
# Ack And Advance Pipeline

The pipeline is the one component that writes: it maps a `RunOutcome` to journaled reality through the store's own idempotent mutators — ack, then (only when the handler declared an advance and the engine says that event is enabled) the `send_event` — so every outcome lands in the tamper-evident chain under a derived `request_id`.

**Steps:**

1. Implement `struct Pipeline` and `fn settle(&mut self, store: &mut Store, clock: &mut dyn Clock, effect: &PendingEffect, outcome: RunOutcome, handler: &HandlerSpec) -> Result<SettleOutcome, ExecError>` in `run.rs`. The clock is borrowed per call, matching the store's `_on` mutators — the driver owns the one `Clock` in the process and nothing else keeps its own.
2. Ack first: `store.ack_effect_outcome_on(clock, instance_id, effect_id, &ack_rid(effect_id), outcome_str, Some(outcome.ack_result()))` where `outcome_str = "ok"` iff `Completed` with `status == 0`, else `"failed"` — which is also what `Killed` and `SpawnFailed` ack, through the deterministic `ack_result()` forms task `3801` defines. An ack of a not-pending effect returns `Err(ErrorObj)` with code **`req/field_unknown`** (the store journals a `request_rejected` record and claims the key) — match that exact code, treat it as benign, log, and return `SettleOutcome::AlreadySettled`.
3. On `ok` with `handler.on_ok = Some(adv)`: re-read `store.instance_view(instance_id, None, None)` — the ack response carries no `enabled_events` — and send only when **both** the instance's `status` is `running` **and** `adv.event`'s entry has `status == "enabled"` (or `"depends_on_payload"` with a non-empty `payload`/`stamps` on the advance). Neither check is redundant: presence in the array is not a gate at all, since every declared event appears there with a status; and the event status alone is not enough, because `enabled_events` reasons from the configuration rather than the lifecycle — cancel leaves the configuration untouched, so a cancelled instance still reports enabled events and only `step` refuses, journaling an `event_rejected` that burns the derived key. Send via `store.send_event_stamp_on(clock, instance_id, &adv.event, &mut payload, &event_rid(effect_id, &adv.event), Some(ack_seq), &adv.stamps)`. Otherwise return `SettleOutcome::AckedNoAdvance` and log the status — never fire an event the engine will reject. `failed` uses `on_failed` identically.
4. A `req/seq_mismatch` on the advance send → re-read the instance and retry the *same* `request_id` with the fresh seq once (SPEC excludes `expect_seq` from the fingerprint and leaves the key unconsumed), else surface `exec/store`.
5. Implement `fn advance_only(&mut self, store, clock, effect_id, instance_id, adv) -> Result<SettleOutcome, ExecError>` for the scheduler's `SendEvent` directive — the resume path after a kill between ack and send. Same two-condition gate (`status == running` **and** the event enabled), `expect_seq: None`, same derived `event_rid`, so a send that did happen replays as `duplicate: true`.
6. Implement `fn poll(&mut self, store, clock, instance_id, deadline, due_ms) -> Result<Value, ExecError>` → `store.poll_instance_deadline_on(clock, instance_id, &poll_rid(...), None)`, journaling `NotDue` as an observation and mapping errors to `exec/store`.
7. Implement two entry points in `service.rs`, because two callers own the writer differently. `tick_with(&mut Watcher, &mut Scheduler, &mut Runner, &mut Pipeline, &mut Store, &mut dyn Clock, now_ms) -> Vec<String>` does the work against a writer it is *lent*: scan → `on_observation` → spawn/poll/kill → settle/advance/poll, returning one action line per action. `tick(.., &Path, ..)` opens the writer, calls `tick_with`, and drops it. Embedded mode (task `3902`) calls `tick_with` with serve's existing handle, since a second `Store::open` in that process would collide with serve's own lock — and any test that drives a writer itself must drop that handle before calling `tick`, for the same reason.
8. `tick` opens the writer **once**, only when the tick has something to write, does every write under that one handle, and drops it before returning: `Store::open` folds the journal and `Drop` writes a snapshot, so a per-directive open would pay both costs per directive. Both `now_ms` and the clock are parameters on purpose — the caller reads `now_ms` from the clock once per tick so all decisions in that tick share one time, while the store's `_on` mutators go on consuming clock ticks of their own as they journal.
9. Call `scheduler.complete(effect_id)` on **every** terminal path — advanced, `AckedNoAdvance`, `AlreadySettled`, an `exec/store` failure, and a failed `spawn` — because an effect left marked in-flight is invisible to the start rule for the life of the process, which is the one way this loop can wedge. Action lines carry identifiers only (effect name, effect id, request id, event, outcome) — never a path, pid, temp dir, or duration, which is what keeps the golden byte-comparable.

**Tests:**

- Happy path: a `Completed status:0` outcome acks `ok` with `request_id == exec-ack-{effect_id}`, then sends the declared `on_ok.event`; the instance leaves the emitting state; the journal shows `effect_acked` then `event_applied` in that order.
- Stamped advance: an `on_ok` with `stamps: ["at"]` against an event declaring that field lands a payload containing the stamped timestamp, and the instance advances.
- Non-zero exit acks `failed` and, with a declared+enabled `on_failed`, sends it; without one, sends nothing and leaves the instance in place.
- `Killed { Timeout }`, `Killed { Cancelled }`, and `SpawnFailed` each ack `failed` with their documented `ack_result()` payload, and re-settling the same outcome replays as `duplicate: true` rather than conflicting.
- Terminal-instance effect: an effect emitted on entering a terminal state acks fine (cancel and completion do not clear `effects_pending`), and the advance is skipped — `AckedNoAdvance`, no `event_rejected` record.
- Cancelled-instance effect: cancel the instance while its effect is pending, then settle it. The ack lands, `enabled_events` still reports the advance event as enabled (cancel left the configuration in place), and the pipeline sends nothing because the status is not `running` — the row that proves the gate is two conditions, not one.
- In-flight bookkeeping: after a settle that returns `AlreadySettled`, and after a `spawn` that fails, the scheduler no longer holds the effect in flight, so the next observation can act on it.
- Event declared but **not enabled** post-ack (guard false) → no send, `AckedNoAdvance`, and no `event_rejected` record in the journal.
- Ack of an already-settled effect → `AlreadySettled` on `req/field_unknown`, no second `effect_acked` record, no panic.
- Restart determinism: settle once, construct a *fresh* `Pipeline`, re-settle with the same inputs → second ack returns `duplicate: true`, journal unchanged.
- Resume: ack an effect directly, then call `advance_only` with the same derived `event_rid` → the advance lands once; calling it twice replays as `duplicate: true` with no second `event_applied`.
- `seq_mismatch`: append an unrelated record between ack and send → the retry with the same `request_id` and refreshed seq succeeds, and the journal holds one `event_applied`.
- Deadline: a due deadline poll transitions the instance per the machine; `NotDue` is journaled and its `request_id` claimed so a repeat is a replay.
- `service::tick` on a fabricated schedule returns the exact ordered action lines and holds the writer for exactly one open — assert by taking the lock from a second handle immediately after `tick` returns. `tick_with` against a caller-held writer produces the identical lines without opening anything, which is what makes embedded mode possible.

- **Done when:** `cargo test -p fsm-execute --test pipeline` passes every row including ordering (ack-before-advance), the enabled-status gate, restart-duplicate, resume-only advance, and seq-mismatch retry, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
