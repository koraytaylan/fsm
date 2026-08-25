---
id: deterministic-scheduler
title: "Deterministic Scheduler"
workstream: "0037"
kind: task
depends_on:
  - read-only-watcher
  - handler-table-config
gated: false
touches:
  - crates/fsm-execute/src/sched.rs
  - crates/fsm-execute/tests/sched.rs
status: planned
merged_as: ""
---
# Deterministic Scheduler

The scheduler is the plan's brain and is pure: given an `Observation` and the in-flight set it emits `Directive`s without spawning a process or touching the store, holding the only `Clock` in the library so tests drive it with `FixedClock` and time is data everywhere.

**Steps:**

1. Implement `enum Directive { Start { effect: PendingEffect, argv: Vec<String>, timeout_ms: i64 }, Kill { effect_id: String }, PollDeadline { instance_id: String, deadline: String, due_ms: i64, request_id: String }, SendEvent { instance_id: String, event: String, payload: Value, request_id: String } }`.
2. Implement `Scheduler { table: HandlerTable, inflight: BTreeMap<String, Inflight>, clock: Box<dyn Clock> }` with `Scheduler::new(table, clock)`. `Inflight { effect: PendingEffect, deadline_ms: i64 }`.
3. Implement `fn on_observation(&mut self, obs: &Observation, now_ms: i64) -> Vec<Directive>` applying the architecture §0037 decision table in order: unhandled-effect default-deny (no directive, mark for `exec/unhandled_effect` log); Start for newly-pending effects with a handler and no inflight entry (substituting argv via `config::substitute`, deadline `now_ms + timeout_ms`); PollDeadline for due deadlines not already inflight for that due; Kill for inflight effects whose instance is in `obs.cancellations`; Kill for inflight effects past `deadline_ms`.
4. Enforce the invariant the tests pin: never two directives for the same `effect_id` at once, and never a `SendEvent` for an effect the scheduler has not recorded as completed (the runner, workstream 0038, emits the actual advance send; here expose `fn complete(&mut self, effect_id: &str)` so the completion is recorded and the effect is never re-`Start`ed).
5. Derive poll/ack/event `request_id`s by calling workstream 0037's `request-id` helpers (`sched` re-exports them for the runner's use).

**Tests:**

- A pending effect with a handler → exactly one `Start` with correctly substituted `argv` and `deadline_ms == now + timeout`; the same observation re-presented does not re-`Start`.
- A pending effect with no handler → no directive, and the scheduler reports the effect under an `unhandled` introspection for the service loop to log as `exec/unhandled_effect`.
- A due deadline → one `PollDeadline` carrying the derived `request_id`; a still-not-due deadline → none; re-presenting the same due does not re-issue.
- An inflight effect whose instance is cancelled → `Kill`; a second observation of the same cancel → no duplicate `Kill`.
- An inflight effect advanced past `deadline_ms` (via `FixedClock`) → `Kill` once.
- Determinism: same `Observation` + same `FixedClock` start → byte-identical `Directive` sequence across two runs.
- `complete(effect_id)` prevents re-`Start` on the next observation of a still-pending-then-cleared effect lifecycle.

- **Done when:** `cargo test -p fsm-execute --test sched` passes every decision-table row under `FixedClock`, the no-duplicate and no-premature-SendEvent invariants hold, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
