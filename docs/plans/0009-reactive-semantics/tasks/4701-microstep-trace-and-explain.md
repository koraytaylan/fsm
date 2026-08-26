---
id: microstep-trace-and-explain
title: "Microstep Trace And Explain"
workstream: "0047"
kind: task
depends_on:
  - internal-queue-semantics
gated: false
touches:
  - crates/fsm-core/src/trace.rs
  - crates/fsm-store/src/store/view.rs
  - crates/fsm-cli/src/render.rs
  - crates/fsm-core/tests/trace_render.rs
status: planned
merged_as: ""
---
# Microstep Trace And Explain

A macrostep that cannot be explained is a macrostep nobody will trust with a workflow; `explain` is this engine's best affordance and it has to show the cascade, not just its result.

**Steps:**

1. Add `pub microsteps: Vec<MicrostepTrace>` to `DecisionTrace` in `crates/fsm-core/src/trace.rs`, where `MicrostepTrace` carries its own `trigger`, `candidates: Vec<LevelTrace>`, and `pipeline: Vec<BlockTrace>`. The three existing fields keep describing microstep 0 and keep their names — a rename here would move every trace golden in the workspace for no gain.
2. Emit `microsteps` from `DecisionTrace::to_value` **only when non-empty**, matching the record rule from `4601` for the same canonical-bytes reason.
3. Record a discarded internal event in the trace as an `internal_unhandled` entry naming the event, so the audit trail shows an event the machine raised and nothing handled — that is a design smell worth surfacing, not noise worth hiding.
4. In `crates/fsm-store/src/store/view.rs`, extend `explain_seq` to render each microstep as its own section under the trigger's, carrying the microstep index, its trigger, and its candidate and block traces. Preserve the existing top-level shape exactly so a non-reactive explain is byte-identical.
5. In `crates/fsm-cli/src/render.rs`, add the human line form `→ microstep 2 (internal $done.state.approve): review → done`, and for an eventless microstep `→ microstep 1 (eventless): route → approve`. Keep the existing indentation conventions.
6. Confirm a `Rejection`'s trace carries the microsteps that ran before the failure, per `4201`'s atomicity rule — a rejection that shows only the failing microstep is a worse record than the one shipped today.

**Tests:**

- `crates/fsm-core/tests/trace_render.rs`: existing goldens for non-reactive machines are **unchanged**, and `to_value` emits no `microsteps` key for them.
- A reactive macrostep's trace carries one entry per reaction microstep with its own candidates and pipeline.
- A discarded internal event appears as `internal_unhandled` naming the event.
- A rejected macrostep's trace holds the microsteps that ran before the failure, in order.
- `explain_seq` on a reactive record renders every microstep; on a non-reactive record its output is byte-identical to the pre-change golden.
- The human renderer produces the two documented line forms, and a 64-microstep macrostep renders without truncation or a panic.
- Round trip: `to_value` output re-parsed and re-rendered is stable.

- **Done when:** `cargo test -p fsm-core --test trace_render` passes with unchanged non-reactive goldens and new reactive cases, `explain_seq` renders cascades, the CLI shows both microstep line forms, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
