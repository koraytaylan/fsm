---
id: instance-commands
title: "Instance Commands"
workstream: "0025"
kind: task
depends_on:
  - machine-commands
gated: false
touches:
  - crates/fsm-cli/src/cli/instance.rs
status: planned
merged_as: ""
---
# Instance Commands

The full instance lifecycle from a terminal — create, send with idempotent request ids and optimistic seq checks, acknowledge effects, cancel with an audited reason, annotate, inspect, list, page history — plus `explain`, which recomputes any past decision trace from the pinned definition.

**Steps:**

1. Fill `crates/fsm-cli/src/cli/instance.rs::SPECS` with `instance new` (`--context k=v`, `--context-json J|@f`, `--request-id`) and `instance send <instance> <event>` (`--payload J|@f|-`, `--request-id`, `--expect-seq N`, `--stamp FIELD` resolving the server clock into a declared timestamp payload field before journaling).
2. Add `instance ack <instance> <effect_id> --outcome ok|failed [--result J]`, `instance cancel --reason TEXT`, and `instance annotate <text>`.
3. Add `instance show` (leaf path, configuration, context, pending effects, enabled events), `instance ls` (`--machine`, `--state`, `--status running|completed|cancelled|all`), and `instance history` (`--from-seq`, `--limit`, `--trace`).
4. Add `explain <instance> --seq N` recomputing the full decision trace (chain-level candidates, guard sub-expression values, pipeline blocks, invariants) from the pinned definition and the journaled record.
5. Add inline unit tests over a temp store: a send rejection rendering its hint, duplicate request_id returning `duplicate: true`, stamp filling a declared timestamp field, and explain reproducing the trace of a past applied event.

- **Done when:** inline instance-command tests prove rejection-with-hint rendering, duplicate-request short-circuit, stamping, and explain-recomputation, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
