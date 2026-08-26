---
id: reactive-analysis-and-diagram
title: "Reactive Analysis And Diagram"
workstream: "0047"
kind: task
depends_on:
  - eventless-cycle-analysis
  - done-region-events
gated: false
touches:
  - crates/fsm-core/src/diagram.rs
  - crates/fsm-core/src/analyze.rs
  - crates/fsm-cli/src/mcp/tools/handlers/machine.rs
  - crates/fsm-cli/src/mcp/tools/schema_out.rs
  - crates/fsm-core/tests/regions_deadlines_analysis_diagram.rs
status: planned
merged_as: ""
---
# Reactive Analysis And Diagram

A diagram that draws an eventless transition as an ordinary arrow is lying about the machine, and an analysis that does not report which done events a definition can generate leaves the model guessing at names it must spell exactly.

**Steps:**

1. In `crates/fsm-core/src/analyze.rs`, add to the analysis output: `eventless_transitions` (count plus the per-cycle findings from `4304`), `done_events` (the exact generated names this machine can produce — `$done.state.*` for each compound owning a `final` child, `$done.region.*` for each region), and `internal_events` (declared names marked `internal: true`, per `4401`).
2. In `crates/fsm-core/src/diagram.rs`, render an eventless transition with an **empty label** and a dashed arrow: Mermaid `-.->`, DOT `style=dashed`. Render a `final` state as Mermaid `(((name)))` and DOT `shape=doublecircle`, distinct from a `terminal` state's existing rendering — if the two look alike the diagram teaches the confusion this plan exists to remove.
3. Render a transition triggered by a generated done event with its `$done.…` label intact. The `$` must survive the existing escaping path in both formats; do not strip or rewrite it.
4. In `crates/fsm-cli/src/mcp/tools/handlers/machine.rs` and `schema_out.rs`, add the three analysis fields to `machine_analyze`'s structured output and its declared output schema. They are additive and optional in the schema, so an existing caller's parse is unaffected.
5. Keep `machine_diagram`'s `instance` overlay working: an instance mid-cascade is never observable (a macrostep is atomic), so the overlay always paints a quiesced configuration and needs no new state.

**Tests:**

- `crates/fsm-core/tests/regions_deadlines_analysis_diagram.rs`: a machine with eventless transitions renders them dashed and unlabelled in both formats; an evented transition beside it is unchanged.
- A `final` state renders as a double circle in both formats, visibly distinct from a `terminal` state in the same diagram.
- A `$done.region.a` transition label survives escaping in Mermaid and DOT.
- `analyze` reports the exact set of generatable done events for a machine with two compounds and two regions, and an empty set for a non-reactive machine.
- `analyze` reports `internal_events` for declared internal events and omits ordinary ones.
- Non-reactive machines produce byte-identical diagram output and byte-identical `analyze_all` findings to the committed goldens.
- `diagram_hostile.rs` gains a case whose state names would collide with a generated done name after escaping, proving the escaping path is exercised even though `def/reserved_ident` makes a real collision impossible.
- `machine_analyze`'s structured output validates against its own declared output schema, via the existing `tool_schemas.rs` machinery.

- **Done when:** `cargo test -p fsm-core --test regions_deadlines_analysis_diagram` and `cargo test -p fsm-cli --test tool_schemas` pass with the new fields and renderings, non-reactive diagram and analysis goldens are unchanged, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
