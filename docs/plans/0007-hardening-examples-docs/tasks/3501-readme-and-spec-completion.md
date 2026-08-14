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

1. Author `crates/fsm-cli/tests/spec_appendix.rs` first: assert every code in `fsm_core::error::ALL_CODES` appears in the embedded SPEC bytes.
2. Write `README.md` per architecture: the one-paragraph thesis (the model translates intent into machines, the engine guarantees the semantics), the 60-second CLI demo, install via `cargo install --path crates/fsm-cli --locked`, the Claude Code (`claude mcp add fsm -- fsm serve`) and Claude Desktop (`mcpServers` JSON) setup snippets, the full 16-row guarantees table with the honest non-claims paragraph, and links to SPEC, EXAMPLES, and RELEASE.
3. Append the three SPEC.md appendices: error codes (every `ALL_CODES` entry, one line each), the limits table, and the format-version registry.
4. Add `LICENSE-MIT` and `LICENSE-APACHE` with the standard license texts matching the manifests' `MIT OR Apache-2.0`.

- **Done when:** `cargo test -p fsm-cli --test spec_appendix` passes, README contains the install command, both MCP setup snippets, and the guarantees table, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
