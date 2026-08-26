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
  - crates/fsm-core/src/replay/mod.rs
  - crates/fsm-store/src/journal_io/classify.rs
  - crates/fsm-store/src/store/reconstruct.rs
  - crates/fsm-core/tests/macrostep_replay.rs
  - crates/fsm-store/tests/macrostep_history.rs
  - docs/SPEC.md
status: done
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

**Landed:** every replay path in `replay/apply.rs` re-applies under `MACROSTEP_EVAL_TICKS` with the comment step 1 asked for, and calls `verify.rs::verify_microsteps` after the existing trigger-field checks: the journaled array is compared with `record::microsteps_value` of the re-derived reactions entry for entry, an absent key requires that none were derived, and a spurious key on a non-reactive record fails for the mirror reason. The difference is one new `ReplayError::MicrostepMismatch { seq, index }` variant — a new error, not a new health class: `journal_io/classify.rs` maps it onto the existing `JournalHealth::ReplayMismatch` with the field `microsteps[<index>]`, so the recovery posture is the one that already exists. The no-historical-compiler-exception reasoning sits as a comment on `verify_microsteps`, and SPEC's replay paragraph now states both directions and the budget. The suite builds its journals by hand from the pure engine, as the other replay proofs do, and covers the clean fold with every `state_hash` reproduced, the three tamper cases (a flipped `entered` naming seq and index, the key deleted from a cascading record, the key added to a non-reactive one), and a 64-microstep macrostep replaying inside the budget. The legacy fixture and determinism suites the task lists run unchanged in the workspace gate.
 One follow-up surfaced on review: the store's own re-application in `store/reconstruct.rs` — the path `instance_history` and `explain` use to rebuild a record's trace, applied and rejected alike — still ran under `MAX_EVAL_TICKS`, so a deep cascade would have reconstructed without a trace; its four macrostep sites now use `MACROSTEP_EVAL_TICKS` (the enabled-event scan keeps the standard budget), and `crates/fsm-store/tests/macrostep_history.rs` pins a 64-microstep event, deadline, and rejection reconstructing through `explain_seq` and `history_page`. The gate then falsified step 1 as written: `replay::tests::historical_guardless_budget_rejection_still_full_folds` holds a journal whose `event_rejected` was a budget exhaustion at the old single-step budget, which the macrostep budget no longer reproduces. A definition the current compiler accepts cannot exhaust the macrostep budget — it is sized for the trigger, `MAX_MICROSTEPS` reactions, and the closing scan at the compile limit each — so a sealed `internal/budget` cause can only come from such a journal; `replay::replay_sealed_step` therefore re-runs a sealed step under the macrostep budget and, only when the record's details claim that cause and no rejection came back, once more under the historical budget, requiring the usual exact match. It is the budget counterpart of SPEC's historical enabled-event diagnostic rule, stated beside it, and is the one function both fold and the store's history rebuild call. No sealed deadline rejection can carry that cause (a poll visits no event guard), so deadlines get no such path.
