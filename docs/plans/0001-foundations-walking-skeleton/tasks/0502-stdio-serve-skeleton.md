---
id: stdio-serve-skeleton
title: "Stdio Serve Skeleton"
workstream: "0005"
kind: task
depends_on:
  - jsonrpc-envelope
  - json-canonical-writer
gated: false
touches:
  - crates/fsm-cli/src/mcp/serve.rs
  - crates/fsm-cli/src/main.rs
  - crates/fsm-cli/tests/mcp_skeleton.rs
  - "crates/fsm-cli/tests/fixtures/transcripts/**"
status: done
merged_as: ""
---
# Stdio Serve Skeleton

`fsm serve` must complete a real MCP handshake before any engine exists, retiring the host-compatibility risk early; the loop is pinned by a byte-exact recorded transcript authored from the 2025-06-18 specification before the implementation.

**Steps:**

1. Author `crates/fsm-cli/tests/fixtures/transcripts/skeleton.in.jsonl` / `skeleton.out.jsonl` and `crates/fsm-cli/tests/mcp_skeleton.rs` first, encoding exactly the transcript inventory under **Tests** (expected responses hand-derived from the MCP 2025-06-18 specification and the architecture's negotiation table — never recorded from the implementation).
2. Implement `serve(input: impl BufRead, output: impl Write)` and `run()` in `crates/fsm-cli/src/mcp/serve.rs` per architecture: 16 MiB line cap, initialize gate, the version negotiation table, capabilities `{tools: {listChanged: false}}`, `serverInfo`, `ping`, `tools/list`, `tools/call fsm_ping`, notification ignoring, EOF shutdown.
3. Implement the single stdout chokepoint `send_line` (canonical bytes, newline assertion, flush) and the `FSM_LOG`-gated stderr `log` helper carrying the scoped `#[expect(clippy::print_stderr)]`.
4. Switch the `serve` arm in `crates/fsm-cli/src/main.rs` from the scaffold stub to `fsm_cli::mcp::serve::run()`.

**Tests:**

- The transcript (`skeleton.in.jsonl` → `skeleton.out.jsonl`, full output stream byte-compared by `mcp_skeleton.rs`), in order: `ping` before initialize → `{}` (allowed at any stage); `tools/list` before initialize → error `-32002`; `initialize` offering `2025-11-25` → result with `protocolVersion: "2025-06-18"`, the exact capabilities object, and `serverInfo`; `notifications/initialized` → no output line; `tools/list` → exactly the one `fsm_ping` stub tool with its schema; `tools/call fsm_ping` → text content `pong`; a batch array → `-32600`; an unknown method → `-32601`; malformed JSON → `-32700` with a null id.
- A second transcript pair (`skeleton_echo.in/out.jsonl`): `initialize` offering `2025-03-26` → echoed back `2025-03-26`; offering `2024-11-05` in a fresh session → echoed back — pinning the whole negotiation table.
- In-memory (not fixture) cases in `mcp_skeleton.rs`: a line exceeding the 16 MiB cap (constructed at runtime) → `-32700` whose message names the cap; EOF immediately after `initialize` → clean `Ok(())` return with all output flushed; an unknown *notification* → no output and the loop continues (next request still answered).
- Output hygiene assertions over every transcript run: the output buffer contains exactly one `\n` per response, no response line contains an interior newline, and nothing is written for notifications — byte-comparison enforces all three, stated here so a failure is diagnosable.

- **Done when:** both transcript pairs and the in-memory cap/EOF/notification cases pass byte-exactly under `cargo test -p fsm-cli --test mcp_skeleton`, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
