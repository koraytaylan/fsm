---
id: final-state-shape
title: "Final State Shape"
workstream: "0045"
kind: task
depends_on:
  - raise-block-action
  - eventless-validation
gated: false
touches:
  - crates/fsm-core/src/spec/parse/states.rs
  - crates/fsm-core/src/spec/mod.rs
  - crates/fsm-core/src/spec/serialize.rs
  - crates/fsm-core/src/spec/validate/reactive.rs
  - crates/fsm-core/src/spec/validate/structure.rs
  - crates/fsm-core/src/error.rs
  - crates/fsm-core/tests/final_states.rs
  - crates/fsm-cli/tests/naive_caller/one_step_data.rs
  - crates/fsm-cli/tests/naive_caller/harness.rs
  - crates/fsm-cli/tests/naive_caller/reactive_flows.rs
  - docs/SPEC.md
status: done
merged_as: ""
---
# Final State Shape

`terminal` means "this ends the machine, or this ends the region"; a compound state that finishes its inner workflow needs a different word, and conflating the two would complete every instance the moment any sub-workflow finished.

**Steps:**

1. Add `pub final_state: bool` to the state node struct in `crates/fsm-core/src/spec/mod.rs` (the field is named `final_state` because `final` is a reserved word in Rust; the **JSON key is `final`** and nothing outside the parser sees the Rust name).
2. In `crates/fsm-core/src/spec/parse/states.rs`, extend the state key list from `["name", "terminal", "history", "initial", "entry", "exit", "states"]` to include `"final"`, parsed as a boolean with `def/shape` on any other type. Serialization omits it when `false`, for the canonical-identity reason every other optional key in this plan omits its default.
3. In `crates/fsm-core/src/spec/validate/reactive.rs`, implement the five rules, each with a hint that says which of `final` and `terminal` the author probably wanted:
   - `def/final_not_leaf` — a `final` state has children;
   - `def/final_at_root` — a `final` state's parent is the machine root or a region root, where `terminal` is the correct and already-supported spelling;
   - `def/final_and_terminal` — both flags true on one state;
   - `def/final_has_transitions` — any transition, evented or eventless or deadline, names a `final` state as its `from`;
   - `def/final_is_initial` — a compound's `initial` names its own `final` child, which would finish the compound before it started.
4. Confirm a `final` state is otherwise an ordinary leaf: it may have entry and exit blocks, it may be a transition **target**, history may bind through it, and it counts against `MAX_STATES` like anything else. Add no rules beyond the five.
5. Leave `terminal`'s meaning and every existing `def/terminal_*` rule exactly as they are. This task adds a concept; it does not adjust one.

**Tests:**

- `crates/fsm-core/tests/final_states.rs`: a compound with one `final` leaf child validates and compiles.
- Each of the five rules fires on its own minimal counter-example and reports at the right JSON pointer: children on a final state, a final state directly under the root, a final state under a region root, `final` and `terminal` together, a transition sourced from a final state, and a compound whose `initial` is its final child.
- A `final` state as a transition **target** is accepted; a `final` state with an entry block is accepted and the block runs on entry.
- `"final": "yes"` is `def/shape`.
- A definition with `final` states but no reactive transitions still validates — `final` on its own is legal and simply makes the compound finishable.
- Identity: a machine with no `final` states serializes without the key and keeps its committed `machine_id`.
- Interaction: a `final` state that is also the target of an eventless transition validates, proving `4302`'s `def/eventless_from_terminal` guards the *source* side only.

- **Done when:** `cargo test -p fsm-core --test final_states` covers all five rules plus the accepted counter-cases, every `examples/` machine keeps its committed `machine_id`, `terminal` semantics are untouched, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.

**Landed:** `StateNode.final_state`, parsed from `final`, serialized only when true (`serialize.rs`, where state serialization lives), and the five rules in `validate/reactive.rs` with the hints step 3 asked for. One rule beyond the five, in `structure.rs`'s existing history-shape check: a history pseudostate cannot be `final`, because it is never entered as a leaf and the tree lookups `4502` builds would otherwise treat it as one. The five codes landed with SPEC rows, naive-caller rows and repairs, and outcome drives per the `4201` correction.
