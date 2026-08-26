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
status: done
merged_as: ""
---
# Oracle Differential

The engine's semantics are proven three ways before they ossify into journals: a deliberately naive second interpreter run differentially over an exhaustive small-tree enumeration, ordering goldens authored from SPEC prose, and seeded history properties — the fast indexed engine versus the obvious one, with any disagreement a bug by definition.

**Steps:**

1. Author `crates/fsm-core/tests/fixtures/scenarios/*.json` first, from SPEC §Semantics prose (not from running the engine), plus `crates/fsm-core/tests/step_golden.rs`, encoding exactly the scenario inventory under **Tests**.
2. Implement the naive oracle in `crates/fsm-core/tests/oracle.rs`: a direct recursive walk over `MachineSpec` with no precomputed tables, recomputing exit/entry paths by parent-walking, clarity over speed.
3. Implement `crates/fsm-core/tests/enumerate_small.rs` and `crates/fsm-core/tests/history_props.rs` encoding the differential and property inventories under **Tests**.

**Tests:**

- Ordering goldens (`step_golden.rs` over `fixtures/scenarios/`), one scenario file each, pinning the exact `exited`/`entered` sequences, pipeline block order, and effect order: external self-transition; ancestor target; descendant target; transition to an ancestor of the source; deep history (bound); shallow history (bound); unbound history; internal transition; the creation chain. Each golden is authored from SPEC §Semantics prose — where a golden and the engine disagree, the golden wins unless it demonstrably contradicts the prose.
- Exhaustive differential (`enumerate_small.rs`): every machine with depth ≤ 3, ≤ 5 states, ≤ 1 history pseudostate, ≤ 2 events, guards from `{none, true, false, ctx.b, not ctx.b}`, ≤ 1 set per block, crossed with every event sequence of length ≤ 4 — for each run, engine ≡ oracle on: outcome kind, post leaf, full context, full history map, and effect order; any disagreement fails naming the machine and sequence.
- Differential side-conditions asserted across the same enumeration: every rejected outcome leaves the state bit-identical (engine and oracle both); `analyze` enterable-set equals the brute-force reachable set computed by the oracle's walk; the evaluation budget never trips (`internal/budget` count is zero).
- History properties (`history_props.rs`, seeded xorshift with the seed printed on failure): random walks over `case_review` and generated small trees — (a) suspend at a random point, resume via deep history → the pre-suspend leaf is restored and entry blocks observably re-ran (the `visits` counter incremented); (b) internal transitions never change leaf or history bindings; (c) rejected events never change history bindings.
- Suite hygiene: the scenario directory iterator fails on an unrecognized fixture file; the enumeration prints its total machine × sequence count so a silently shrunken generator is visible in the test output.

- **Done when:** all four suites pass — every scenario golden exact, zero engine/oracle disagreements across the full enumeration with the count printed, and all three history properties — under `cargo test -p fsm-core --test step_golden --test enumerate_small --test history_props`, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
