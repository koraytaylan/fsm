---
id: eventless-validation
title: "Eventless Validation"
workstream: "0043"
kind: task
depends_on:
  - eventless-transition-shape
  - validate-module-split
  - macrostep-driver
gated: false
touches:
  - crates/fsm-core/src/spec/validate/reactive.rs
  - crates/fsm-core/src/spec/validate/blocks.rs
  - crates/fsm-core/src/analyze.rs
  - crates/fsm-core/src/error.rs
  - crates/fsm-core/tests/spec_validate.rs
  - crates/fsm-cli/tests/naive_caller/one_step_data.rs
  - crates/fsm-cli/tests/naive_caller/harness.rs
  - crates/fsm-cli/tests/naive_caller/tool_outcomes.rs
  - docs/SPEC.md
status: done
merged_as: ""
---
# Eventless Validation

An eventless transition is the one construct in this engine that can fire without anybody asking, so the rules that keep it honest — no `evt` binding, no terminal source, no silent shadowing — belong at admission where a bad definition never reaches a journal.

**Steps:**

1. In `crates/fsm-core/src/spec/validate/reactive.rs`, implement `def/eventless_evt`: an eventless transition whose `if`, `do`, `emit`, or (later) `raise` expression references the `evt` binding. Report it with the same span precision as any other unknown-binding error, using the existing expression scope machinery rather than a string scan — the scope for an eventless transition simply excludes `evt`.
2. Implement `def/eventless_from_terminal`: an eventless transition whose `from` is a terminal state. `def/terminal_has_transitions` covers the evented case today; this is its twin, and it exists separately so the hint can say the right thing ("a terminal state ends its machine or region; nothing runs after it").
3. Implement `def/eventless_shadowed`: within one `from`'s eventless group in document order, a guardless or literal-`true`-guarded transition followed by any later eventless transition from the same state. This mirrors the existing `def/shadowed` rule for `(from, on)` groups; reuse its message shape so the two read alike.
4. Implement `def/eventless_internal_noop` as a **warning**: an eventless transition with no `to`, no `do`, no `emit`, and no `raise` can only burn a microstep. It is not an error because a definition may legitimately be mid-authoring, but it is always a mistake in a shipped machine.
5. Reuse the existing `def/duplicate_guard` rule for structurally identical guards within one eventless group — do not write a second copy of it.
6. Call these from `validate_reactive`, appending findings in the order listed so the output is stable, and leave the cycle rules to `4304`, which owns the graph analysis in `analyze.rs`.

**Tests:**

- `crates/fsm-core/tests/spec_validate.rs`: an eventless transition whose guard is `evt.amount > "0"` reports `def/eventless_evt` with a span covering `evt`; the same guard on an evented transition is accepted.
- An eventless `do` that assigns from `evt.x` reports `def/eventless_evt`; an eventless `do` that assigns from `ctx.x` is accepted.
- An eventless transition from a terminal state reports `def/eventless_from_terminal`; an evented one from the same state still reports `def/terminal_has_transitions`.
- Two eventless transitions from one state where the first has no `if` reports `def/eventless_shadowed` naming the second; reversing them so the guarded one is first is accepted.
- An eventless transition with no `to`/`do`/`emit` reports the `def/eventless_internal_noop` **warning** and the definition is still accepted.
- Two eventless transitions from one state with structurally identical guards report the existing `def/duplicate_guard`.
- Finding order is stable across two runs of the same malformed definition, and a definition with no eventless transitions produces byte-identical findings to the pre-change behaviour.

- **Done when:** `cargo test -p fsm-core --test spec_validate` covers every rule above including the accepted counter-cases, findings for non-eventless definitions are unchanged, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.

**Landed:** the two refusals — `def/eventless_evt` (an AST walk over the guard, `do`, and `emit` sources with the reference's own span, at `/transitions/{i}/if`, `/do/{j}/value`, and `/emit/{j}/args/{k}`) and `def/eventless_from_terminal` (with `blocks.rs` leaving eventless transitions to it, so `def/terminal_has_transitions` keeps its evented meaning) — live in `validate/reactive.rs`. The two advisory rules do not: `validate` has no warning channel (any finding refuses), and `def/shadowed`, the rule `def/eventless_shadowed` mirrors, is itself an `analyze_all` finding that `machine_create` reports as a warning and never refuses. So `def/eventless_shadowed` lands beside `def/shadowed` in `analyze::shadowing_findings` with the same message shape and severity, and `def/eventless_internal_noop` is `analyze::eventless_noop_findings`, appended last in `analyze_all` so non-reactive analysis output is byte-identical. `def/duplicate_guard` covers the `$always` cell unchanged. Each code landed with its `ALL_CODES` entry, SPEC rows, naive-caller row, and outcome drive, per the `4201` correction; the cycle rules stay with `4304`.
