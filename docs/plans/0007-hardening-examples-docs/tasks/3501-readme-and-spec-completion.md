---
id: readme-and-spec-completion
title: "Readme And Spec Completion"
workstream: "0035"
kind: chore
depends_on:
  - example-walkthroughs
gated: false
touches:
  - README.md
  - docs/SPEC.md
  - LICENSE-MIT
  - LICENSE-APACHE
  - crates/fsm-cli/tests/spec_appendix.rs
status: planned
merged_as: ""
---
# Readme And Spec Completion

The README carries the thesis, the 60-second demo, the MCP setup, and the guarantees table; SPEC.md gains its closing appendices so an independent team could reimplement the engine from the document alone — with a test tying the error-code appendix to the code.

**Steps:**

1. Author `crates/fsm-cli/tests/spec_appendix.rs` first, encoding exactly the mechanical inventory under **Tests**.
2. Write `README.md` per architecture: the one-paragraph thesis (the model translates intent into machines, the engine guarantees the semantics), the 60-second CLI demo, install via `cargo install --path crates/fsm-cli --locked`, the Claude Code (`claude mcp add fsm -- fsm serve`) and Claude Desktop (`mcpServers` JSON) setup snippets, the full 16-row guarantees table with the honest non-claims paragraph, and links to SPEC, EXAMPLES, and RELEASE.
3. Append the three SPEC.md appendices: error codes (every `ALL_CODES` entry, one line each), the limits table, and the format-version registry.
4. Add `LICENSE-MIT` and `LICENSE-APACHE` with the standard license texts matching the manifests' `MIT OR Apache-2.0`.

**Tests:**

- `spec_appendix.rs`, mechanical — SPEC appendices: every code in `fsm_core::error::ALL_CODES` appears in the embedded SPEC bytes; the three format tags (`fsm.machine/1`, `fsm.journal/1`, `fsm.state/1`) each appear; three spot-pinned limits values from the limits appendix appear verbatim (definition size, nesting depth, eval budget) so the appendix cannot drift from `limits.rs` silently.
- `spec_appendix.rs`, mechanical — README (read from the repo path at test time): contains the exact install command `cargo install --path crates/fsm-cli --locked`, the exact Claude Code line `claude mcp add fsm -- fsm serve`, an `mcpServers` JSON snippet that parses under `fsm_core::json::parse` (extracted from its fenced block), all 16 guarantee-table rows (counted between the table's header and the non-claims paragraph), and the non-claims phrase `single-node`.
- `spec_appendix.rs`, mechanical — licensing: `LICENSE-MIT` and `LICENSE-APACHE` exist, are non-empty, and contain their respective license names.
- Review items (manual, named): the 60-second demo's commands run verbatim against a built binary; the thesis paragraph and links section read correctly; the license texts are the standard unmodified ones.

- **Done when:** `cargo test -p fsm-cli --test spec_appendix` passes every mechanical assertion above, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
