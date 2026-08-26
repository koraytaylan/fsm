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
status: done
merged_as: ""
---
# Tool Registry And Schemas

The thirteen tool schemas are the model's contract with the engine: inputs closed and strict, outputs promising only guaranteed fields, and argument validation that names every offending field — this task lands the registry with placeholder run functions so descriptions and dispatch can proceed in parallel.

**Steps:**

1. Author `crates/fsm-cli/tests/tool_schemas.rs` first, encoding exactly the inventory under **Tests**.
2. Implement `ToolSpec`, `registry()`, and the input/output schema constants in `crates/fsm-cli/src/mcp/tools.rs` exactly per the architecture field tables, with `run` stubs returning `internal/unimplemented`; reference description consts from `descriptions.rs` and fill that file with named placeholder consts.
3. Implement `validate_args(schema, args)` supporting the emitted subset (`type`, `required`, `properties`, `enum`, `additionalProperties`), producing `req/args_invalid` with field-by-field expected-vs-got in `details` and the first fix in `hint`.

**Tests:**

- Registry shape in `tool_schemas.rs`: exactly 13 tools in the architecture's fixed order — `machine_create`, `machine_list`, `machine_get`, `machine_analyze`, `machine_diagram`, `instance_create`, `instance_send`, `effect_ack`, `instance_cancel`, `instance_get`, `instance_list`, `instance_history`, `simulate` — order asserted, not just membership.
- Input schemas: every one declares `type: "object"` and `additionalProperties: false`; the per-tool `required` lists match the architecture tables exactly (in particular `request_id` required on all four mutating tools: `instance_create`, `instance_send`, `effect_ack`, `instance_cancel`; `spec` required on `machine_create`; `instance_id` on every instance tool).
- Output schemas: every one carries `additionalProperties: true`; and every output schema is itself accepted by our own schema checker — the same validator run in both directions, so an unsupported construct in an output schema is a test failure, not a silent over-promise.
- `validate_args` accept row per tool: a minimal valid call for each of the 13 accepted.
- `validate_args` reject rows, each asserting `req/args_invalid` with the offending field named in `details` and a fix in `hint`: `instance_send` missing `request_id`; `machine_list` with `limit` as a string (expected-vs-got types in `details`); an unknown extra field under `additionalProperties: false`; `machine_diagram` with `format: "png"` → the `enum` values listed in `details`.
- Stub wiring: calling one registered tool's `run` stub returns `internal/unimplemented` — proving the registry is executable before dispatch lands.

- **Done when:** `cargo test -p fsm-cli --test tool_schemas` passes with all 13 schemas, the two-direction schema check, and the validation table green, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
