---
id: jsonrpc-envelope
title: "Jsonrpc Envelope"
workstream: "0005"
kind: task
depends_on:
  - json-structural-parser
gated: false
touches:
  - crates/fsm-cli/src/mcp/jsonrpc.rs
status: planned
merged_as: ""
---
# Jsonrpc Envelope

The MCP transport speaks newline-delimited JSON-RPC 2.0; this task lands the message types, the line parser (which rejects batch arrays under every protocol revision), and the response builders the serve loop will dispatch through.

**Steps:**

1. Implement `Incoming { Request, Notification }` and `parse_line(&str) -> Result<Incoming, WireError>` in `crates/fsm-cli/src/mcp/jsonrpc.rs` over `fsm_core::json::parse`: top-level arrays are `WireError::Batch`; a missing `jsonrpc: "2.0"` or `method` is `WireError::Invalid`; `id` is kept as a raw `Value`.
2. Implement `result_response(id, Value)` and `error_response(id, code, message)` builders plus the code constants `-32700`, `-32600`, `-32601`, `-32602`, `-32603`, and `-32002` (server not initialized).
3. Add inline unit tests: malformed JSON, batch array, notification vs request discrimination, id passthrough including string and null ids.

- **Done when:** unit tests in `crates/fsm-cli/src/mcp/jsonrpc.rs` cover every `WireError` variant and both builders, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
