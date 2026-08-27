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
  - crates/fsm-execute/src/run.rs
  - crates/fsm-execute/src/sched.rs
  - crates/fsm-execute/src/error.rs
  - crates/fsm-execute/src/rid.rs
  - crates/fsm-execute/src/dead.rs
  - crates/fsm-execute/src/lib.rs
  - crates/fsm-cli/src/cli/execute.rs
  - crates/fsm-execute/tests/dead_letters.rs
  - crates/fsm-execute/tests/run.rs
  - crates/fsm-execute/tests/pipeline.rs
  - crates/fsm-cli/tests/execute_cmd.rs
  - docs/EMBEDDING.md
status: done
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

**Landed:** The retry decision is taken at **settle time**, in `service::settle`, because only a finished run knows how it failed and only the journal knows how many times it already has. A failure whose class the handler retries, with budget left, is journaled as an `effect_attempted` record and left pending for the next scan; the last one is acked `failed` through the ordinary path. Nothing else changed shape: no terminal state, no new record kind, and `on_failed` fires exactly as it did before retry existed.

The exhausted `result` is the last run's capture with three keys laid over it — `error` set to `exec/retries_exhausted`, `attempts` naming the count, and `class` preserving the cause `error` was carrying before exhaustion took its place. Without `class` a timeout and a non-zero exit would be indistinguishable after the fact, since `error` is the only field a `Killed` outcome had. All three derive from the journal and the outcome, so a re-issued ack rebuilds the same bytes and still replays.

A handler with **no** `on_failed` still stalls, exactly as plan 0008 documented, and the ack still clears the effect — so the instance sits in place with nothing in its outbox to say why. That is precisely what the report is for, and the suite asserts the shape rather than describing it.

The report is a derivation over `effect_acked` records whose result carries the exhaustion cause, memoizing the one expensive part (re-deriving an effect's name costs a prefix fold). It stores nothing: a dead-letter queue with its own state would be a second source of truth about what happened to an effect, and would drift from the first the moment one of them was pruned, restored, or replayed. `fsm execute --list-dead`, its exclusive `--since <seq>` bound, and the `dead_letters` field on the `--check` pre-flight all read through `Store::open_read_only`, which takes no lock — asserted against a live writer on both surfaces.

`--check` is the executor's status output, so that is where `dead_letters` went: "your table is valid" is only half of what a pre-flight owes an operator when an effect exhausted under the previous run. A data directory that does not exist yet reports none rather than failing, because a journal that does not exist provably holds no dead letters; one that exists and cannot be read is a real fault and is reported as one, since the executor could not have run against it either.

**Corrections.** Two, both discovered here.

The start rule skipped an effect with `attempt > handler.retry.attempts` in silence, on the strength of a comment saying "the ack path takes over". It does not: the ack path only runs when a *run* finishes, and this state is unreachable through the ordinary flow — the last attempt is acked rather than journaled, so a three-attempt handler leaves two records. It is reachable only by lowering a table's `attempts` while an effect is part way through, and the effect was then stranded with no diagnostic anywhere. Acking it would mean journaling a failure this process never observed, so it is now reported once as the stall it is, through the machinery that already exists for the other unstartable case.

`attempt_rid` had been inserted **inside** `event_rid`'s doc comment in 7403, which left `event_rid` undocumented and gave `attempt_rid` a first paragraph describing the wrong key. Both now say what they are.

Test-only: `Pipeline::settle` gained the exhaustion argument, so `pipeline.rs`'s thirteen call sites pass `None` — stated explicitly rather than defaulted, because "this settle is not an exhaustion" is a claim each row should make.
