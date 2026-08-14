---
id: negotiation-and-lifecycle
title: "Negotiation And Lifecycle"
workstream: "0028"
kind: task
depends_on: []
gated: false
touches:
  - crates/fsm-cli/src/mcp/serve.rs
  - crates/fsm-cli/src/mcp/mod.rs
  - crates/fsm-cli/src/mcp/tools.rs
  - crates/fsm-cli/src/mcp/descriptions.rs
  - crates/fsm-cli/src/mcp/resources.rs
  - crates/fsm-cli/src/mcp/prompts.rs
  - crates/fsm-cli/tests/mcp_lifecycle.rs
  - "crates/fsm-cli/tests/fixtures/transcripts/lifecycle.*"
status: planned
merged_as: ""
---
# Negotiation And Lifecycle

The plan-0001 skeleton proves only the handshake; this task hardens the serve loop to the full lifecycle — negotiation table, initialize gate, batch rejection, notification policy, panic hook, EOF shutdown, and the error-channel rule — and pre-routes stub modules so the later tool/resource/prompt tasks each touch only their own file.

**Steps:**

1. Author the fixture transcript first: `crates/fsm-cli/tests/fixtures/transcripts/lifecycle.in.jsonl` and `lifecycle.out.jsonl` covering per-revision echo for `2025-03-26` and `2024-11-05`, fallback to `2025-06-18` for `2025-11-25` and for an unknown string, a `notifications/cancelled` that produces no output, a duplicate request id answered normally, and a request after EOF-of-initialize-gate returning `-32002`; plus `crates/fsm-cli/tests/mcp_lifecycle.rs` byte-comparing the full stream.
2. Implement `negotiate`, the initialize gate, batch rejection under every revision, the notification policy, the panic hook (stderr + `abort`), and EOF shutdown (flush, drop store, exit 0) in `crates/fsm-cli/src/mcp/serve.rs`, parameterized as `serve(store, clock, input, output)` per architecture.
3. Implement the error-channel helpers `rpc_error` (envelope faults only) and `tool_error` (in-band `isError: true` with rendered text plus the structured error object).
4. Create stub modules `tools.rs`, `descriptions.rs`, `resources.rs`, `prompts.rs` (empty registries, empty `INSTRUCTIONS` const), register them in `mcp/mod.rs`, pre-route `tools/*`, `resources/*`, and `prompts/*` to them, and advertise the complete capabilities object.

- **Done when:** the lifecycle transcript passes byte-exactly under `cargo test -p fsm-cli --test mcp_lifecycle`, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
