---
id: simulate-and-enabled-events
title: "Simulate And Enabled Events"
workstream: "0047"
kind: task
depends_on:
  - reactive-analysis-and-diagram
  - done-state-events
  - macrostep-record-shape
gated: false
touches:
  - crates/fsm-core/src/simulate.rs
  - crates/fsm-core/src/analyze/enabled_events.rs
  - crates/fsm-cli/src/mcp/tools/handlers/simulate.rs
  - crates/fsm-cli/src/mcp/tools/schema_common.rs
  - crates/fsm-store/src/store/view.rs
  - crates/fsm-core/tests/simulate_runs.rs
  - crates/fsm-core/tests/enabled_events.rs
  - crates/fsm-cli/tests/fixtures/transcripts/skeleton.out.jsonl
  - docs/SPEC.md
status: done
merged_as: ""
---
# Simulate And Enabled Events

`simulate` is where an author discovers that one event caused five transitions, and `enabled_events` is where a caller decides what to send — the first must show the cascade and the second must refuse to guess at it.

**Steps:**

1. In `crates/fsm-core/src/simulate.rs`, run macrosteps like every other entry point and add the microstep list to each per-event report, so a simulated run shows the full cascade each event caused. This is the feature's primary authoring affordance and the reason `simulate` exists.
2. Keep `simulate`'s existing contract otherwise unchanged: it records nothing, it does not poll deadlines, and `on_reject: stop | continue` behaves as before — with "reject" now meaning the whole macrostep rejected, atomically.
3. In `crates/fsm-core/src/analyze.rs`'s `enabled_events`, keep the meaning **exactly** as it is: which declared events, sent now, would select a transition in the trigger microstep. Do **not** attempt to predict the cascade. Predicting would mean running a speculative macrostep per declared event under a scan budget, and the honest answer to "what will happen" is `simulate`. Put that sentence in the code.
4. Exclude events declared `internal: true` from `enabled_events` entirely — not reported as `disabled`, but absent — and surface them through the `internal_events` list `4702` added. A caller reading `enabled_events` is deciding what to send, and an event they can never send is noise.
5. Exclude generated `$done.*` names from `enabled_events` for the same reason; they are never sendable.
6. In `crates/fsm-cli/src/mcp/tools/handlers/simulate.rs`, carry the per-event microstep list into the tool's structured output using the shape `4601` fixed for records, so a model sees one vocabulary for cascades across `simulate`, `instance_history`, and `explain`.

**Tests:**

- `crates/fsm-core/tests/simulate_runs.rs`: simulating one event against a cascading machine reports the trigger plus every reaction microstep, and the final configuration equals what a real `step` would produce.
- A simulated macrostep that hits `run/microstep_limit` reports the rejection, and `on_reject: stop` halts the run there while `continue` proceeds to the next event.
- `simulate` still polls no deadlines and records nothing, for a reactive machine as for any other.
- `crates/fsm-core/tests/enabled_events.rs`: an internal event never appears in `enabled_events`, in any status; a `$done.*` name never appears.
- `enabled_events` for a state whose only exit is eventless reports **no** enabled events — correct, because no event the caller can send selects anything — and the accompanying analysis makes the eventless exit visible instead.
- `enabled_events` output for a non-reactive machine is byte-identical to the committed goldens.
- The enabled-event scan still runs on the standard `MAX_EVAL_TICKS` budget, not the macrostep budget, and a machine at the admission ceiling does not exhaust it.
- The `simulate` tool's structured output validates against its declared output schema.

- **Done when:** `cargo test -p fsm-core --test simulate_runs` and `--test enabled_events` pass with cascades reported and internal/generated events excluded, non-reactive goldens are unchanged, the scan budget is unchanged, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.

**Landed:** `simulate.rs` budgets each event at `MACROSTEP_EVAL_TICKS` — the step it calls already runs a macrostep — and its module comment says what that makes visible; the `simulate` tool copies `record::microsteps_value` of each applied step's trace into the report step, absent when there was no reaction, so the cascade vocabulary is the one `instance_history` and `explain` use, and `schema_common.rs` declares it. `analyze/enabled_events.rs` skips events declared `internal` (absent, never `disabled`) with the sentence step 3 asked for beside the loop; generated names were never declared, so they never appeared. The instance view lists `internal_events` beside `enabled_events` when a machine declares any, declared in the same schema, and its scan keeps the standard budget with a comment saying why. SPEC's scan paragraph states the exclusions. Tests: `simulate_runs.rs` pins the cascade report against a real `step`, the ceiling rejection under both `on_reject` modes, and that no deadline fires; `enabled_events.rs` pins the exclusions, the eventless-only exit reporting nothing enabled while `reactive_summary` shows the exit, and the standard budget; the existing `enabled_events` and view goldens pin plain machines. The MCP skeleton transcript was regenerated for the two schema lines (`tools/list` is 20 345 bytes under the 21 000 ceiling 4702 set).
