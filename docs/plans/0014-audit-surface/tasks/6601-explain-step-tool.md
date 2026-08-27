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
  - crates/fsm-cli/tests/tool_schemas.rs
  - crates/fsm-cli/tests/mcp_full.rs
  - crates/fsm-cli/tests/mcp_regions_deadlines.rs
  - crates/fsm-cli/tests/naive_caller/core_tests.rs
  - crates/fsm-cli/tests/review_regressions/output_schema_and_wire_format.rs
  - crates/fsm-cli/tests/mcp_affordance_golden.rs
  - crates/fsm-cli/tests/fixtures/
  - docs/EMBEDDING.md
status: done
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

**Landed:** `explain_step(instance_id, seq)` returns `explain_seq`'s value **unchanged** — candidates with their guard verdicts, the block pipeline with before and after values, the invariant results, and plan 0009's microsteps when the record has them. Parity with `fsm explain --json` is asserted by canonical bytes rather than assumed from a shared call site, because divergence between the two surfaces is the whole failure this tool's shape avoids. A rejection explains as a rejection: its kind, the event and payload that were sent, the configuration they met, and an empty candidate list — which is the explanation. A seq belonging to another instance or past the journal's end is a structured error rather than an empty trace.

Twenty tools measure **31 149** bytes against the 38 000 ceiling: `explain_step` cost 961, well under its 1 562 share, leaving 6 851 for the four still to come.

**Corrections.**

- *`explain_step` is the first tool with a required numeric argument, and the schema suite could not build a sample for one.* Its argument generator mapped `integer` to the string fallback, so this tool failed validation for a reason that had nothing to do with it. Fixed there.
- *Seven suites and three fixtures count the tools.* Registry order, listing length, output-schema coverage, the skeleton transcript, and 6501's affordance golden all move when a tool is added — each one a gate catching the addition, which is what they are for.
- *EMBEDDING's read-only list grew by one.* 6502's documentation test reads that list back out of the prose and compares it to `MUTATING_TOOLS`, so the doc cannot lag the registry.
