---
id: explain-step-tool
title: "Explain Step Tool"
workstream: "0066"
kind: task
depends_on: []
gated: false
touches:
  - crates/fsm-cli/src/mcp/tools/handlers/audit.rs
  - crates/fsm-cli/src/mcp/tools/handlers/mod.rs
  - crates/fsm-cli/src/mcp/tools/mod.rs
  - crates/fsm-cli/src/mcp/tools/schema_in.rs
  - crates/fsm-cli/src/mcp/tools/schema_out.rs
  - crates/fsm-cli/src/mcp/descriptions.rs
  - crates/fsm-cli/tests/audit_explain.rs
status: planned
merged_as: ""
---
# Explain Step Tool

`explain_seq` already reconstructs every candidate, every guard verdict, and every set's before and after — the single best answer to "why did it do that" — and no tool reaches it, so a model debugging a workflow guesses from history instead.

**Steps:**

1. Create `crates/fsm-cli/src/mcp/tools/handlers/audit.rs`, declared in `handlers/mod.rs`, to hold this workstream's four tools.
2. Add `explain_step(instance_id, seq)` to the registry with schemas in `schema_in.rs` and `schema_out.rs`, wrapping `store.explain_seq(instance_id, seq)` and returning its `Value` unchanged. There is nothing to compute here; do not reshape what `explain_seq` produces, or `explain_step` and `fsm explain --json` will diverge.
3. Keep it **out of** `MUTATING_TOOLS`, so it works on a read-only server and plan 0013's derived annotations give it `readOnlyHint: true` with no special case.
4. Write the description to say what the tool is *for*, not only what it does: reach for this when a workflow did something surprising; it shows which transitions were considered, which guard decided, and what every action computed. A model that does not know it exists will keep guessing from `instance_history`, and the description is the only place it can find out.
5. Report a seq that does not belong to the named instance, or does not exist, as a structured tool error using the existing `req/*` vocabulary — never an empty trace, which would read as "nothing happened".
6. Confirm the output carries plan 0009's microstep list when the record has one, by returning `explain_seq`'s value verbatim rather than projecting selected fields.

**Tests:**

- `crates/fsm-cli/tests/audit_explain.rs`: `explain_step` on an applied event returns the full trace — candidates with guard verdicts, the block pipeline with before/after values, and invariant results.
- Its output is byte-identical to `fsm explain --json` for the same instance and seq — assert directly, since divergence between the two surfaces is the failure this step prevents.
- A rejected step's record explains with the rejection's own trace.
- A seq belonging to a different instance is a structured tool error, not an empty trace.
- A seq beyond the journal end is a structured tool error.
- The tool works on a read-only server and is absent from `MUTATING_TOOLS`.
- Its derived annotations report `readOnlyHint: true` and `openWorldHint: false`.
- Its structured output validates against its declared output schema via `tool_schemas.rs`.
- A record carrying microsteps explains with them present.

- **Done when:** `cargo test -p fsm-cli --test audit_explain --test tool_schemas --test read_only` passes, `explain_step` output byte-matches `fsm explain --json`, the tool is read-only, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
