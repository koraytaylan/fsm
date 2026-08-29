---
id: audit-tools-seal
title: "Audit Tools Report The Seal"
workstream: "0082"
kind: task
depends_on:
  - cli-journal-archive
gated: false
touches:
  - crates/fsm-cli/src/mcp/tools/schema_out.rs
  - crates/fsm-cli/src/mcp/tools/handlers/audit.rs
  - crates/fsm-cli/tests/tools_budget.rs
  - crates/fsm-cli/tests/fixtures/mcp_affordance/session.expected
  - crates/fsm-cli/tests/fixtures/transcripts/skeleton.out.jsonl
status: done
merged_as: ""
---
# Audit Tools Report The Seal

A model reading a sealed store must be told the prefix is elsewhere, and it must be told inside the tools it already calls, because there is no byte budget for another one.

**Steps:**

1. Add the seal to the **existing** output schemas of `journal_verify`, `journal_replay`, and `store_doctor` in `crates/fsm-cli/src/mcp/tools/schema_out.rs`: the cut sequence, the sealed last hash, the archive id, and — for verify — which of the three verdicts this result is.
2. **Add no tool.** `tools/list` measures 36 256 bytes against the 38 000 ceiling, leaving 1 744 bytes, about one tool at the current mean. Sealing is an operator action with a mandatory target directory and a destructive-looking footprint; it belongs to the CLI. The model's interest is reading a seal, not writing one.
3. `tools_budget.rs` must still pass. If the schema additions do not fit, **shorten descriptions rather than raise the ceiling**, exactly as that test's comment instructs: a ceiling that only ever goes up is not a budget. Record the measured before-and-after byte count in the commit message. (One optional object per schema costs about seventy bytes in total, so nothing had to be shortened; a test now reports the measured size in a CI log rather than only when the ceiling is crossed.)
4. Keep the tool annotations correct: all three remain read-only, and nothing in this task makes any tool mutating. The annotations are derived from `MUTATING_TOOLS` and must stay derived — never a second table.
5. Make the middle verify verdict unmistakable in the structured result, exactly as `verify-from-seal` made it unmistakable in prose: a distinct enumerated verdict value, not a boolean plus an optional field a caller can overlook.
6. A degraded server — one whose store will not open — still answers all three tools on a sealed store. That is plan 0014's property and this task must not narrow it.
7. Keep the structured results byte-identical to the CLI's `--json` output, per the standing parity contract, and let `mcp_structured_parity.rs` prove it.

**Tests:**

- `crates/fsm-cli/tests/tool_schemas.rs`: the three output schemas carry the seal fields, and the verify schema enumerates exactly three verdicts.
- `crates/fsm-cli/tests/audit_golden.rs`: the structured results for an unsealed store are byte-identical to the pre-task goldens.
- Golden results for a sealed store in all three verify states — unsealed, sealed-unwalked, sealed-walked — are byte-compared.
- `tools_budget.rs` passes, and the test asserting the measured size records the new number.
- `tool_annotations.rs` passes with all three tools still read-only.
- `mcp_structured_parity.rs` passes: the structured result equals the CLI `--json` output for a sealed store.
- A degraded server answers `journal_verify`, `journal_replay`, and `store_doctor` on a sealed store whose base is missing.
- The middle verdict is distinguishable in the structured result without reading any prose field.

- **Done when:** `cargo test -p fsm-cli --test tool_schemas --test audit_golden --test tools_budget --test tool_annotations --test mcp_structured_parity` passes, no tool was added, the byte count stays under 38 000 with the before-and-after recorded, the middle verdict is a distinct enumerated value, a degraded server still serves all three, and `cargo test`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo fmt --check` succeed.
