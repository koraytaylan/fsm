---
id: deterministic-scheduler
title: "Deterministic Scheduler"
workstream: "0037"
kind: task
depends_on:
  - read-only-watcher
  - handler-table-config
  - request-id-and-idempotency
gated: false
touches:
  - crates/fsm-execute/src/sched.rs
  - crates/fsm-execute/tests/sched.rs
status: planned
merged_as: ""
---
# Deterministic Scheduler

The scheduler is the plan's brain and is pure: given an `Observation` and a `now_ms` it emits `Directive`s without spawning a process, touching the store, or reading a clock. Every decision is taken from journal-derived facts, so a fresh process with an empty in-flight map reaches the same conclusions its killed predecessor did.

**Steps:**

1. Implement `enum Directive { Start { effect: PendingEffect, argv: Vec<String>, timeout_ms: i64 }, Kill { effect_id: String, reason: KillReason }, PollDeadline { instance_id: String, deadline: String, due_ms: i64, request_id: String }, SendEvent { instance_id: String, effect_id: String, event: String, payload: Value, stamps: Vec<String>, request_id: String } }`.
2. Implement `Scheduler { table: HandlerTable, inflight: BTreeMap<String, Inflight>, issued_polls: BTreeSet<(String, String, i64)> }` with `Scheduler::new(table)`. `Inflight { effect: PendingEffect, deadline_ms: i64 }`. It holds **no** clock — time arrives as `on_observation`'s `now_ms`, which is what keeps every decision a pure function of its inputs; the driver owns the one `Clock`. `inflight` tracks only children running in *this* process (a pid has no journal representation); `issued_polls` de-duplicates polls within a process life. Neither is required for correctness after a restart.
3. Implement `fn on_observation(&mut self, obs: &Observation, now_ms: i64) -> Vec<Directive>` applying the architecture §0037 decision table in order (`obs.settled` carries each effect's resolved name, so rule 3 can look up its handler):
   1. `Start` for each `obs.pending` effect that has a handler, has no `inflight` entry, and whose `ack_rid` is not in `obs.claimed_request_ids` — substituting argv via `config::substitute`, deadline `now_ms + timeout_ms`;
   2. nothing for a pending effect with no handler (default-deny), recorded for `fn unhandled(&self) -> &[String]` so the service loop logs `exec/unhandled_effect` once per effect;
   3. `SendEvent` for each `obs.settled` effect whose handler declares the matching `on_ok`/`on_failed` and whose `event_rid` is **not** in `obs.claimed_request_ids` — the rule that resumes an advance lost to a kill between ack and send, and equally honours an effect a human acked from the CLI;
   4. `PollDeadline` for each `due_deadline` whose `poll_rid` is neither in `obs.claimed_request_ids` nor in `issued_polls`;
   5. `Kill { reason: Cancelled }` for each inflight effect whose instance is in `obs.cancellations`;
   6. `Kill { reason: Timeout }` for each inflight effect past its `deadline_ms`.
4. Enforce the invariants the tests pin: never two directives for the same `effect_id` in one tick, and never a directive whose `request_id` is already claimed in the journal. Expose `fn complete(&mut self, effect_id: &str)` so the runner's reap clears the in-flight entry; correctness after a restart must not depend on it having been called.
5. Derive every `request_id` by calling the `rid` module — the scheduler composes keys, it never invents them.

**Tests:**

- A pending effect with a handler → exactly one `Start` with correctly substituted `argv` and `deadline_ms == now + timeout`; the same observation re-presented while the effect is still inflight does not re-`Start`.
- An observation whose `claimed_request_ids` already contains the effect's `ack_rid` → no `Start`, even with an empty `inflight` (the fresh-process guard).
- A pending effect with no handler → no directive, and the effect appears in `unhandled()` exactly once.
- A `settled` effect with a declared `on_ok` and no claimed `event_rid` → exactly one `SendEvent` carrying the handler's payload and stamps; with the `event_rid` already claimed → none; with no `on_ok` declared → none.
- A due deadline → one `PollDeadline` carrying the derived `request_id`; a not-yet-due deadline → none; re-presenting the same due → none.
- An inflight effect whose instance is cancelled → `Kill { reason: Cancelled }`; a second observation of the same cancel → no duplicate `Kill`.
- An inflight effect whose `deadline_ms` is passed by a later `now_ms` → `Kill { reason: Timeout }` exactly once.
- Restart equivalence: build a scheduler, feed observation A, then build a **fresh** scheduler from the same table and feed the *post-ack* observation — the fresh one emits the outstanding `SendEvent` and no `Start`, proving decisions are journal-derived.
- Determinism: same `Observation` + same `now_ms` → byte-identical `Directive` sequence across two runs, with no clock anywhere in the scheduler to make it otherwise.

- **Done when:** `cargo test -p fsm-execute --test sched` passes every decision-table row under explicit `now_ms` values, the no-duplicate, no-already-claimed, and restart-equivalence invariants hold, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
