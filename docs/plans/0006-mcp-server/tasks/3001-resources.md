---
id: resources
title: "Resources"
workstream: "0030"
kind: task
depends_on:
  - negotiation-and-lifecycle
gated: false
touches:
  - crates/fsm-cli/src/mcp/resources.rs
  - docs/EXAMPLES.md
  - crates/fsm-cli/tests/mcp_resources.rs
status: planned
merged_as: ""
---
# Resources

Resources let the model read the normative spec and any stored machine on demand, keeping tool descriptions lean — they are an enhancement, never load-bearing, since errors stay self-contained.

**Steps:**

1. Author `crates/fsm-cli/tests/mcp_resources.rs` first: against a temp store with two machines defined, assert `resources/list` returns `fsm://docs/spec`, `fsm://docs/examples`, and the machines newest-first with the architecture's names and MIME types; `resources/templates/list` returns the single `fsm://machine/{id}` template; `resources/read` on a machine URI returns bytes identical to the stored canonical spec; an unknown URI returns JSON-RPC `-32002` with message "Resource not found".
2. Create `docs/EXAMPLES.md` as a one-paragraph placeholder stating that the worked examples land in plan 0007.
3. Implement `resources/list`, `resources/templates/list`, and `resources/read` in `crates/fsm-cli/src/mcp/resources.rs` per architecture, embedding `docs/SPEC.md` and `docs/EXAMPLES.md` via `include_str!` and serving machine specs as exact stored canonical bytes.

- **Done when:** `cargo test -p fsm-cli --test mcp_resources` passes including the byte-identity assertion for stored specs, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
