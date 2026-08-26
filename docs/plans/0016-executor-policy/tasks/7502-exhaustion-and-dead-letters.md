---
id: exhaustion-and-dead-letters
title: "Exhaustion And Dead Letters"
workstream: "0075"
kind: task
depends_on:
  - backoff-schedule
gated: false
touches:
  - crates/fsm-execute/src/service.rs
  - crates/fsm-cli/src/cli/execute.rs
  - crates/fsm-execute/tests/dead_letters.rs
status: planned
merged_as: ""
---
# Exhaustion And Dead Letters

Exhaustion is a failure like any other from the machine's point of view, so `on_failed` still fires — and the dead-letter report exists for the handlers that declare no failure path and would otherwise stall invisibly.

**Steps:**

1. In `crates/fsm-execute/src/service.rs`, ack `failed` through the **ordinary** path when the final attempt fails, with `result.error = "exec/retries_exhausted"` and `result.attempts` naming the count. Do not add a terminal state or a new record kind: the ack is already the terminal fact.
2. Confirm the machine's `on_failed` advance still fires. A definition that models a failure path must keep working with no change, and this is the property that makes retry safe to add to an existing deployment.
3. Confirm a handler with **no** `on_failed` still stalls deliberately, exactly as plan 0008 documented for an undeclared failure. That behaviour is unchanged and is precisely why the report below exists.
4. Implement the dead-letter report as a **derivation over the journal**, storing nothing: every effect acked `failed` whose result carries the exhaustion cause, with its instance, effect name, attempt count, and last capture. A dead-letter queue with its own state would be a second source of truth about what happened, and the journal already knows.
5. Surface it as `fsm execute --list-dead` in `crates/fsm-cli/src/cli/execute.rs` and as a `dead_letters` field on the executor's status output, both reading through `Store::open_read_only` so the report is safe against a running executor.
6. Support `--list-dead --since <seq>` so an operator can ask what has died since they last looked, rather than re-reading the whole history each time.
7. Emit one `log()` line at exhaustion, identifiers only, naming the effect, the instance, and the attempt count — the moment an operator most needs to see in a tick trace.

**Tests:**

- `crates/fsm-execute/tests/dead_letters.rs`: a handler with `attempts: 3` whose every attempt fails produces exactly 2 `effect_attempted` records and one `effect_acked` with `outcome: "failed"` and the exhaustion cause.
- The machine's `on_failed` event fires after exhaustion, advancing the instance.
- A handler with no `on_failed` leaves the instance where it was, and the effect is cleared by the ack.
- `--list-dead` lists exactly the exhausted effects with instance, effect name, attempt count, and last capture; a store with none reports none.
- `--list-dead --since <seq>` bounds the report correctly.
- An effect that failed **without** exhausting — a class not in `on` — is acked failed but does **not** appear in the dead-letter report; assert this, since the two failures look alike from a distance.
- An effect that succeeded on attempt 2 produces one attempt record, one ack with `outcome: "ok"`, and no dead-letter entry.
- The report reads through `open_read_only` and takes no lock — assert it works while the executor holds the writer.
- The exhaustion log line carries identifiers only.

- **Done when:** `cargo test -p fsm-execute --test dead_letters` passes every case above, exhaustion still fires `on_failed`, the report is derived and stores nothing, a non-exhausted failure is excluded from it, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
