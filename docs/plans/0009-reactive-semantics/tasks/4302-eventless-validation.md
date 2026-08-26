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
  - crates/fsm-core/tests/spec_validate.rs
status: planned
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
