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
status: done
merged_as: ""
---
# Instance Commands

The full instance lifecycle from a terminal — create, send with idempotent request ids and optimistic seq checks, acknowledge effects, cancel with an audited reason, annotate, inspect, list, page history — plus `explain`, which recomputes any past decision trace from the pinned definition.

**Steps:**

1. Fill `crates/fsm-cli/src/cli/instance.rs::SPECS` with `instance new` (`--context k=v`, `--context-json J|@f`, `--request-id`) and `instance send <instance> <event>` (`--payload J|@f|-`, `--request-id`, `--expect-seq N`, `--stamp FIELD` resolving the server clock into a declared timestamp payload field before journaling).
2. Add `instance ack <instance> <effect_id> --outcome ok|failed [--result J]`, `instance cancel --reason TEXT`, and `instance annotate <text>`.
3. Add `instance show` (leaf path, configuration, context, pending effects, enabled events), `instance ls` (`--machine`, `--state`, `--status running|completed|cancelled|all`), and `instance history` (`--from-seq`, `--limit`, `--trace`).
4. Add `explain <instance> --seq N` recomputing the full decision trace (chain-level candidates, guard sub-expression values, pipeline blocks, invariants) from the pinned definition and the journaled record.
5. Write the inline test module encoding exactly the inventory under **Tests** (spec `run` functions over a temp store with `FSM_CLOCK_MS` and capture buffers).

**Tests:**

- Inline in `instance.rs` — `new`: creating a `case_review` instance prints the instance id, the initial leaf `docs_review`, and the request id it used (defaulted and printed when `--request-id` is absent); an over-precision `--context` decimal → `req/field_scale` rendered, exit 1, no instance created.
- `send` applied: `docs_ok` → the transition summary (source state, exited/entered lists), the new leaf, and enabled events rendered; exit 0.
- `send` rejected: an event with no candidate anywhere on the chain → `run/unhandled` rendered on stderr with its hint and the enabled-events list, exit 1; stdout stays empty on the failure.
- Idempotency: re-running `send` with the same `--request-id` prints the original outcome marked `duplicate: true`, exit 0, and `history` shows no new record.
- Optimistic concurrency: `--expect-seq` stale with a fresh request id → `req/seq_mismatch` rendered with its re-read hint, exit 1.
- `--stamp`: a declared timestamp field absent from the payload is filled from the clock (pinned value under `FSM_CLOCK_MS`) and `history` shows the resolved concrete value in the journaled payload.
- `ack`: acknowledging a pending effect id empties it from `show`'s pending list and changes nothing else; acknowledging an unknown id → the state-dependent rejection rendered, exit 1.
- `cancel --reason`: status becomes `cancelled` with the reason in `history`; a further `send` → `run/instance_cancelled`, exit 1.
- `annotate`: the note appears in `history` verbatim; leaf, context, and enabled events unchanged.
- `show`: renders leaf path (`in_review.docs_review` dotted display form), the full configuration array, context values, pending effects, and the enabled-events statuses.
- `ls`: `--status running` excludes a cancelled instance; `--machine` and `--state` filters intersect; `all` shows everything.
- `history` paging: `--from-seq`/`--limit` return exactly the expected seq window in order; `--trace` includes the recomputed trace per entry.
- `explain --seq N` of a past applied event renders a trace string equal to the one printed at apply time (traces are derived, never journaled — equality proves it).

- **Done when:** inline instance-command tests prove rejection-with-hint rendering, duplicate-request short-circuit, stamping, and explain-recomputation, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
