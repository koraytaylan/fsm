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
4. Write the inline test module encoding exactly the inventory under **Tests**.

**Tests:**

- Inline in `tools.rs`, against a temp store with a `FixedClock` — reference resolution: a full `name@sha256:<64 hex>` id resolves; a unique 12-hex prefix resolves; a bare name with one version resolves; a bare name with two stored versions → `req/machine_ambiguous` with both full ids in `details`; a prefix shorter than 12 hex is rejected.
- Idempotent replay: the same `request_id` sent twice through `instance_send` → the second response is byte-identical to the first plus `duplicate: true`, and the instance `seq` shows the event applied exactly once.
- Full post-state assembly: one applied `instance_send` response asserts, by name, the presence of `state` (leaf path), `configuration`, `context`, `effects_pending`, `trace`, `enabled_events`, `seq`, and `state_hash` — and the returned `state_hash` equals a recomputation over the store's instance state.
- Text/structured coupling: for one success and one error response, the `text` content block byte-equals the shared renderer applied to the `structuredContent`.
- Error mapping: a `run/not_enabled` rejection arrives in-band (`isError: true`) with `retryable` equal to `fsm_core::error::retryable("run/not_enabled")`; a stale `expect_seq` → `req/seq_mismatch` with `retryable: true` and the `request_id` **not** consumed (a retry with the same id after re-reading applies cleanly — the load-bearing ordering pinned at the dispatch layer too).
- Flag semantics: `dry_run: true` on `machine_create` validates but stores nothing (`machine_list` stays empty); `if_exists: "error"` on an identical spec errors while the default returns `created: false`; `stamp` on a declared timestamp field fills it with the `FixedClock` value (asserted exactly).
- Completeness sweep: iterate `registry()`, call every tool once with minimal valid arguments, and assert no `internal/unimplemented` surfaces anywhere.

- **Done when:** the inline dispatch tests pass under `cargo test -p fsm-cli` with every tool wired (the completeness sweep green), and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
