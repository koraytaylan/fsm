---
id: tool-registry-and-schemas
title: "Tool Registry And Schemas"
workstream: "0029"
kind: task
depends_on:
  - negotiation-and-lifecycle
gated: false
touches:
  - crates/fsm-cli/src/mcp/tools.rs
  - crates/fsm-cli/src/mcp/descriptions.rs
  - crates/fsm-cli/tests/tool_schemas.rs
status: planned
merged_as: ""
---
# Tool Registry And Schemas

The thirteen tool schemas are the model's contract with the engine: inputs closed and strict, outputs promising only guaranteed fields, and argument validation that names every offending field — this task lands the registry with placeholder run functions so descriptions and dispatch can proceed in parallel.

**Steps:**

1. Author `crates/fsm-cli/tests/tool_schemas.rs` first: assert exactly 13 tools in the architecture's fixed order, every input schema declaring `type: "object"` with `additionalProperties: false` and the architecture's required fields (`request_id` on every mutating tool), every output schema carrying `additionalProperties: true`, and an accept/reject table for `validate_args`.
2. Implement `ToolSpec`, `registry()`, and the input/output schema constants in `crates/fsm-cli/src/mcp/tools.rs` exactly per the architecture field tables, with `run` stubs returning `internal/unimplemented`; reference description consts from `descriptions.rs` and fill that file with named placeholder consts.
3. Implement `validate_args(schema, args)` supporting the emitted subset (`type`, `required`, `properties`, `enum`, `additionalProperties`), producing `req/args_invalid` with field-by-field expected-vs-got in `details` and the first fix in `hint`.

- **Done when:** `cargo test -p fsm-cli --test tool_schemas` passes with all 13 schemas and the validation table green, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
