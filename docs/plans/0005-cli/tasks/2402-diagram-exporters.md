---
id: diagram-exporters
title: "Diagram Exporters"
workstream: "0024"
kind: task
depends_on:
  - output-frame
gated: false
touches:
  - crates/fsm-core/src/diagram.rs
  - crates/fsm-core/src/lib.rs
  - crates/fsm-core/tests/diagram_golden.rs
  - "crates/fsm-core/tests/fixtures/diagram/**"
  - crates/fsm-cli/src/cli/diagram.rs
status: planned
merged_as: ""
---
# Diagram Exporters

Machines render as deterministic Mermaid stateDiagram-v2 (nested composite blocks, history stereotypes) and DOT (clusters), with an optional instance overlay marking the current and visited states — pure exporters pinned by golden fixtures authored first.

**Steps:**

1. Author the goldens first: `crates/fsm-core/tests/fixtures/diagram/case_review.mmd` and `case_review.dot` for the reference machine (nested block for the composite, initial arrows, terminal arrows, deep-history stereotype node), plus `crates/fsm-core/tests/diagram_golden.rs` byte-comparing both formats with and without an overlay.
2. Implement `InstanceOverlay`, `mermaid`, and `dot` in `crates/fsm-core/src/diagram.rs` per architecture (BTree-ordered, fully deterministic output), and add `pub mod diagram;` to `crates/fsm-core/src/lib.rs`.
3. Fill `crates/fsm-cli/src/cli/diagram.rs::SPECS` with `machine diagram <machine> [--format mermaid|dot] [--instance ID] [-o FILE]` — `-o` writes the file, default prints through the output frame; `--instance` builds the overlay from the store.

- **Done when:** `cargo test -p fsm-core --test diagram_golden` byte-matches both formats with and without overlay, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
