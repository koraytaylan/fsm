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

1. Author the fixtures first — `crates/fsm-cli/tests/fixtures/transcripts/full_2025-06-18.{in,out}.jsonl`, `full_2025-03-26.{in,out}.jsonl`, and `full_2024-11-05.{in,out}.jsonl` — encoding exactly the session inventory under **Tests**, with expected responses hand-derived from the workstream-0029 schema tables and the shared renderer's format (never recorded from the implementation).
2. Implement `crates/fsm-cli/tests/mcp_full.rs`: run each transcript through `serve` with a temp store and a `FixedClock` stepping 1000 ms per journal append, byte-comparing the entire stdout stream.
3. Implement `crates/fsm-cli/tests/mcp_structured_parity.rs`: replay the operations behind every plan-0005 `tests/fixtures/structured/*.json` fixture through tool dispatch and assert each `structuredContent` is byte-identical to the CLI `--json` fixture.

**Tests:**

- Each per-revision transcript drives the same session, exchange by exchange (full stdout stream byte-compared): `initialize` (revision-specific echo per the negotiation table) → `resources/list` → `prompts/get author_machine` → `machine_create` with `dry_run: true` and a deliberate `expr/unknown_var` in a guard (the response's `hint` with its Levenshtein suggestion is inside the byte-compare) → the corrected `machine_create` → `instance_create` → an applied `instance_send` that emits an effect (full post-state fields in the byte-compare) → `effect_ack` → the `confirmed` domain event reaching a terminal leaf (`status: "completed"`) → `instance_history` with `include_trace: true` (recomputed traces) → `simulate` → EOF.
- Cross-revision invariance, asserted mechanically in `mcp_full.rs`: after normalizing the initialize response's `protocolVersion` field, the three `.out` streams are byte-identical — pinning that negotiation gates nothing else in v1.
- Determinism of the harness: running one transcript twice in-process produces identical output (FixedClock and seq-derived ids leave nothing volatile).
- `mcp_structured_parity.rs`: for every file in the plan-0005 `fixtures/structured/` directory, the equivalent tool call's `structuredContent` canonical bytes equal the fixture's bytes; the test fails if that directory is missing or empty (no vacuous pass).

- **Done when:** all three per-revision transcripts, the cross-revision invariance check, and the parity test pass under `cargo test -p fsm-cli --test mcp_full --test mcp_structured_parity`, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
