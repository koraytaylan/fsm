---
id: tool-dispatch
title: "Tool Dispatch"
workstream: "0029"
kind: task
depends_on:
  - tool-registry-and-schemas
gated: false
touches:
  - crates/fsm-cli/src/mcp/tools.rs
status: planned
merged_as: ""
---
# Tool Dispatch

Every tool call flows through one dispatch layer into the store and core, and every mutating response must carry the full post-state — leaf path and configuration, context, pending effects, trace, enabled events, sequence, and state hash — so the model never needs a follow-up read.

**Steps:**

1. Replace the `run` stubs in `crates/fsm-cli/src/mcp/tools.rs` with real dispatch into the plan-0004 `Store` and plan-0003 core for all 13 tools, resolving machine references per architecture (full id, unique prefix of at least 12 hex, or unambiguous bare name — otherwise `req/machine_ambiguous` listing the versions).
2. Assemble mutating responses with the complete post-state fields from the architecture tables, `enabled_events` from the three-valued ancestor-chain report, and `duplicate: true` replays for repeated `request_id`s; honor `dry_run`, `if_exists`, `stamp`, and `expect_seq` semantics.
3. Produce the `text` block by rendering the canonical `structuredContent` through the plan-0005 shared renderer, and map every domain failure through `tool_error` with `retryable` taken solely from `fsm_core::error::retryable`.
4. Add inline unit tests covering reference resolution, duplicate replay, and one full post-state assembly against a temp store.

- **Done when:** inline dispatch tests pass under `cargo test -p fsm-cli` with every tool wired (no `internal/unimplemented` remaining in `tools.rs`), and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
