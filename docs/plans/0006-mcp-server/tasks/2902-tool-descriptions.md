---
id: tool-descriptions
title: "Tool Descriptions"
workstream: "0029"
kind: task
depends_on:
  - tool-registry-and-schemas
gated: false
touches:
  - crates/fsm-cli/src/mcp/descriptions.rs
  - crates/fsm-cli/tests/tools_budget.rs
status: planned
merged_as: ""
---
# Tool Descriptions

Tool descriptions are the model's entire manual and a permanent token cost in every conversation, so the prose follows strict writing guidelines and lives under a hard, test-enforced budget.

**Steps:**

1. Author `crates/fsm-cli/tests/tools_budget.rs` first: build the full `tools/list` response and assert its canonical serialization is at most 20,000 bytes, that every description is non-empty, and that the `machine_create` and `instance_send` descriptions are each at most 190 words.
2. Replace the placeholder consts in `crates/fsm-cli/src/mcp/descriptions.rs` with the shipped prose for all 13 tools: the two workhorse texts verbatim from architecture, list/get tools at ≤ 40 words, every description opening with when-to-use, cross-referencing the next tool in the flow, stating the schema-inexpressible invariants (decimals as strings, `request_id` retry semantics, `$`-reserved names), and pre-teaching the commonest errors.
3. Add the doc-comment header in `descriptions.rs` recording the eight writing guidelines so future edits stay within them.

- **Done when:** `cargo test -p fsm-cli --test tools_budget` passes with the full response at or under 20,000 bytes, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
