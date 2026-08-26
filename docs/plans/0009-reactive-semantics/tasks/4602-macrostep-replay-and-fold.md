---
id: macrostep-replay-and-fold
title: "Macrostep Replay And Fold"
workstream: "0046"
kind: task
depends_on:
  - macrostep-record-shape
gated: false
touches:
  - crates/fsm-core/src/replay/apply.rs
  - crates/fsm-core/src/replay/verify.rs
  - crates/fsm-core/tests/macrostep_replay.rs
status: planned
merged_as: ""
---
# Macrostep Replay And Fold

A journaled `microsteps` array is only worth writing if replay re-derives it and compares — otherwise it is decoration in a tamper-evident chain, which is worse than nothing.

**Steps:**

1. In `crates/fsm-core/src/replay/apply.rs`, re-apply every record through the same macrostep entry points a live write uses, with `Budget::new(MACROSTEP_EVAL_TICKS)`. Using the standard budget here would fail replay of a legitimately deep macrostep that the original write accepted, and that failure would surface as `StateHashMismatch` on a healthy store — the worst diagnosis this system can produce.
2. In `crates/fsm-core/src/replay/verify.rs`, verify the microsteps as a claim in both directions: when the record carries `microsteps`, the re-derived sequence must match it entry for entry (index, trigger, event, source_state, transition_idx, exited, entered); when the record **omits** the key, replay must derive **zero** reaction microsteps. The second half is what makes an omitted key a checked assertion rather than an absence of evidence.
3. Report a mismatch through the existing verification failure path with a message naming the record `seq` and the first differing microstep index. Do not invent a new health class — a divergent macrostep is a `StateHashMismatch`-class problem and the existing recovery posture ("refuse; no repair") is correct for it.
4. Add no historical-compiler exception. The reactive features are opt-in syntax, so a definition written before this plan cannot acquire an eventless transition, a `raise`, or a `final` state on recompilation; SPEC's existing legacy-compiler rule stays scoped to the history-shape bug it was written for. Write that reasoning as a comment where a future reader would otherwise be tempted.
5. Confirm the fold path's duplicate-`request_id` detection, `state_root` recomputation, and record-hash verification are untouched.

**Tests:**

- `crates/fsm-core/tests/macrostep_replay.rs`: a journal containing reactive records folds clean and reproduces every `state_hash`.
- Tamper detection: flipping one `entered` entry inside a journaled `microsteps` array makes verification fail, naming the record seq and the microstep index.
- Tamper detection the other way: **deleting** the `microsteps` key from a record whose machine does cascade makes verification fail, because replay derives reaction microsteps where the record claims none.
- Adding a spurious `microsteps` key to a non-reactive record fails verification for the mirror reason.
- Every committed legacy journal fixture (`crates/fsm-core/tests/format_v2_goldens.rs`, `crates/fsm-store/tests/legacy_snapshot_migration.rs`, and the historical-genesis fixtures) folds unchanged with an unchanged final `state_root`.
- A macrostep of 64 microsteps replays under `MACROSTEP_EVAL_TICKS` without exhausting the budget.
- `replay_determinism.rs` and `crates/fsm-cli/tests/replay_determinism.rs` pass unchanged.

- **Done when:** `cargo test -p fsm-core --test macrostep_replay` proves both directions of the microstep claim and all four tamper cases, every legacy fixture folds unchanged, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
