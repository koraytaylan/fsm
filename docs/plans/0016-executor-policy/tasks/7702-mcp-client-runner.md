---
id: mcp-client-runner
title: "MCP Client Runner"
workstream: "0077"
kind: task
depends_on:
  - mcp-handler-config
gated: false
touches:
  - crates/fsm-execute/src/mcp_client.rs
  - crates/fsm-execute/src/lib.rs
  - crates/fsm-execute/src/run.rs
  - crates/fsm-execute/tests/mcp_client.rs
status: planned
merged_as: ""
---
# MCP Client Runner

The executor already spawns processes under a timeout with bounded capture; this makes one of them a conversation instead of an exit code, and reuses everything else.

**Steps:**

1. Create `crates/fsm-execute/src/mcp_client.rs`, declare `pub mod mcp_client;` in `crates/fsm-execute/src/lib.rs` beside the crate's existing modules, and implement a minimal stdio MCP client over the workspace's own JSON parser and writer. No new dependency, and no reuse of `fsm-cli`'s server code — this crate must not depend on the binary crate.
2. Spawn `argv` with piped stdin and stdout under the handler's existing timeout and kill machinery in `crates/fsm-execute/src/run.rs`. Capture stderr to a file with the same bounded, digest-backed capture the process kind uses, so a crashing server leaves evidence rather than silence.
3. Perform exactly this exchange: `initialize` with protocol version `2025-06-18`, read the result, send `notifications/initialized`, send one `tools/call`, read until its response. Nothing else.
4. **One effect is one tool call.** A handler that needs two calls is two effects, which keeps each independently retryable and independently journaled. Write the reasoning in the module doc; it is the constraint that keeps this feature small.
5. Enforce the same line cap the server uses on inbound lines, and treat a malformed line, an unexpected message, a missing `initialize` result, or a response id mismatch as `exec/mcp_protocol` — a protocol violation from a subprocess is a failure of that effect, never a panic in the executor.
6. Spawn **one process per effect** with no pooling and no reuse. The same reasoning that gives each subprocess handler its own process: an isolated timeout, an isolated kill, and no state shared between effects that could make one effect's failure another's problem.
7. Kill and reap on timeout exactly as the process runner does, and drop into the same orphan boundary plan 0008 documented — an executor killed outright leaves a re-parented child, and the next executor re-runs the effect.
8. Ignore any notification or log message the server sends while waiting; read until the awaited response id or the timeout. A server that logs is not a server that failed.

**Tests:**

- `crates/fsm-execute/tests/mcp_client.rs`: against a stub MCP server (the test binary re-executing itself with a marker argument, following `crash_harness.rs`'s cross-platform precedent), a full exchange returns the tool result.
- A server that never responds is killed at the timeout and reported `exec/timeout`.
- A server that exits during `initialize` is reported `exec/mcp_protocol`.
- A malformed JSON line, an oversized line, a response with an unexpected id, and an unexpected message kind are each `exec/mcp_protocol` with no panic.
- Server notifications and log messages sent before the response are ignored and the response is still read.
- Stderr is captured, bounded, and digested when oversized, exactly as for a process handler.
- One process per effect: running two effects spawns two servers, and neither is reused.
- A `spawn` failure names `argv[0]` and reports `exec/spawn`.
- Killing the client mid-conversation reaps the child and leaves no zombie.
- The crate still has only `fsm-core` and `fsm-store` as path dependencies — `cargo tree -p fsm-execute --depth 1` is unchanged.

- **Done when:** `cargo test -p fsm-execute --test mcp_client` passes every case above with a cross-platform stub server, every protocol violation is a bounded failure rather than a panic, one process per effect is enforced, no dependency is added, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
