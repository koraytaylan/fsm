---
id: reactive-inertness-proof
title: "Reactive Inertness Proof"
workstream: "0046"
kind: task
depends_on:
  - macrostep-replay-and-fold
  - done-region-events
gated: false
touches:
  - crates/fsm-core/tests/reactive_inertness.rs
  - crates/fsm-core/tests/fixtures/inertness/drives.json
  - crates/fsm-cli/tests/reactive_inertness_store.rs
  - crates/fsm-cli/tests/fixtures/inertness/preplan_session.journal
  - crates/fsm-cli/tests/fixtures/inertness/preplan_session.meta.json
status: done
merged_as: ""
---
# Reactive Inertness Proof

The plan's central promise is that a definition using none of these features produces byte-identical bytes; this task is that promise written as a suite, and if it fails the plan has broken its own contract and the failure is not negotiable.

**Steps:**

1. Create `crates/fsm-core/tests/reactive_inertness.rs`. For every machine in `examples/` and every fixture machine used by `step_golden.rs`, `record_golden.rs`, `select_golden.rs`, `shadowing_golden.rs`, and `hashes_golden.rs`, assert the machine is non-reactive (no eventless transition, no `raise`, no `final` state) and that its `machine_id` equals the value committed in the goldens.
2. For each, drive a representative create → step → poll sequence and assert every produced record body has **no** `microsteps` key, every `state_hash` matches the committed golden, and the `DecisionTrace` serialization has no `microsteps` key.
3. Assert budget inertness: the ticks consumed by a one-microstep macrostep equal the ticks the same operation consumed before this plan, using the existing budget accounting. A macrostep that quiesces immediately must not pay for the loop it did not run.
4. Create `crates/fsm-cli/tests/reactive_inertness_store.rs` for the store-level leg: build a store from a committed pre-plan journal fixture, fold it, assert `Ok` and an unchanged final `state_root`; then append new records for the same non-reactive machine and assert the appended record hashes match hashes computed by the pre-plan build (committed as a fixture alongside).
5. Assert the genesis `limits` block is byte-identical to the pre-plan value, and that neither `MAX_MICROSTEPS` nor `MAX_RAISES_PER_BLOCK` appears in it.
6. Assert `fsm.state/2` is still the state format emitted by every write path, and that no new format version string exists anywhere in the workspace.
7. Write the file header to say what this suite is for in two sentences, so a future author who "simplifies" it understands they are deleting the plan's compatibility contract.

**Tests:**

- Every assertion above is itself the test inventory; there is no implementation to cover separately.
- Negative control: a deliberately reactive fixture machine defined inside the test **does** produce a `microsteps` key, proving the suite would notice if the key were never emitted at all.
- Negative control: temporarily forcing the emit-key guard to unconditional (behind a `#[cfg(test)]` helper, not a real code path) makes the inertness assertions fail — verify by construction during development, then assert the guard's real behaviour.
- The suite runs on all three CI operating systems without a path or line-ending dependency in any fixture.

- **Done when:** `cargo test -p fsm-core --test reactive_inertness` and `cargo test -p fsm-cli --test reactive_inertness_store` both pass, every `examples/` machine keeps its committed `machine_id` and `state_hash` values, a pre-plan journal fixture folds to an unchanged `state_root`, the genesis limits block is unmoved, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.

**Landed:** both goldens were written by the pre-plan build itself — a worktree at commit `c5b5620` ran the same test files with `FSM_REGEN_FIXTURES=1` — rather than by the current build, so "byte-identical to before the plan" is checked against the plan's actual baseline. `reactive_inertness.rs` walks each committed machine's document for reactive syntax (an `on`-less transition, `raise`, `final`), checks its id against `identity.jsonl`, and drives it — create, every declared event with a zero payload, a far-future deadline poll — comparing machine id, state hashes, decision traces, tick counts, and the genesis limits block with `fixtures/inertness/drives.json`; it also pins `fsm.state/2` as the only current state format named anywhere in the workspace and carries the reactive negative control. The "temporarily force the guard unconditional" control from the task's test list was exercised the honest way: the suite failed for real on first run, because the driver raised `$done.region.<region>` for a plain parallel machine and recorded the discard as `internal_unhandled` — the contract breach the task exists to catch — and that became the preceding fix (`a generated event nobody handles is never raised`). `reactive_inertness_store.rs` lays the pre-plan journal into a data directory, opens it, checks the folded `last_hash` and state root the pre-plan build recorded, appends the same continuation through today's store, and requires the resulting journal to be the pre-plan bytes, chain-verified. Creation's tick cost is not in the golden: `create` sizes its own budget. The pre-plan generator for the store leg lives outside the tree (it only runs in a pre-plan checkout); the core suite is its own generator.
