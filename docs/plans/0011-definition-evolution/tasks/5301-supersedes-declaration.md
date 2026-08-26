---
id: supersedes-declaration
title: "Supersedes Declaration"
workstream: "0053"
kind: task
depends_on: []
gated: false
touches:
  - crates/fsm-core/src/spec/mod.rs
  - crates/fsm-core/src/spec/parse/decls.rs
  - crates/fsm-core/src/spec/serialize.rs
  - crates/fsm-core/src/spec/validate/reactive.rs
  - crates/fsm-core/src/error.rs
  - crates/fsm-core/tests/supersedes_declaration.rs
status: planned
merged_as: ""
---
# Supersedes Declaration

The mapping lives in the new definition and therefore inside its `machine_id`, which is the decision the whole plan rests on: a reader holding the new hash holds the mapping too, and a migration can never be reinterpreted after the fact.

**Steps:**

1. Add `pub supersedes: Option<SupersedesSpec>` to the machine spec in `crates/fsm-core/src/spec/mod.rs`, with `pub struct SupersedesSpec { pub machine: String, pub states: Vec<(String, String)>, pub context: Vec<(String, String)> }`. Both projections are ordered vectors so document order survives canonical serialization.
2. In `crates/fsm-core/src/spec/parse/decls.rs`, parse the optional top-level `supersedes` key: required string `machine`, optional object `states` (old name → new name), optional object `context` (new variable → expression source). Any other key is `def/unknown_key` at the right pointer.
3. Serialize `supersedes` **into** the canonical form whenever present, and omit it entirely when absent. It is part of `machine_id` by design — write that in a comment, because every other optional key in this workspace is omitted to *protect* identity and this one is included to *establish* it.
4. Add the plan's complete closed set of new codes to `crates/fsm-core/src/error.rs`'s `ALL_CODES`, so no later task edits that file: `def/supersedes_machine_ref`, `def/supersedes_self`, `def/supersedes_unknown_machine`, `def/supersedes_unknown_state`, `def/supersedes_target_not_leaf`, `def/supersedes_target_terminal`, `def/supersedes_region`, `def/supersedes_ctx_unknown`, `def/supersedes_ctx_type`, `def/supersedes_slot`, `req/migrate_settled`, `req/migrate_unmapped`, `req/migrate_not_superseded`, `req/migrate_slot`.
5. In `crates/fsm-core/src/spec/validate/reactive.rs`, implement the two rules decidable from this definition alone: `def/supersedes_machine_ref` (not 64 lowercase hex) and `def/supersedes_self` (the block names this machine's own hash, which is unsatisfiable because the hash covers the block).
6. Allow at most one `supersedes` per definition and note in the module doc that a three-definition chain migrates in two journaled hops. A transitive closure computed by the engine would be a mapping nobody wrote, and this plan refuses to invent one.

**Tests:**

- `crates/fsm-core/tests/supersedes_declaration.rs`: a definition with a valid `supersedes` parses, compiles, and round-trips byte-stably with the block present in the canonical form.
- **Identity:** two definitions identical except for their `states` mapping produce **different** `machine_id` values — assert this directly, it is the plan's founding property.
- A machine with no `supersedes` serializes without the key and keeps its committed `machine_id`; every `examples/` machine is unaffected.
- `machine` of 63 hex, 65 hex, uppercase hex, or a plain name reports `def/supersedes_machine_ref`.
- A block naming the machine's own computed hash reports `def/supersedes_self`.
- An unknown key inside the block reports `def/unknown_key` at the right pointer; a non-object `states` reports `def/shape`.
- An empty `states` map and an empty `context` map are both accepted — a mapping that covers nothing is legal and simply migrates nothing.
- `ALL_CODES` entries are unique, non-empty, and carry one of the four namespace prefixes.

- **Done when:** `cargo test -p fsm-core --test supersedes_declaration` passes every case above including the differing-`machine_id` property, every `examples/` machine keeps its committed identity, the fourteen codes are registered, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
