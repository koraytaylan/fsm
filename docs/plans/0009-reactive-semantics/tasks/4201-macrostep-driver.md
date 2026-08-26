---
id: macrostep-driver
title: "Macrostep Driver"
workstream: "0042"
kind: task
depends_on: []
gated: false
touches:
  - crates/fsm-core/src/step/micro.rs
  - crates/fsm-core/src/step/mod.rs
  - crates/fsm-core/src/step/transition.rs
  - crates/fsm-core/src/step/block.rs
  - crates/fsm-core/src/step/create.rs
  - crates/fsm-core/src/step/deadline.rs
  - crates/fsm-core/src/limits.rs
  - crates/fsm-core/src/trace.rs
  - docs/SPEC.md
  - docs/plans/0009-reactive-semantics/ARCHITECTURE.md
  - crates/fsm-core/tests/macrostep_loop.rs
status: done
merged_as: ""
---
# Macrostep Driver

The macrostep is the one new control flow this plan adds: a pure loop around the existing pipeline primitives that runs a triggered transition's reactions to quiescence, atomically, under a ceiling — and this task lands the loop, its limits, and the complete closed set of error codes the whole plan will raise.

**Steps:**

1. Create `crates/fsm-core/src/step/micro.rs` with the types from architecture §0042: `Macrostep`, `InternalEvent`, `InternalOrigin`, `MicrostepRecord`, and `MicrostepTrigger`. `Macrostep::queue` is a `VecDeque<InternalEvent>` held in a stack frame only — **it is never added to `InstanceState`**, and the module doc says why in one sentence (`fsm.state/2` would move, and every store on earth would need migrating).
2. Implement `pub fn run_to_quiescence(m: &CompiledMachine, t: &Tree, working: &mut InstanceState, macro_state: &mut Macrostep, now_ms: i64, budget: &mut Budget) -> Result<(), Rejection>` with the loop order fixed by §0042: eventless selection first, then the internal queue front, then quiescence. At this task's stage both selection hooks are `pub(crate)` seams returning `Ok(None)` — `4303` fills the eventless one and `4403` fills the queue one — so the loop is complete and testable now and neither later task has to restructure it.
3. Implement the ceiling: at most `MAX_MICROSTEPS` reaction microsteps. Exceeding it returns `Rejection { code: "run/microstep_limit", .. }` whose message names the highest-index microstep and whose hint names the `source_state` and `transition_idx` that fired last. A hint that says only "the machine looped" is a defect in this task.
4. Implement atomicity: `run_to_quiescence` mutates a working clone the caller owns, and any `Rejection` from any microstep propagates out of the whole `step`/`create`/`poll_deadline` call with the caller's state untouched. The `Rejection.trace` retains the microsteps that ran before the failure.
5. Add to `crates/fsm-core/src/limits.rs`: `pub const MAX_MICROSTEPS: u32 = 64;` and `pub const MACROSTEP_EVAL_TICKS: u32 = MAX_EVAL_TICKS * (MAX_MICROSTEPS + 1);`, each with the doc comment §0042 justifies — in particular that admission keeps charging one microstep's worth and the *operation* budget is what widens, so SPEC's "an accepted definition never exhausts a fresh budget" survives.
6. Add the plan's **complete** closed set of new codes to `crates/fsm-core/src/error.rs`'s `ALL_CODES`, in catalogue order, so no later task edits that file: `run/microstep_limit`, `req/event_internal`, `def/eventless_evt`, `def/eventless_from_terminal`, `def/eventless_shadowed`, `def/eventless_cycle`, `def/eventless_cycle_guarded`, `def/eventless_internal_noop`, `def/eventless_depth`, `def/limit_raises`, `def/final_not_leaf`, `def/final_at_root`, `def/final_and_terminal`, `def/final_has_transitions`, `def/final_is_initial`.
7. Add the matching rows to `docs/SPEC.md`'s `## Appendix A — Error codes` and the two limits to `## Appendix B — Limits` in the same commit. `crates/fsm-cli/tests/spec_appendix.rs` asserts every `ALL_CODES` entry appears in the appendix, so splitting these across two tasks would leave the gate red for the length of the plan.
8. Wire `mod micro;` into `crates/fsm-core/src/step/mod.rs` and route **all three** pure entry points through it — `step` (`step/mod.rs`), `create` (`step/create.rs`), and `poll_deadline` (`step/deadline.rs`). Each builds the `Macrostep` after its own trigger has been applied, calls `run_to_quiescence`, and folds the resulting `microsteps`, `effects`, and `monitor_flags` into the `Applied` it returns. Routing only `step` is the easy mistake here and it would leave a machine whose initial state has an eventless exit stuck at creation, which `4601` and `4603` both test for.
9. **Move invariant evaluation out of the transition pipeline and into the driver.** `eval_invariants` is called from `apply_selected_transition` today (`crates/fsm-core/src/step/transition.rs:213`); a reaction microstep runs that same pipeline, so leaving the call there would evaluate invariants once per microstep against intermediate configurations. Give `apply_selected_transition` a flag (or a sibling entry point) that skips the invariant block, call it that way from every microstep including the trigger, and evaluate invariants exactly once at quiescence — on the final ctx and final active configuration, per SPEC §Semantics 8. `monitor_flags` accumulate across microsteps and are de-duplicated in first-failure order.
10. Preserve the existing behaviour exactly for a non-reactive machine: one microstep, invariants evaluated once, the same `Applied` fields, the same trace. The whole existing `step_golden`, `create_chain`, and `record_replay_deadlines` suites must pass untouched, which is the check that step 9's refactor did not change semantics.

