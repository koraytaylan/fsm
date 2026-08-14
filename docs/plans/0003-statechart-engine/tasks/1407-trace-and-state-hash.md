---
id: trace-and-state-hash
title: "Trace And State Hash"
workstream: "0014"
kind: task
depends_on:
  - apply-pipeline
  - canonical-hash-identity
gated: false
touches:
  - crates/fsm-core/src/trace.rs
  - crates/fsm-core/src/hashes.rs
  - crates/fsm-core/tests/trace_render.rs
status: planned
merged_as: ""
---
# Trace And State Hash

Every applied and rejected event carries a full explain trace — chain-grouped guard evaluations, per-block pipeline records with before/after values, invariant results, discarded-value preservation on rejection — and every instance state hashes canonically (history bindings included) for replay verification.

**Steps:**

1. Implement `DecisionTrace`, `LevelTrace`, `CandidateTrace`, `BlockTrace`, `SetTrace`, `InvariantTrace`, and `to_value()` in `crates/fsm-core/src/trace.rs` per architecture, wiring them into `step()`/`create()` outputs.
2. Implement `state_hash(machine_id, instance_id, seq, st)` in `crates/fsm-core/src/hashes.rs`: tag `fsm:state:1` over the canonical `fsm.state/1` value with sorted context, sorted bound history entries, and sorted pending ids.
3. Add `crates/fsm-core/tests/trace_render.rs`: golden JSON renderings for one applied event (with a skipped guard subtree and a multi-block pipeline) and one rejected event (entry-block failure preserving the completed exit block's discarded values); plus state-hash tests pinning that hashes change with leaf, context, history binding, pending set, and seq, and are byte-identical across two computations.

- **Done when:** the trace golden renderings and state-hash sensitivity tests pass under `cargo test -p fsm-core --test trace_render`, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
