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
status: done
merged_as: ""
---
# Resources

Resources let the model read the normative spec and any stored machine on demand, keeping tool descriptions lean — they are an enhancement, never load-bearing, since errors stay self-contained.

**Steps:**

1. Author `crates/fsm-cli/tests/mcp_resources.rs` first, encoding exactly the inventory under **Tests**.
2. Create `docs/EXAMPLES.md` as a one-paragraph placeholder stating that the worked examples land in plan 0007.
3. Implement `resources/list`, `resources/templates/list`, and `resources/read` in `crates/fsm-cli/src/mcp/resources.rs` per architecture, embedding `docs/SPEC.md` and `docs/EXAMPLES.md` via `include_str!` and serving machine specs as exact stored canonical bytes.

**Tests:**

- `mcp_resources.rs`, against a temp store with two machines defined — `resources/list`: contains `fsm://docs/spec` and `fsm://docs/examples` with MIME `text/markdown`, plus both machines newest-first with MIME `application/json` and the architecture's name fields.
- `resources/templates/list`: exactly the single `fsm://machine/{id}` template.
- `resources/read` on `fsm://docs/spec` → bytes equal the embedded `docs/SPEC.md`; on `fsm://docs/examples` → bytes equal the placeholder (so plan 0007's replacement is observable through this same test); on a machine URI → bytes identical to the stored canonical spec (byte-compared against the store's canonical bytes for that id).
- Unknown URI → JSON-RPC error `-32002` with message `Resource not found` (the documented numeric collision with the initialize gate, distinguished by message).
- Capabilities coherence: the advertised resources capability is `{subscribe: false, listChanged: false}` — pinned here so a future capability change is a deliberate edit.

- **Done when:** `cargo test -p fsm-cli --test mcp_resources` passes including the byte-identity assertion for stored specs, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
