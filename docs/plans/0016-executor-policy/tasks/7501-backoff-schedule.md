---
id: backoff-schedule
title: "Backoff Schedule"
workstream: "0075"
kind: task
depends_on:
  - attempt-aware-scheduler
gated: false
touches:
  - crates/fsm-execute/src/service.rs
  - crates/fsm-execute/src/sched.rs
  - crates/fsm-execute/tests/backoff.rs
status: done
merged_as: ""
---
# Backoff Schedule

There is deliberately no jitter, and that is a decision rather than an omission: jitter would make the scheduler non-deterministic, and a single-node executor has no thundering herd to spread.

**Steps:**

1. In `crates/fsm-execute/src/sched.rs`, compute the deadline as `due_ms = last_attempt_ts + min(backoff_ms * 2^(attempt - 1), max_backoff_ms)`, where `last_attempt_ts` is the `ts` of the most recent `effect_attempted` record for that effect.
2. Derive **every** term from journaled facts or the handler table. `last_attempt_ts` is a record timestamp, not a value the process remembers, so a restarted executor computes the identical deadline and resumes the same wait rather than restarting it.
3. Use checked arithmetic on the shift and the addition. `2^15` against a large `backoff_ms` overflows a naive multiply, and an overflowed deadline in the past would turn backoff into a busy loop — saturate at `max_backoff_ms` and write the reason in a comment.
4. **Add no jitter and no randomness of any kind.** Record the reasoning in the module doc: the restart-equivalence property plan 0008 pins requires that the same observation and the same `now_ms` produce the same directives, and randomness would break it for a benefit that does not apply to one node.
5. Compare against the tick's single `now_ms`, so every decision in a tick sees one instant — the rule plan 0008 established for exactly this kind of comparison.
6. Cap the effective wait at `max_backoff_ms` from the first attempt whose computed value exceeds it, so a long `attempts` does not produce an unreachable deadline.
7. Add a `log()` line when a tick defers an effect solely because of backoff, at the identifier-only level, with `exec/inflight_deferred`'s sibling reason — an operator watching a quiet tick should be able to tell "waiting to retry" from "nothing to do".

**Tests:**

- `crates/fsm-execute/tests/backoff.rs`: with `backoff_ms: 1000`, deadlines after attempts 1, 2, and 3 are `+1000`, `+2000`, and `+4000` from their respective `last_attempt_ts`.
- `max_backoff_ms` caps the growth: attempt 10 with a 60000 ceiling yields exactly 60000.
- Overflow: `backoff_ms` near the integer ceiling with `attempts: 16` saturates at `max_backoff_ms` and never produces a past or negative deadline.
- Determinism: the same observation and `now_ms` produce identical directives across two runs, and across a fresh scheduler.
- The deadline is computed from the record timestamp, not from process start — assert by constructing an observation whose record `ts` is far in the past and confirming the effect is immediately due.
- An effect exactly at its deadline is due; one millisecond before is not.
- No randomness: run the same scenario 100 times and assert byte-identical directive sequences.
- A deferred-by-backoff tick emits its log line with identifiers only — no path, pid, or duration.

- **Done when:** `cargo test -p fsm-execute --test backoff` passes every case above including the overflow saturation and the 100-run determinism check, the deadline derives entirely from journaled facts, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.

**Landed:** `due_ms = last_attempt_ts + min(backoff_ms * 2^(attempt - 1), max_backoff_ms)`, with every term from a journaled fact or the handler table. The timestamp is the record's own, so an executor that comes up an hour later **resumes** the wait rather than restarting it — asserted by an observation whose record is an hour old being immediately due.

The shift and the multiply saturate. A large base against a high attempt overflows a naive multiply and an overflowed deadline lands in the past, which would turn backoff into a busy loop — the exact opposite of what it is for. Sixteen attempts against `i64::MAX / 4` are asserted to stay positive and under the ceiling.

**No jitter and no randomness**, with the reasoning in the module doc: restart equivalence requires that the same observation and the same `now_ms` produce the same directives, and spreading a thundering herd is a benefit that does not apply to one node. A hundred runs of one scenario produce byte-identical directive sequences.

A tick that defers only because of backoff says so, with identifiers only — an operator watching a quiet tick can tell "waiting to retry" from "nothing to do".
