---
id: inflight-concurrency-cap
title: "Inflight Concurrency Cap"
workstream: "0076"
kind: task
depends_on:
  - backoff-schedule
  - retry-policy-config
gated: false
touches:
  - crates/fsm-execute/src/config.rs
  - crates/fsm-execute/src/sched.rs
  - crates/fsm-execute/tests/concurrency.rs
status: planned
merged_as: ""
---
# Inflight Concurrency Cap

An outbox holding five hundred pending effects currently spawns five hundred subprocesses, and the fix has to be deterministic — the same observation must always produce the same directives, or restart equivalence stops meaning anything.

**Steps:**

1. Add two top-level keys to the handler table in `crates/fsm-execute/src/config.rs`: `max_inflight` (default 8, range 1 to 64) and `max_inflight_per_instance` (default 2, range 1 to 16). Both are optional; both defaults are chosen so an existing table gains a bound it almost certainly never hits.
2. Add them to the table's closed top-level key set so a misspelling is refused rather than ignored, matching how `HANDLER_KEYS` treats a misspelled handler key.
3. In `crates/fsm-execute/src/sched.rs`, apply both caps when selecting `Start` directives, counting the process-local `inflight` map toward them — a cap on concurrency is inherently about what this process is running now, which is the one legitimate use of that map.
4. Order candidates by a **stable total order** before applying the caps. `effect_id` is `{instance}/{seq}/{k}` and is therefore totally ordered; `7602` refines this ordering for fairness, and this task establishes that an ordering exists and is applied deterministically.
5. Extend the existing determinism test rather than writing a parallel one: the same observation and `now_ms` must produce a byte-identical directive sequence, caps included.
6. **`log()` what a cap deferred**, once per tick, at the identifier-only level: a tick that starts 8 of 40 candidates says so with `exec/inflight_deferred`. Silent truncation reads as "nothing to do" and is exactly the failure an operator cannot diagnose.
7. Apply the caps to `Start` directives only. A `Kill`, an `Ack`, a `SendEvent`, or a `PollDeadline` is bookkeeping against the journal, costs no subprocess, and must never be deferred by a concurrency bound.

**Tests:**

- `crates/fsm-execute/tests/concurrency.rs`: 40 pending effects across 10 instances with `max_inflight: 8` yields exactly 8 `Start` directives.
- With 8 already in flight, the next observation yields 0 `Start` directives; completing one frees exactly one slot.
- `max_inflight_per_instance: 2` limits one instance to 2 concurrent starts even when the global cap has room.
- Both caps together: the binding one wins, and the test covers each binding in turn.
- Determinism: the same observation and `now_ms` yield a byte-identical directive sequence across 100 runs.
- Caps apply only to `Start`: a tick at the cap still emits `Kill`, `SendEvent`, and `PollDeadline` directives.
- The deferral log line appears once per tick, names the deferred count, and carries identifiers only.
- Config: `max_inflight` of 0 and of 65 are each `exec/config`; a misspelled key is refused by the closed key set.
- A table with neither key gets the documented defaults.
- Restart: a fresh scheduler with an empty `inflight` fed the same observation emits up to the cap, which is correct — the cap is about this process's concurrency, not about journal state.

- **Done when:** `cargo test -p fsm-execute --test concurrency` passes every case above, both caps bind deterministically over a stable order, deferrals are logged rather than silent, non-`Start` directives are never deferred, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
