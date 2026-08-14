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

1. Author `crates/fsm-cli/tests/tools_budget.rs` first, encoding exactly the inventory under **Tests**.
2. Replace the placeholder consts in `crates/fsm-cli/src/mcp/descriptions.rs` with the shipped prose for all 13 tools: the two workhorse texts verbatim from architecture, list/get tools at ≤ 40 words, every description opening with when-to-use, cross-referencing the next tool in the flow, stating the schema-inexpressible invariants (decimals as strings, `request_id` retry semantics, `$`-reserved names), and pre-teaching the commonest errors.
3. Add the doc-comment header in `descriptions.rs` recording the eight writing guidelines so future edits stay within them.

**Tests:**

- Budget, in `tools_budget.rs`: the full `tools/list` response's canonical serialization is at most 20,000 bytes — the concrete proxy for the ~5k-token budget.
- Per-description caps: every description non-empty; `machine_create` and `instance_send` each at most 190 words; the four list/get tools (`machine_list`, `machine_get`, `instance_get`, `instance_list`) each at most 40 words.
- Flow cross-references, asserted by substring: `machine_create`'s text contains `instance_create`; `instance_create`'s contains `instance_send`; `instance_send`'s contains `effect_ack` and `enabled_events`.
- Invariant phrases, asserted by substring: `instance_send`'s text contains `request_id`; `machine_create`'s contains `JSON strings` (the decimals rule) and `dry_run`.
- Guideline header: `descriptions.rs` contains the eight-guideline doc comment (presence check; the guidelines' substance is review-level and lives in the architecture).

- **Done when:** `cargo test -p fsm-cli --test tools_budget` passes with the full response at or under 20,000 bytes and every cap and cross-reference green, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
