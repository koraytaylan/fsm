---
id: eventless-selection
title: "Eventless Selection"
workstream: "0043"
kind: task
depends_on:
  - macrostep-driver
  - eventless-transition-shape
gated: false
touches:
  - crates/fsm-core/src/step/mod.rs
  - crates/fsm-core/src/step/micro.rs
  - crates/fsm-core/src/step/guard.rs
  - crates/fsm-core/src/step/create.rs
  - crates/fsm-core/src/error.rs
  - crates/fsm-core/tests/eventless_selection.rs
  - crates/fsm-cli/tests/naive_caller/one_step_every_non_infra_code.rs
  - crates/fsm-cli/tests/naive_caller/tool_outcomes.rs
  - docs/SPEC.md
status: done
merged_as: ""
---
# Eventless Selection

Eventless selection is the existing candidate scan with one substitution — the cell key `$always` instead of an event name — and one asymmetry that is the likeliest thing in this plan to get wrong: all-guards-false is quiescence, not `run/not_enabled`.

**Steps:**

1. In `crates/fsm-core/src/step/mod.rs`, factor the existing candidate scan out of `step` into `fn scan_candidates(m, t, config, key: &str, ...) -> Option<Winner>` so the event path and the eventless path share one implementation. The refactor must not change the event path's behaviour: same innermost-first `chain(leaf)` walk, same region document order, same skip of regions whose active leaf is terminal, same single global winner.
2. Implement `pub(crate) fn select_eventless(m, t, config, ctx, budget) -> Result<Option<SelectedTransition>, Rejection>` calling `scan_candidates` with `ALWAYS_KEY`, evaluating guards against the current `ctx` with **no `evt` in scope**, first-true-guard-wins in document order.
3. Fix the asymmetry explicitly and comment it where the code branches: for an event, no candidates is `run/unhandled` and all-false is `run/not_enabled`; for an eventless scan, **both** mean "no eventless transition is selected", which the caller reads as quiescence with respect to eventless transitions. Neither is an error, and neither may be reported as one.
4. Keep guard-error handling identical: a guard that fails to evaluate is `run/guard_error` and is **never** treated as false, in the eventless path exactly as in the event path. This is the one place where the two paths must not diverge.
5. Replace `4201`'s eventless seam in `run_to_quiescence` with a call to `select_eventless`, and apply a selected transition through the existing `apply_selected_transition` in `crates/fsm-core/src/step/transition.rs` with no `evt` binding — the same pipeline, the same history capture from the pre-microstep configuration, the same deadline rescheduling from the macrostep's single `now_ms`.
6. Confirm `to`-absent still means internal (no exit/entry) for an eventless transition, and that an external eventless self-transition uses `dom = parent(from)` like any other, by pointing both paths at the same code rather than special-casing.

**Tests:**

- `crates/fsm-core/tests/eventless_selection.rs`: a machine whose entry state has one guardless eventless transition advances two states on a single `create`, and the resulting `microsteps` has one entry with `trigger: Eventless`.
- All-guards-false quiesces: the instance settles on the state with the eventless transitions and the outcome is `Applied`, **not** a `run/not_enabled` rejection.
- No eventless candidates at all quiesces the same way, and is not `run/unhandled`.
- A guard that errors during an eventless scan rejects the whole macrostep with `run/guard_error`, and the caller's state is untouched.
- Innermost-first: a child and its ancestor both have eventless transitions; the child's fires.
- Parallel: two regions both have eventless transitions enabled; exactly one fires per microstep, chosen by region document order, and the next microstep fires the other.
- A region on a terminal leaf is skipped by the eventless scan.
- Chaining: three eventless transitions in sequence produce three reaction microsteps in one macrostep, and `deadlines_after` reflects only the final configuration — a deadline on a state entered and exited mid-macrostep is absent.
- Event-path regression: the whole existing `step_golden.rs` and `select_golden.rs` suites pass unchanged after the `scan_candidates` refactor.

- **Done when:** `cargo test -p fsm-core --test eventless_selection` passes every case above, `step_golden` and `select_golden` are unchanged, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.

**Landed:** `scan_candidates` in `step/mod.rs` is the one scan for both paths, returning the winner and the candidate trace; `EngineSelector::select_eventless` (in `micro.rs`, which `4201` created for exactly this fill-in) calls it with `ALWAYS_KEY` and maps "no winner" to quiescence whether or not candidates existed. `guard.rs` joined the footprint so an eventless guard binds no `evt` at all rather than an empty object. `run/microstep_limit` landed here rather than in `4201`, per that task's correction, with `create` recording it as the `cause` of its `run/create_failed`; the naive-caller flow sends an event into a guarded eventless self-loop, reads the hint, and redefines the machine with a guard that becomes false.
