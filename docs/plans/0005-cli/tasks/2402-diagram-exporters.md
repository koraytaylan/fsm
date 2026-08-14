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

1. Author the goldens under `crates/fsm-core/tests/fixtures/diagram/` and `crates/fsm-core/tests/diagram_golden.rs` first, encoding exactly the inventory under **Tests**.
2. Implement `InstanceOverlay`, `mermaid`, and `dot` in `crates/fsm-core/src/diagram.rs` per architecture (BTree-ordered, fully deterministic output), and add `pub mod diagram;` to `crates/fsm-core/src/lib.rs`.
3. Fill `crates/fsm-cli/src/cli/diagram.rs::SPECS` with `machine diagram <machine> [--format mermaid|dot] [--instance ID] [-o FILE]` — `-o` writes the file, default prints through the output frame; `--instance` builds the overlay from the store.

**Tests:**

- `diagram_golden.rs` byte-compares four committed goldens for the reference machine: `case_review.mmd` (a nested `state in_review { … }` block; `[*]` initial arrows at top level to `intake` and inside the composite to `docs_review`; `approved` and `rejected` arrows to `[*]`; a `resume_review` node annotated `<<deep-history>>`), `case_review.dot` (`subgraph cluster_…` for the composite, same nodes and edges), and `case_review_overlay.mmd` / `case_review_overlay.dot` for the overlay `current = risk_review, visited = {intake, docs_review}` (current marked bold via `classDef`/node attributes, visited dimmed, unvisited plain).
- Determinism: rendering each format twice yields byte-identical output (BTree iteration order, no map randomness).
- Well-formedness spot-assertions inside the test (cheap guards beyond byte-equality): every state name appears exactly once as a node declaration per format, and the transition count in the output equals the machine's transition count.
- CLI wiring, inline in `diagram.rs`: `-o FILE` writes bytes identical to the stdout rendering of the same invocation; an unknown `--format` value → usage error, exit 2; `--instance` with an unknown id → the not-found error, exit 3.

- **Done when:** `cargo test -p fsm-core --test diagram_golden` byte-matches both formats with and without overlay, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
