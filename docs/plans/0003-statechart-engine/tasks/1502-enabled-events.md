---
id: enabled-events
title: "Enabled Events"
workstream: "0015"
kind: task
depends_on:
  - apply-pipeline
  - shadowing-and-const-fold
gated: false
touches:
  - crates/fsm-core/src/analyze.rs
  - crates/fsm-core/tests/enabled_events.rs
status: planned
merged_as: ""
---
# Enabled Events

The steering-wheel report for a live instance: for every declared event, walk the ancestor chain in conflict order with three-valued guard evaluation (context concrete, event fields unknown) and report enabled, disabled, depends-on-payload, or preempted — so a caller never has to guess what an instance can accept next.

**Steps:**

1. Implement `EventStatus { Enabled, Disabled, DependsOnPayload, Preempted, PreemptedMaybe }`, `EventReport` (per-candidate detail plus, for payload-dependent guards, the event field names read), and `enabled_events(m, t, st, budget)` in `crates/fsm-core/src/analyze.rs` per architecture: the per-event summary is the first non-preempted status down the chain; a definite inner winner preempts everything after it; an unknown inner candidate makes later candidates `PreemptedMaybe`.
2. Add `crates/fsm-core/tests/enabled_events.rs`: table cases over `case_review` (in `docs_review`: `docs_ok` enabled, `scored` disabled, ancestor events `suspend`/`withdraw`/`note_added` enabled; in `risk_review`: `scored` depends-on-payload with field `score` reported) plus hand-built machines exercising `Preempted` and `PreemptedMaybe` orderings and the conservative-Unknown rule for guards whose concrete subtree errors.

- **Done when:** the report table cases pass, including preemption orderings and payload-field reporting, under `cargo test -p fsm-core --test enabled_events`, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
