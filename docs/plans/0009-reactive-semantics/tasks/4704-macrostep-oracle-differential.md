---
id: macrostep-oracle-differential
title: "Macrostep Oracle Differential"
workstream: "0047"
kind: task
depends_on:
  - done-region-events
  - macrostep-replay-and-fold
  - eventless-cycle-analysis
gated: false
touches:
  - crates/fsm-core/tests/oracle.rs
  - crates/fsm-core/tests/oracle/macrostep.rs
  - crates/fsm-core/tests/oracle/step.rs
  - crates/fsm-core/tests/oracle/create.rs
  - crates/fsm-core/tests/oracle/deadline.rs
  - crates/fsm-core/tests/oracle/eval.rs
  - crates/fsm-core/tests/enumerate_small/compare.rs
  - crates/fsm-core/tests/enumerate_small/machine_json.rs
  - crates/fsm-core/tests/enumerate_small.rs
  - crates/fsm-core/tests/enumerate_reactive.rs
status: done
merged_as: ""
---
# Macrostep Oracle Differential

Plan 0003 proved the single-transition engine against a deliberately naive second interpreter over exhaustively enumerated small trees; a macrostep is new control flow and gets exactly the same treatment, because a loop is where a clever implementation quietly disagrees with the specification.

**Steps:**

1. Create `crates/fsm-core/tests/oracle/macrostep.rs` implementing the macrostep loop **the dumbest possible way**: a `Vec` used as a queue with `remove(0)`, a linear scan over all transitions for eventless candidates, no memoisation, no shared helpers with the engine. The naivety is the point — a second implementation that borrows the first's abstractions tests nothing.
2. Extend `crates/fsm-core/tests/enumerate_small/machine_json.rs` and `trees.rs` to emit, per enumerated tree: zero or one eventless transitions per state, zero or one `raise` in one block, and zero or one `final` leaf per compound. Keep the enumeration small enough that the existing runtime budget holds; the suite's value is exhaustiveness over tiny shapes, not size.
3. Differentially compare, for every enumerated machine and every input event: the final configuration, the final context, the effect list with its `k` numbering, the microstep sequence including triggers, the status, and the rejection code when either rejects — including `run/microstep_limit`.
4. Assert both implementations agree on **admission**: a machine the cycle analysis refuses must be refused by the oracle's own independent cycle check, which is how `4304`'s analysis gets differentially tested rather than merely unit-tested.
5. Wire the new module into `crates/fsm-core/tests/oracle.rs` beside the existing `create`, `step`, `deadline`, `eval`, `independence`, and `reach` legs.
6. Print the machine JSON and the diverging field on failure, matching what the existing oracle legs already do, so a failure is a reproduction rather than a hunt.

**Tests:**

- The differential itself is the test: every enumerated machine × every event agrees on all six compared outputs.
- A deliberately introduced engine bug — for example, draining the queue before eventless selection — is caught by the differential; verify during development by making the change locally and observing a failure, then revert.
- The oracle's own `run/microstep_limit` fires at the same boundary as the engine's, proving the ceiling is a specified number and not an implementation detail.
- Machines refused at admission by the cycle analysis are refused by both, and machines accepted by both never hit `run/microstep_limit` unless they contain a guarded cycle.
- Runtime stays within the existing oracle suite's CI budget on all three operating systems; report the measured wall time in the commit message.

- **Done when:** `cargo test -p fsm-core --test oracle` passes with the macrostep leg enabled over the extended enumeration, the naive implementation shares no code with the engine, admission agreement is asserted, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.

**Landed:** `oracle/macrostep.rs` is the loop, the dumbest possible way — a `Vec` queue popped with `remove(0)`, a linear scan over every transition for eventless candidates, generated done events found by walking the spec, the ceiling written as the number 64 — and every naive entry point runs it: `step.rs` was split into selection plus `apply_candidate`, `create.rs` treats the initial entry as the trigger of a macrostep, `deadline.rs` hands its transition to the same loop, and `eval.rs`'s blocks now also raise, evaluating `with` against the pre-block snapshot like an emit. `naive_certain_cycle` is the brute-force admission check (every nonempty set of leaves, closed under every selectable candidate up to the first certain one), asserted equal to `def/eventless_cycle` on every enumerated machine. The enumeration lives in a second root, `enumerate_reactive.rs`, sharing `enumerate_small/`'s toolkit (`compare.rs` gained `compare_macrostep_run`, `assert_macrostep_parity`, `assert_admission_parity`; `machine_json.rs` gained the eventless, raise, and final shapes; `trees.rs` needed nothing) so `enumerate_small.rs` stays under the file ceiling; both roots mark the shared modules `allow(dead_code)` for the part they do not use. Six families: eventless source × target × guard, raises from transition, entry, and exit blocks with a handler anywhere or nowhere, a final leaf under every compound with its done event handled on the compound, an ancestor, or nobody, raise-plus-eventless orderings, the 63/64/65 counted self-loop, and three fork/join machines; six compared outputs plus discards, and a refused creation compared by cause. The injected-bug control from the task's test list was run honestly: draining the queue before eventless selection passed the first five families — nothing in them combined a raise with an eventless transition — which is why the ordering family exists; with it the mutation fails, and the engine was reverted. Development-host timings are in the commit message.
