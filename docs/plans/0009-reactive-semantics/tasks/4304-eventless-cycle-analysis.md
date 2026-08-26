---
id: eventless-cycle-analysis
title: "Eventless Cycle Analysis"
workstream: "0043"
kind: task
depends_on:
  - eventless-validation
gated: false
touches:
  - crates/fsm-core/src/analyze.rs
  - crates/fsm-core/tests/eventless_cycles.rs
status: planned
merged_as: ""
---
# Eventless Cycle Analysis

A guardless eventless cycle is a machine that provably cannot quiesce, and refusing it at admission is strictly better than discovering it at run time as a `run/microstep_limit` rejection on somebody's live workflow.

**Steps:**

1. In `crates/fsm-core/src/analyze.rs`, beside the existing `reachability_findings` and `ancestor_shadowed` analyses, build the eventless transition graph: nodes are state names, and an edge `from → target` exists for each eventless transition, with `target` resolved through the same rules `step` uses — `history_descent` for a history target, `parent(from)` re-entry for an external self-transition, `properLCA` otherwise. Resolve targets with the shared helpers, not a second implementation.
2. Run an **iterative** Tarjan strongly-connected-components pass over that graph. Iterative, not recursive: `MAX_STATES` is 256 and `MAX_NESTING` is 12, but a hostile definition must not be able to blow the stack, and `diagram_hostile.rs` is the precedent for caring.
3. For each SCC with more than one node, and for each single node carrying a self-edge: if **every** edge in the component is guardless or guarded by the literal `true`, emit `def/eventless_cycle` as an **error**, naming the participating states in document order. Otherwise emit `def/eventless_cycle_guarded` as a **warning** naming the same states, whose hint says plainly that the engine cannot decide the guard and that `MAX_MICROSTEPS` is what will stop it at run time.
4. Decide guardedness **syntactically** — `if` absent, or the source is exactly the literal `true`. Do **not** reach for `expr/partial.rs`'s three-valued evaluator: admission must be a pure function of the definition, and a partial evaluation over an unknown context would make whether a machine is accepted depend on which context a caller might later supply. Put that sentence in the code as a comment; it is the kind of shortcut a later reader will otherwise "fix".
5. Compute the **longest acyclic eventless path** over the same graph and emit `def/eventless_depth` as a **warning** when `longest_path × region_count` reaches half of `MAX_MICROSTEPS`. The ceiling is shared: selection picks one global winner per microstep, so eight regions each running a three-step cascade spend 24 microsteps, not 3. An author who would hit `run/microstep_limit` on a live workflow should learn it at admission instead, and a warning is the honest strength of the claim because the analysis cannot know which branches a guard will take.
6. Register both analyses in `analyze_all` so `machine_analyze`, `fsm validate`, and `machine_create` all surface them through the paths they already use.

**Tests:**

- `crates/fsm-core/tests/eventless_cycles.rs`: `a --> b --> a`, both guardless, reports `def/eventless_cycle` naming both states, and the definition is **refused**.
- The same pair with a guard on one edge reports the `def/eventless_cycle_guarded` warning and the definition is **accepted**.
- A guardless eventless self-transition with `to` naming its own state reports `def/eventless_cycle`.
- An eventless self-transition with `to` absent (internal) reports `def/eventless_internal_noop` from `4302` and, because it has no target edge, no cycle finding — pin this, it is the case where the two rules interact.
- A three-node guardless cycle `a → b → c → a` reports one finding naming all three, not three findings.
- An acyclic eventless chain of length 5 reports nothing.
- A cycle reached through a history target resolves through `history_descent` and is still detected.
- Depth: a 200-state eventless chain plus one back edge completes without stack overflow and reports one cycle.
- Depth warning: a sequential machine whose longest eventless path is 40 reports `def/eventless_depth`; one whose longest path is 4 does not.
- Depth warning under regions: an 8-region machine whose longest per-region eventless path is 5 reports `def/eventless_depth`, because the shared ceiling is what the product measures — this is the case the warning exists for.
- The depth finding is a **warning**: the definition is still accepted and can still be created and stepped.
- A machine with no eventless transitions produces byte-identical `analyze_all` output to the pre-change behaviour — the existing `analyze_golden.rs` fixtures do not move.

- **Done when:** `cargo test -p fsm-core --test eventless_cycles` passes every case above, `analyze_golden.rs` is unchanged for non-reactive machines, the SCC pass is iterative, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
