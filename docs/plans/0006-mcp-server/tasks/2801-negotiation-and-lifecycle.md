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

1. Author `crates/fsm-cli/tests/fixtures/transcripts/lifecycle.in.jsonl` / `lifecycle.out.jsonl` and `crates/fsm-cli/tests/mcp_lifecycle.rs` first, encoding exactly the inventory under **Tests** (expected responses hand-derived from the architecture's negotiation table and error-channel rule, never recorded from the implementation).
2. Implement `negotiate`, the initialize gate, batch rejection under every revision, the notification policy, the panic hook (stderr + `abort`), and EOF shutdown (flush, drop store, exit 0) in `crates/fsm-cli/src/mcp/serve.rs`, parameterized as `serve(store, clock, input, output)` per architecture.
3. Implement the error-channel helpers `rpc_error` (envelope faults only) and `tool_error` (in-band `isError: true` with rendered text plus the structured error object).
4. Create stub modules `tools.rs`, `descriptions.rs`, `resources.rs`, `prompts.rs` (empty registries, empty `INSTRUCTIONS` const), register them in `mcp/mod.rs`, pre-route `tools/*`, `resources/*`, and `prompts/*` to them, and advertise the complete capabilities object.

**Tests:**

- The transcript pair (full stdout stream byte-compared by `mcp_lifecycle.rs`, fresh session per row where a new `initialize` is needed): offer `2025-03-26` → echoed `2025-03-26`; offer `2024-11-05` → echoed `2024-11-05`; offer `2025-11-25` → answered `2025-06-18`; offer the unknown string `9999-01-01` → answered `2025-06-18`; `tools/list` before initialize → `-32002`; a batch array sent inside a `2024-11-05` session → `-32600` (batching is rejected under every negotiated revision, not only the baseline); `notifications/cancelled` → no output line, and the next request on the stream is still answered (the loop continues); a JSON-RPC id reused across two requests → both answered normally (no envelope-level dedup).
- In-memory cases in `mcp_lifecycle.rs`: EOF immediately after `initialize` → `serve` returns `Ok(())` with all output flushed and the store dropped — asserted by reopening the same temp data dir in the test (the lock must be free); a request after EOF is impossible by construction, stated for clarity.
- Panic path, via the crash-harness re-exec pattern: the test re-runs its own binary with an env flag that routes a deliberate panic through the serve loop, asserting stderr contains the panic text and the process exit is abnormal (abort, not a clean error response) — stdout must contain no partial JSON line.
- Error-channel helpers, inline: `rpc_error` produces `{jsonrpc, id, error: {code, message}}` with canonical bytes pinned for one example; `tool_error` produces `isError: true`, one `text` content block, and `structuredContent.error` carrying the shared envelope (`code`, `message`, `path`, `span?`, `hint`, `retryable`, `duplicate`, `details`, `docs`) — field presence asserted by name.
- Capabilities: the initialize result advertises `tools`, `resources`, and `prompts` exactly as the architecture's capabilities object — byte-pinned inside the transcript.

- **Done when:** the lifecycle transcript passes byte-exactly under `cargo test -p fsm-cli --test mcp_lifecycle`, including the re-exec panic case, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
