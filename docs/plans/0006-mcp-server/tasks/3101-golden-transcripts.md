---
id: golden-transcripts
title: "Golden Transcripts"
workstream: "0031"
kind: task
depends_on:
  - tool-dispatch
  - resources
  - prompt-and-instructions
gated: false
touches:
  - crates/fsm-cli/tests/mcp_full.rs
  - crates/fsm-cli/tests/mcp_structured_parity.rs
  - "crates/fsm-cli/tests/fixtures/transcripts/full_*"
status: planned
merged_as: ""
---
# Golden Transcripts

The complete surface is pinned byte-for-byte per negotiated protocol revision — the regression net for framing, negotiation, schemas, dispatch, and stdout hygiene all at once — and the CLI's `--json` fixtures must byte-match MCP `structuredContent` so the two surfaces cannot drift.

**Steps:**

1. Author the fixtures first: `crates/fsm-cli/tests/fixtures/transcripts/full_2025-06-18.{in,out}.jsonl`, `full_2025-03-26.{in,out}.jsonl`, and `full_2024-11-05.{in,out}.jsonl`, each driving the architecture's full session — initialize, resources/list, prompts/get, a dry-run create with a deliberate expression error, the hinted correction, instance lifecycle with effects and `effect_ack`, a domain event to a terminal state, `instance_history` with traces, `simulate`, EOF.
2. Implement `crates/fsm-cli/tests/mcp_full.rs`: run each transcript through `serve` with a temp store and a `FixedClock` stepping 1000 ms per journal append, byte-comparing the entire stdout stream.
3. Implement `crates/fsm-cli/tests/mcp_structured_parity.rs`: replay the operations behind every plan-0005 `tests/fixtures/structured/*.json` fixture through tool dispatch and assert each `structuredContent` is byte-identical to the CLI `--json` fixture.

- **Done when:** all three per-revision transcripts and the parity test pass under `cargo test -p fsm-cli --test mcp_full --test mcp_structured_parity`, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
