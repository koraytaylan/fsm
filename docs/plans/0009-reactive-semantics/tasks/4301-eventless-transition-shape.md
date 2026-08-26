---
id: eventless-transition-shape
title: "Eventless Transition Shape"
workstream: "0043"
kind: task
depends_on: []
gated: false
touches:
  - crates/fsm-core/src/spec/mod.rs
  - crates/fsm-core/src/spec/parse/transitions.rs
  - crates/fsm-core/src/spec/serialize.rs
  - crates/fsm-core/src/spec/compile.rs
  - crates/fsm-core/tests/spec_parse.rs
status: planned
merged_as: ""
---
# Eventless Transition Shape

Making `on` optional is the smallest possible change that lets a machine express "when this guard becomes true, move on" — and the whole risk is in identity, because `machine_id` hashes the canonical definition and a stray serialized key would rename every machine that does not use the feature.

**Steps:**

1. Change `TransitionSpec.on` in `crates/fsm-core/src/spec/mod.rs` from `String` to `Option<String>`, and add `pub const ALWAYS_KEY: &str = "$always";` beside it with a doc comment noting that `def/reserved_ident` already forbids `$`-prefixed declared names, so no user event can collide.
2. In `crates/fsm-core/src/spec/parse/transitions.rs`, drop `on` from the required-key set while keeping it in the `check_keys` allow-list. An absent `on` parses to `None`; an explicit `"on": null` is `def/shape` with a hint saying to omit the key rather than null it, because an explicit null is a typo and not an intention.
3. In `crates/fsm-core/src/spec/serialize.rs`, omit the `on` key entirely when it is `None`. This is the identity-preserving half of the task: emitting `"on": null` — or `"on": "$always"` — for an eventless transition is fine, but emitting *anything at all* for a transition that has an event would change canonical bytes, so the guard is on the `None` branch only.
4. In `crates/fsm-core/src/spec/compile.rs`, key an eventless transition into `CompiledMachine.transitions_by` as `(from, ALWAYS_KEY.to_string())`. Document order within the cell is the existing `Vec<usize>` and is unchanged. `MAX_TRANSITIONS_PER_CELL` applies to the `$always` cell like any other.
5. Leave `def/limit_eval` admission accounting alone except for one addition the architecture requires: an eventless transition with an omitted `if` charges one tick for its implicit `true`, exactly as an event transition with an omitted guard does. Do not multiply anything here — the operation budget is what widens, in `4201`.

**Tests:**

- `crates/fsm-core/tests/spec_parse.rs`: a transition with no `on` parses with `on: None`; `"on": null` is `def/shape` at the right JSON pointer; `"on": ""` remains whatever the current empty-name rule reports, unchanged.
- Round trip: parse → serialize → parse of a definition containing both evented and eventless transitions is byte-stable, and the serialized form of the eventless one has no `on` key.
- **Identity invariance:** for every machine in `examples/`, the `machine_id` computed after this change equals the value committed in the existing goldens. This is the test that catches a serializer that started emitting a key.
- Compile: an eventless transition lands under `(from, "$always")`; an evented one is unaffected; a machine with 33 eventless transitions from one state reports `def/limit_cell`.
- A declared event named `$always` is still refused by the existing `def/reserved_ident` rule — assert it, so the collision argument in the doc comment is load-bearing rather than decorative.

- **Done when:** `cargo test -p fsm-core --test spec_parse` covers the optional-`on`, null-rejection, round-trip, and identity-invariance cases, every `examples/` machine keeps its committed `machine_id`, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
