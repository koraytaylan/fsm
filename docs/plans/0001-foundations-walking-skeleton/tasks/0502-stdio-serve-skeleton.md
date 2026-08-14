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
status: planned
merged_as: ""
---
# Stdio Serve Skeleton

`fsm serve` must complete a real MCP handshake before any engine exists, retiring the host-compatibility risk early; the loop is pinned by a byte-exact recorded transcript authored from the 2025-06-18 specification before the implementation.

**Steps:**

1. Author the transcript fixtures first: `crates/fsm-cli/tests/fixtures/transcripts/skeleton.in.jsonl` and `skeleton.out.jsonl` covering, in order: `ping` before initialize, a non-ping request before initialize (`-32002`), `initialize` offering `2025-11-25` (server answers `2025-06-18`), `notifications/initialized`, `tools/list` (one stub tool `fsm_ping`), `tools/call` on `fsm_ping` (text content `pong`), a batch array (`-32600`), an unknown method (`-32601`), and malformed JSON (`-32700`); plus `crates/fsm-cli/tests/mcp_skeleton.rs`, which pipes the `.in` file through the serve function over in-memory buffers and byte-compares the full output to `.out`.
2. Implement `serve(input: impl BufRead, output: impl Write)` and `run()` in `crates/fsm-cli/src/mcp/serve.rs` per architecture: 16 MiB line cap, initialize gate, the version negotiation table, capabilities `{tools: {listChanged: false}}`, `serverInfo`, `ping`, `tools/list`, `tools/call fsm_ping`, notification ignoring, EOF shutdown.
3. Implement the single stdout chokepoint `send_line` (canonical bytes, newline assertion, flush) and the `FSM_LOG`-gated stderr `log` helper carrying the scoped `#[expect(clippy::print_stderr)]`.
4. Switch the `serve` arm in `crates/fsm-cli/src/main.rs` from the scaffold stub to `fsm_cli::mcp::serve::run()`.

- **Done when:** the transcript test passes byte-exactly under `cargo test -p fsm-cli --test mcp_skeleton`, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
