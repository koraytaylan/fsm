---
id: cli-and-mcp-composition-tools
title: "CLI And MCP Composition Tools"
workstream: "0051"
kind: task
depends_on:
  - signal-delivery-operation
gated: false
touches:
  - crates/fsm-cli/src/args.rs
  - crates/fsm-cli/src/cli/instance.rs
  - crates/fsm-cli/src/mcp/tools/mod.rs
  - crates/fsm-cli/src/mcp/tools/handlers/instance.rs
  - crates/fsm-cli/src/mcp/tools/schema_in.rs
  - crates/fsm-cli/src/mcp/tools/schema_out.rs
  - crates/fsm-cli/src/mcp/descriptions.rs
  - crates/fsm-cli/tests/composition_tools.rs
status: planned
merged_as: ""
---
# CLI And MCP Composition Tools

The executor is the default path for composition, not the only one: a session with no executor running must still be able to start a child, collect its result, and deliver a signal, or the feature is invisible to the audience this engine was built for.

**Steps:**

1. Add three CLI subcommands in `crates/fsm-cli/src/args.rs` and `crates/fsm-cli/src/cli/instance.rs`, following the existing `instance ack` shape including `--request-id`: `fsm instance invoke <parent> <slot>`, `fsm instance return <parent> <slot>`, and `fsm instance signal <sender> <signal-id>`.
2. Add three MCP tools in `crates/fsm-cli/src/mcp/tools/mod.rs`: `invocation_start`, `invocation_return`, and `signal_deliver`, each with an input schema in `schema_in.rs` requiring `request_id` like every other mutating tool, and an output schema in `schema_out.rs` carrying the resulting instance view plus the operation's own fields.
3. **Add all three to `MUTATING_TOOLS`.** They reach store mutators, so a read-only server must refuse them with the mode-naming message. The constant is counted from the store code, and forgetting one means a model gets an unexplained failure while composing — the exact failure plan 0008's comment warns about.
4. Write the tool descriptions in `crates/fsm-cli/src/mcp/descriptions.rs` in the existing voice: what it does, when to call it, and what to call next. `invocation_start`'s description must say the executor normally does this; `invocation_return`'s must say it is legal only once the child has settled and that the result arrives as `$done.invoke.<slot>`.
5. Keep `--json` output byte-identical to the MCP structured result for all three, which `crates/fsm-cli/tests/review_regressions/cli_mcp_parity.rs` already enforces workspace-wide.
6. Update the MCP `instructions` string in `crates/fsm-cli/src/mcp/prompts.rs`? **No** — leave prompts to the plan that owns the live surface; adding a sentence here would move a byte-compared transcript for a benefit `5202`'s documentation delivers better. Note that in the task's commit message.

**Tests:**

- `crates/fsm-cli/tests/composition_tools.rs`: each of the three tools performs its operation and returns the documented structured shape.
- Each is refused by a read-only server with a message naming read-only mode; each appears in `MUTATING_TOOLS`.
- Each requires `request_id` and reports the existing argument error without it.
- Replaying a call with the same `request_id` returns `duplicate: true`; different content under the same key is refused.
- CLI/MCP parity: `fsm instance invoke --json` output is byte-identical to `invocation_start`'s `structuredContent` for the same inputs, and likewise for the other two.
- Each tool's structured output validates against its declared output schema through `tool_schemas.rs`.
- The complete tool count in the registry matches the count asserted by the MCP golden suites, updated in this commit.
- `tools/list` stays under the response budget `tools_budget.rs` enforces.

- **Done when:** `cargo test -p fsm-cli --test composition_tools --test tool_schemas --test read_only` passes, all three tools are in `MUTATING_TOOLS`, CLI/MCP parity holds, the tool-count and budget assertions are updated, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
