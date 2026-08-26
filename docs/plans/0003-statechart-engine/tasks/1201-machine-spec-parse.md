---
id: machine-spec-parse
title: "Machine Spec Parse"
workstream: "0012"
kind: task
depends_on: []
gated: false
touches:
  - crates/fsm-core/src/lib.rs
  - crates/fsm-core/src/spec.rs
  - crates/fsm-core/src/tree.rs
  - crates/fsm-core/src/machine.rs
  - crates/fsm-core/src/step.rs
  - crates/fsm-core/src/trace.rs
  - crates/fsm-core/src/analyze.rs
  - crates/fsm-core/src/simulate.rs
  - crates/fsm-core/src/hashes.rs
  - crates/fsm-core/tests/spec_parse.rs
  - "crates/fsm-core/tests/fixtures/machines/**"
status: done
merged_as: ""
---
# Machine Spec Parse

The `fsm.machine/1` JSON format — recursive state tree, flat transition array, entry/exit blocks, history pseudo-children — parses into a typed model with JSON-Pointer error paths; as the plan's first task it also wires all engine modules into `lib.rs` so no later task touches it again.

**Steps:**

1. Add `pub mod tree; pub mod spec; pub mod machine; pub mod step; pub mod trace; pub mod analyze; pub mod simulate; pub mod hashes;` to `crates/fsm-core/src/lib.rs` and create the corresponding empty stub files.
2. Commit the reference fixture `crates/fsm-core/tests/fixtures/machines/case_review.json` first, verbatim from architecture, plus the malformed variants and `crates/fsm-core/tests/spec_parse.rs` encoding exactly the inventory under **Tests**.
3. Implement the typed model (`MachineSpec`, `StateNode`, `Block`, `TransitionSpec`, `InvariantSpec`, …) and `parse_machine(v: &Value) -> Result<MachineSpec, Vec<Finding>>` in `crates/fsm-core/src/spec.rs` with `def/unknown_key`, `def/shape`, and `req/number_token` errors carrying JSON-Pointer paths.

**Tests:**

- `case_review.json` parses, with the model shape asserted in `spec_parse.rs`: top-level state order `[intake, in_review, suspended, approved, rejected]`; `in_review`'s children in document order `[resume_review (deep history), docs_review, risk_review]`; `in_review.entry` has one set and one emit, `in_review.exit` one set, `risk_review.entry` one set; the transitions array has exactly 8 entries in document order with `note_added`'s entry carrying no `to` (internal); the invariant parses with `mode: Enforce`; `on_unhandled` is `reject`.
- Malformed variants, one file per case, each asserted for code *and* exact JSON-Pointer path: an unknown top-level key → `def/unknown_key` at `/badkey`; `states` as an object → `def/shape` at `/states`; a `format` other than `"fsm.machine/1"` → `def/shape` at `/format`; a transition missing `on` → `def/shape` at `/transitions/0`; a raw JSON number in a context `init` → `req/number_token` at `/context/0/init`; a raw number inside a nested entry-block emit argument → `req/number_token` with the full nested pointer (e.g. `/states/1/entry/emit/0/args/total`), pinning pointer construction through the tree.
- `spec_parse.rs` mechanics: iterates the malformed-fixture directory and fails the run if any fixture file yields no finding or an unexpected code — no silently passing fixtures.
- Inline: the parsed model of `case_review.json` re-serializes (via the model's `to_value`) to a document that re-parses to an equal model (model round-trip; byte-level canonicalization is plan 0001's concern, not repeated here).

- **Done when:** `case_review.json` parses into the asserted model shape and every malformed fixture yields its expected `def/*` or `req/*` code and JSON-Pointer path under `cargo test -p fsm-core --test spec_parse`, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
