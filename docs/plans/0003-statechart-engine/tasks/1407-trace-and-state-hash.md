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

1. Author `crates/fsm-core/tests/trace_render.rs` first, encoding exactly the inventory under **Tests**.
2. Implement `DecisionTrace`, `LevelTrace`, `CandidateTrace`, `BlockTrace`, `SetTrace`, `InvariantTrace`, and `to_value()` in `crates/fsm-core/src/trace.rs` per architecture, wiring them into `step()`/`create()` outputs.
3. Implement `state_hash(machine_id, instance_id, seq, st)` in `crates/fsm-core/src/hashes.rs`: tag `fsm:state:1` over the canonical `fsm.state/1` value with sorted context, sorted bound history entries, and sorted pending ids.

**Tests:**

- Applied-event trace golden: one `case_review` event whose guard short-circuits (a `Skipped` subtree present) and whose pipeline spans multiple blocks — the full `to_value()` rendering byte-compared against a committed golden string in the test: chain-grouped `LevelTrace` order, per-`SetTrace` before/after values, spans present.
- Rejected-event trace golden: the entry-block-failure case — the completed exit block's `SetTrace` values present and marked discarded, the failing block named `entry(<state>)` with its span, invariants absent (never reached).
- `not_considered` and guard-grouping: a two-level candidate scan renders one `LevelTrace` per chain level in innermost-first order with the loser labeled.
- State-hash sensitivity, five pairwise cases each asserting a *different* 64-lowercase-hex string: changing only the leaf; only one context value; only a history binding; only the pending set; only `seq` — and byte-identical output across two computations of the identical state.
- Canonical ordering: two `InstanceState`s built by inserting context and history entries in different orders hash identically (BTreeMap canonicalization pinned).
- Domain separation: the same state value hashed via `state_hash` differs from `domain_hash` under the machine tag (tags separate domains).

- **Done when:** both trace goldens match byte-for-byte and the state-hash sensitivity, ordering, and separation cases pass under `cargo test -p fsm-core --test trace_render`, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
