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

1. Write the inline test module first (all tests live in `jsonrpc.rs`), encoding exactly the inventory under **Tests**.
2. Implement `Incoming { Request, Notification }` and `parse_line(&str) -> Result<Incoming, WireError>` in `crates/fsm-cli/src/mcp/jsonrpc.rs` over `fsm_core::json::parse`: top-level arrays are `WireError::Batch`; a missing `jsonrpc: "2.0"` or `method` is `WireError::Invalid`; `id` is kept as a raw `Value`.
3. Implement `result_response(id, Value)` and `error_response(id, code, message)` builders plus the code constants `-32700`, `-32600`, `-32601`, `-32602`, `-32603`, and `-32002` (server not initialized).

**Tests:**

- `parse_line` rejection cases: malformed JSON → the parse error is surfaced (not swallowed); a top-level array (`[{...}]`) → `WireError::Batch`; `jsonrpc` missing → `Invalid`; `jsonrpc: "1.0"` → `Invalid`; `method` missing → `Invalid`; `method` not a string → `Invalid`.
- Discrimination: an object with an `id` → `Incoming::Request`; without `id` → `Incoming::Notification`; `params` optional in both.
- Id passthrough (raw `Value`, no interpretation): a number-token id (`1`) round-trips into `result_response` byte-identically; a string id; a null id (legal in error responses for parse failures).
- Builders, canonical bytes asserted exactly for one example each: `result_response(1, {})` and `error_response(null, -32700, "...")` — pinning key order, the `jsonrpc: "2.0"` field, and the error object shape `{code, message}`.
- Code constants: each constant equals its spec value (a one-line table test, so a typo is a named failure).

- **Done when:** the inline tests cover every `WireError` variant, both discrimination arms, all three id forms, and both builders byte-exactly under `cargo test -p fsm-cli jsonrpc`, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
