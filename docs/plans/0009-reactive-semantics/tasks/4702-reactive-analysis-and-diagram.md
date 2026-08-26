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
  - crates/fsm-core/src/analyze/mod.rs
  - crates/fsm-core/src/analyze/eventless.rs
  - crates/fsm-cli/src/mcp/tools/handlers/machine.rs
  - crates/fsm-cli/src/mcp/tools/schema_out.rs
  - crates/fsm-core/tests/regions_deadlines_analysis_diagram.rs
  - crates/fsm-core/tests/diagram_hostile.rs
  - crates/fsm-cli/tests/analyze_reactive.rs
  - crates/fsm-cli/tests/tools_budget.rs
  - crates/fsm-cli/tests/fixtures/transcripts/skeleton.out.jsonl
status: done
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

**Landed:** `analyze/eventless.rs::reactive_summary` (re-exported from `analyze`) returns `ReactiveSummary { eventless_transitions, eventless_findings, done_events, unhandled_done_events, internal_events }`; `machine_analyze` carries it as additive optional fields — `eventless_transitions` as a plain count, because the cycle and depth findings step 1 wanted beside it are already in the same output's `findings`, where `analyze_all` reports them — and `schema_machine_analyze_out` declares them, validated against a real output in `analyze_reactive.rs`. The additions moved two byte-exact fixtures: the MCP skeleton transcript was regenerated (`REGEN_SKELETON=1`), and the `tools/list` ceiling in `tools_budget.rs` rose from 20 000 to 21 000 bytes — the listing stood at 19 990, so any additive field crossed it; the per-description word caps that bound a model's reading cost are untouched. One field beyond the three asked for: since a generated event nobody handles is never raised (the `4603` correction), `done_events` lists the names this machine raises — generatable and handled — and `unhandled_done_events` the ones it could handle and does not; that list is where an unhandled generated name is discovered now that the trace never shows one, and for a plain parallel machine it names the region joins the author has not written. Step 2 was corrected against the target grammars: Mermaid's `stateDiagram-v2` has no `-.->` and no `(((name)))` — both are flowchart syntax — so an eventless edge announces itself in its label (`[guard] (eventless)`, or `(eventless)` alone) and a final state gets the `<<final>>` description line and never an arrow to `[*]`; DOT does dash the edge (`style=dashed`, guard-only label) and draws `shape=doublecircle`. `$done.…` labels survive both escaping paths with the prefix intact, pinned in `diagram_hostile.rs` with a join on a region whose name is hostile beside a state named after the generated name minus its prefix. The CLI's `machine analyze` command keeps its narrower findings-and-completeness output; the reactive surface is the MCP tool's. Plain machines' diagrams and analysis stay byte-exact (`sequential_diagrams_remain_byte_exact`, `diagram_golden`, `analyze_golden`).
