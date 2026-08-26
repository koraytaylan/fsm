---
id: validate-module-split
title: "Validate Module Split"
workstream: "0042"
kind: chore
depends_on: []
gated: false
touches:
  - crates/fsm-core/src/spec/validate.rs
  - crates/fsm-core/src/spec/validate/mod.rs
  - crates/fsm-core/src/spec/validate/structure.rs
  - crates/fsm-core/src/spec/validate/blocks.rs
  - crates/fsm-core/src/spec/validate/reactive.rs
status: done
merged_as: ""
---
# Validate Module Split

`spec/validate.rs` is 746 lines of one function against a 1000-line ceiling, and three feature workstreams in this plan each need to add validation to it; splitting it into a module directory with **zero behaviour change** is what lets those three own separate files instead of queueing behind one another.

**Steps:**

1. Delete `crates/fsm-core/src/spec/validate.rs` and create `crates/fsm-core/src/spec/validate/mod.rs` in its place. The module path `crate::spec::validate` and the public `pub fn validate(spec: &MachineSpec) -> Result<(), Vec<Finding>>` signature do not change, so `spec/mod.rs` needs no edit and nothing outside this directory moves.
2. Move the name, tree, initial, history, terminal, region, and definition-limit rules verbatim into `structure.rs` as `pub(super)` helpers. Verbatim means verbatim: no renaming, no reordering of pushes, no "while I'm here" tidying.
3. Move `check_block_limits` and the assignment/duplicate-set rules verbatim into `blocks.rs`.
4. Create `reactive.rs` holding only `pub(super) fn validate_reactive(_spec: &MachineSpec, _errs: &mut Vec<Finding>) {}` and a module doc naming the three workstreams that will fill it (0043 eventless, 0044 internal events, 0045 final states) so the next author knows the file is a destination and not dead code.
5. In `mod.rs`, call the moved helpers in **exactly the order the original function ran them**, then `validate_reactive` last. Finding order is observable through `spec_validate.rs`, `analyze_golden.rs`, and the `machine_create` error payload; a reordering here surfaces as a golden diff three tasks later, attributed to the wrong change.
6. Confirm each resulting file is under the 1000-line ceiling and that `reactive.rs` has room for three features' worth of rules.

**Tests:**

- No new test file. The proof obligation is that **every existing test passes unchanged**: `cargo test -p fsm-core` with no fixture edits, in particular `spec_validate.rs`, `spec_parse.rs`, `analyze_golden.rs`, `compile_machine.rs`, and `format_v2_goldens.rs`.
- `cargo test -p fsm-cli --test naive_caller` is green, since its one-step-every-code suite depends on which finding a malformed definition reports first.
- `scripts/oversized-files.sh` passes, and `scripts/oversized-files.sh 500` reports the new files' sizes for the record.
- A diff review step, not an assertion: `git show --stat` for this commit must show only moves plus the new `mod.rs` wiring — if the diff contains a changed string literal or a changed push order, the split is wrong.

- **Done when:** `crates/fsm-core/src/spec/validate/` replaces the single file with four modules, the whole existing suite passes with **no** fixture or golden edits, `scripts/oversized-files.sh` is green, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.

**Landed:** the four modules exist and every existing test passes with no fixture edit. One correction to step 3: the assignment and duplicate-set rules (`def/assign_type`, `def/dup_set`) never lived in `validate.rs` — they are compile-time checks in `spec/compile.rs` — so `blocks.rs` holds what the original file actually had beside `check_block_limits`: the transition, deadline, entry/exit block-limit, and `(from, on)` cell-ceiling phases. `structure.rs` holds regions, name tables, node rules, initial chains, declaration limits, field counts, and enum references. The original function became fifteen phase functions called in its exact order from `mod.rs`; every phase body moved verbatim.
