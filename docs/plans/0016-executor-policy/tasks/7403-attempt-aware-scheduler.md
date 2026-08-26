---
id: attempt-aware-scheduler
title: "Attempt Aware Scheduler"
workstream: "0074"
kind: task
depends_on:
  - attempt-record-kind
  - retry-policy-config
gated: false
touches:
  - crates/fsm-execute/src/watch.rs
  - crates/fsm-execute/src/sched.rs
  - crates/fsm-execute/src/rid.rs
  - crates/fsm-execute/tests/sched_retry.rs
status: planned
merged_as: ""
---
# Attempt Aware Scheduler

The whole point of journaling attempts is that a fresh process reaches the same conclusion its killed predecessor did, so the attempt count and the deadline both come from the observation and neither comes from memory.

**Steps:**

1. In `crates/fsm-execute/src/watch.rs`, extend `Observation` with per-effect attempt state: for each pending `effect_id`, the highest `attempt` seen in an `effect_attempted` record and the `ts` of that record. Read it from the same journal walk the watcher already performs — do not add a second scan.
2. In `crates/fsm-execute/src/rid.rs`, add `attempt_rid(effect_id, attempt) -> "exec-try-{effect_id}-{attempt}"`, derived from journaled content like every other key, so a restart re-issuing the same attempt replays rather than double-writing.
3. In `crates/fsm-execute/src/sched.rs`, replace the existing start rule with the attempt-aware one: a pending effect with a handler, **no** `inflight` entry, an unclaimed `ack_rid`, an attempt count below `attempts`, and a backoff deadline at or before `now_ms` → `Start` carrying `attempt = last + 1`.
4. Produce **no directive at all** for an effect still inside its backoff window. That is what makes backoff free: the executor does not sleep and does not hold a slot, it simply does not act yet.
5. Keep `inflight` process-local and keep correctness independent of it: a fresh scheduler with an empty `inflight`, fed the same observation, must emit the same directives. This is the property plan 0008's restart-equivalence test pins and this task must extend rather than weaken.
6. Classify a failure into one of `on`'s classes at settle time and consult the handler's `on` list: a failure whose class is not listed goes straight to the ack path with no further attempts, regardless of remaining budget.
7. **Never retry a `KillReason::Cancelled` outcome**, whatever the table says — `7402` refuses the class at config time, and this is the run-time half of the same rule.

**Tests:**

- `crates/fsm-execute/tests/sched_retry.rs`: a pending effect with no attempt records and `attempts: 3` yields one `Start` with `attempt: 1`.
- After one failed attempt, an observation past the backoff deadline yields `Start` with `attempt: 2`; before the deadline it yields nothing.
- After `attempts - 1` failed attempts, the next start is the last; after `attempts`, no `Start` is emitted and the ack path takes over.
- **Restart equivalence:** a fresh scheduler fed the post-attempt-2 observation emits `Start` with `attempt: 3` and no duplicate — proving the count is journal-derived.
- An effect whose `attempt_rid` is already claimed yields no `Start`, even with an empty `inflight`.
- A failure class absent from `on` produces no retry even with attempts remaining.
- A cancelled kill is never retried, even if the table were somehow to list the class.
- Determinism: the same observation and the same `now_ms` produce a byte-identical directive sequence across two runs.
- A handler with no `retry` block behaves exactly as before: one attempt, then the ack.
- The watcher performs one journal walk per scan, not two.

- **Done when:** `cargo test -p fsm-execute --test sched_retry` passes every case above including restart equivalence mid-retry, backoff emits no directive rather than sleeping, cancelled outcomes are never retried, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
