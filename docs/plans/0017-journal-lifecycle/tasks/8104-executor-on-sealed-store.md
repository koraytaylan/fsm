---
id: executor-on-sealed-store
title: "Executor On A Sealed Store"
workstream: "0081"
kind: task
depends_on:
  - replay-doctor-sealed
gated: false
touches:
  - crates/fsm-execute/src/effect.rs
  - crates/fsm-execute/src/dead.rs
  - crates/fsm-execute/tests/sealed_store.rs
status: planned
merged_as: ""
---
# Executor On A Sealed Store

The executor keeps nothing in memory on purpose and re-derives everything by scanning records, which makes it the component a seal is most likely to break quietly.

**Steps:**

1. Fix `crates/fsm-execute/src/effect.rs::fold_before`. It builds the prefix as every record with `seq < record.seq` and calls `fold_with`, which folds **from empty**. On a sealed store that prefix is missing everything below the cut, so the fold produces a state that is wrong rather than a failure that is loud. Fold from the base instead, using the same `fold_from` the store's own open path uses, and take the base from the opened store rather than re-reading it.
2. This is the defect that matters most in the plan, because its symptom is a handler running against **stale or absent arguments** rather than an error. `replay_emits` re-runs the pure entry point against `before` to recover an effect's name and argv; a `before` folded from the wrong origin re-runs it against the wrong context.
3. `crates/fsm-execute/src/dead.rs::dead_letters` scans records for failed `effect_acked`. On a sealed store, acks below the cut are archived and the report silently shrinks. Report the seal alongside the results — the cut sequence and that entries below it are in the archive — so a short report is visibly short rather than apparently empty. `fsm execute --list-dead` exists because a stalled workflow leaves nothing else behind; a version of it that under-reports without saying so is the failure this whole plan is trying not to introduce.
4. `crates/fsm-execute/src/watch.rs::attempt_state` needs **no** change, and confirming that is part of this task rather than an assumption: `7904`'s pin makes it impossible to archive an attempt record for a pending effect, so the derived count is complete by construction. Add the test that proves it and a comment at the scan naming the pin as the reason it is safe, because the next reader will otherwise see an unbounded scan over a store that no longer holds everything.
5. Change **no** executor semantics. Retry counts, backoff, caps, fairness, and every ack stay exactly as plan 0016 defined them. This task makes the derivations correct on a sealed store; it does not adjust what they conclude.
6. Do not add a store-shape check to the scheduler. The scheduler is a pure function of one observation and must stay one; sealing is a fact about where the observation came from, not an input to the decision.

**Tests:**

- `crates/fsm-execute/tests/sealed_store.rs`: an effect emitted **above** the cut resolves, and its argv matches the argv the same effect produced before the store was sealed — byte for byte, which is the assertion that catches a `fold_before` folding from the wrong origin.
- The same for a creation-emitted effect on an instance whose creation record survives the cut.
- An executor run against a sealed store produces the same directives as against the equivalent unsealed store, from the same starting journal.
- A restart mid-retry on a sealed store resumes at the correct attempt number, proving `attempt_state` is complete under the pin.
- `--list-dead` on a sealed store reports the seal and states that earlier entries are archived; the entries it does report are exactly those above the cut.
- `--list-dead` on an unsealed store is byte-identical to before this task.
- An effect whose emitting record was archived cannot occur — assert that the seal that would create it is refused by `7904`'s pin, which is where that case is closed.
- The executor's own goldens for an unsealed store are unchanged.
- The scheduler is unchanged: its unit tests pass untouched, and no sealing concept appears in `sched.rs`.

- **Done when:** `cargo test -p fsm-execute --test sealed_store` passes every case above, `fold_before` folds from the base, the argv-equality assertion holds byte for byte across sealing, `--list-dead` reports the seal rather than under-reporting silently, `attempt_state` is proved complete under the pin and carries the comment saying why, no executor semantics changed, and `cargo test`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo fmt --check` succeed.
