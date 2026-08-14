---
id: oracle-differential
title: "Oracle Differential"
workstream: "0015"
kind: task
depends_on:
  - trace-and-state-hash
  - simulate
  - enabled-events
gated: false
touches:
  - crates/fsm-core/tests/oracle.rs
  - crates/fsm-core/tests/enumerate_small.rs
  - crates/fsm-core/tests/step_golden.rs
  - crates/fsm-core/tests/history_props.rs
  - "crates/fsm-core/tests/fixtures/scenarios/**"
status: planned
merged_as: ""
---
# Oracle Differential

The engine's semantics are proven three ways before they ossify into journals: a deliberately naive second interpreter run differentially over an exhaustive small-tree enumeration, ordering goldens authored from SPEC prose, and seeded history properties — the fast indexed engine versus the obvious one, with any disagreement a bug by definition.

**Steps:**

1. Author `crates/fsm-core/tests/fixtures/scenarios/*.json` first, from SPEC §Semantics prose (not from running the engine): scenario files pinning exact `exited`/`entered`/pipeline/effect sequences for external self-transition, ancestor target, descendant target, transition to an ancestor of the source, deep history, shallow history, unbound history, internal transition, and the creation chain; plus `crates/fsm-core/tests/step_golden.rs` asserting them.
2. Implement the naive oracle in `crates/fsm-core/tests/oracle.rs`: a direct recursive walk over `MachineSpec` with no precomputed tables, recomputing exit/entry paths by parent-walking, clarity over speed.
3. Implement `crates/fsm-core/tests/enumerate_small.rs`: exhaustively enumerate machines (depth ≤ 3, ≤ 5 states, ≤ 1 history pseudostate, ≤ 2 events, guards from `{none, true, false, ctx.b, not ctx.b}`, ≤ 1 set per block) crossed with all event sequences of length ≤ 4; assert engine ≡ oracle on outcome kind, leaf, context, history, and effect order; rejected outcomes leave state bit-identical; `analyze` reachability equals the brute-force enterable set; the evaluation budget never trips.
4. Implement `crates/fsm-core/tests/history_props.rs`: seeded random walks (seed printed on failure) asserting deep-history resume restores the pre-suspend leaf with entry blocks observably re-run (the `visits` counter), internal transitions never change leaf or history, and rejected events never change history bindings.

- **Done when:** all four suites pass — every scenario golden exact, zero engine/oracle disagreements across the full enumeration — under `cargo test -p fsm-core --test step_golden --test enumerate_small --test history_props`, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