**Tests:**

- `crates/fsm-core/tests/macrostep_loop.rs` drives `run_to_quiescence` directly with hand-built seams: a loop that quiesces immediately (no eventless, empty queue) performs zero reaction microsteps and returns an empty `microsteps` vector.
- A stub eventless seam that returns a selection `N` times then `None` produces exactly `N` reaction microsteps with `index` 1..=N.
- A stub that never returns `None` rejects with `run/microstep_limit` after exactly `MAX_MICROSTEPS` reaction microsteps, and the rejection's hint names a `source_state` and a `transition_idx`.
- Atomicity: a seam whose third microstep returns a `Rejection` leaves the caller's `InstanceState` byte-identical to its pre-call value, and the rejection's trace holds the two microsteps that did run.
- Ordering: with both seams live, an eventless selection available and a non-empty queue, the eventless one is taken first; with no eventless selection available the queue front is taken.
- Budget: `MACROSTEP_EVAL_TICKS == MAX_EVAL_TICKS * 65`, and a macrostep of `MAX_MICROSTEPS + 1` microsteps each costing `MAX_EVAL_TICKS` does not exhaust it.
- `InstanceState` has exactly its six existing fields — a compile-time assertion or an explicit field-count test, so a later task cannot quietly persist the queue.
- **All three entry points are wired:** `step`, `create`, and `poll_deadline` each return an `Applied` whose `microsteps` reflect the driver, verified with a stub seam that reports one reaction microstep for each.
- Invariants are evaluated **once** per macrostep: a stub seam producing three reaction microsteps evaluates the machine's invariants exactly once, asserted with a counting invariant expression or an evaluation counter.
- An enforce invariant that would fail on an intermediate configuration but passes on the final one **does not** reject — the case that justifies moving the call.
- A monitor invariant failing in two different microsteps appears once in `monitor_flags`, in first-failure order.
- Regression: `step_golden`, `create_chain`, `select_golden`, and `record_replay_deadlines` pass with no fixture edits, proving the invariant refactor changed no semantics for non-reactive machines.
- `ALL_CODES` entries are unique, non-empty, and each carries one of the four namespace prefixes; `cargo test -p fsm-cli --test spec_appendix` passes with the appendix rows added.

- **Done when:** `cargo test -p fsm-core --test macrostep_loop` passes every case above, `step`/`create`/`poll_deadline` all run macrosteps, invariants are evaluated exactly once per macrostep at quiescence, `cargo test -p fsm-cli --test spec_appendix` is green with the two new limits documented (the codes land per producing task; see *Landed*), the existing `step_golden`, `create_chain`, `select_golden`, and `record_golden` suites pass with no fixture edits, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.

**Landed:** the driver, the ceiling, atomicity, the budget constants, and all three entry points routed through `run_to_quiescence`, with `step_with` / `create_with` / `poll_deadline_with` taking a `ReactionSelector` so the loop is testable before any reactive shape exists. Corrections the implementation forced, each also written into ARCHITECTURE §0042:

- **Step 6 did not land here.** Two naive-caller gates (`tool_outcomes::all_codes_hygiene`, `one_step_every_non_infra_code`) fail for any `ALL_CODES` entry that no real tool outcome produces, so cataloguing fifteen codes the engine cannot yet raise would have left `cargo test` red until `4501`. Each code lands in `error.rs`, SPEC Appendix A, and the naive-caller rows in the task that first produces it; `run/microstep_limit` lands with `4303`. Appendix B's two limit rows landed here as step 7 asked.
- **`MACROSTEP_EVAL_TICKS` is `MAX_EVAL_TICKS * (MAX_MICROSTEPS + 2)`**, and a discarded internal event counts against `MAX_MICROSTEPS`. The `+ 1` draft left the closing quiescence scan uncounted and let unhandled events buy unbounded scans, both breaking SPEC's never-exhausts guarantee; the budget test pins `4096 × 66`.
- **`trace.rs` is in the footprint.** The per-microstep candidates and pipelines need a home the later tasks (`4403` fills discards, `4701` renders) can share without editing `micro.rs` and `trace.rs` at once; the one `MicrostepTrace` struct replaces the draft's `MicrostepRecord` + `MicrostepTrace` pair.
- **Invariants evaluated once means monitor flags are one list**, not an accumulation; the "failing in two microsteps appears once" case is pinned as a monitor that fails on the intermediate and final configurations and is reported once.
- **Settlement is deferred** so a non-reactive macrostep keeps SPEC's invariants-before-schedules order byte for byte (a deadline-schedule failure's trace carries the invariant traces, and replay checks that trace).
- The "both seams live, eventless taken first" test needs a non-empty queue, which nothing can fill until `4402`; it moves to `4403`'s `internal_queue.rs`. This task pins the call order instead: the eventless seam is consulted on every iteration and the queue seam never while the queue is empty.
