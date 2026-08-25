---
id: golden-two-process-session
title: "Golden Two Process Session"
workstream: "0040"
kind: task
depends_on:
  - ack-and-advance-pipeline
  - execute-subcommand
gated: false
touches:
  - crates/fsm-cli/tests/executor_session.rs
  - crates/fsm-cli/tests/fixtures/executor/machine.json
  - crates/fsm-cli/tests/fixtures/executor/handlers.template.json
  - crates/fsm-cli/tests/fixtures/executor/session.expected.txt
status: planned
merged_as: ""
---
# Golden Two Process Session

Fixtures-first golden session that makes "a workflow runs unattended" byte-checkable: a writer drives a machine into a pending effect, the executor (not a chat turn) observes, runs a stub handler, acks, and advances to terminal — the abstract tick trace byte-compared, with derived `request_id`s visible proving idempotency is engaged.

**Steps:**

1. Author the fixtures first. `machine.json`: a machine that on entering a state emits one effect and declares the advance event the handler will name, modelled on `order_lifecycle`'s `request_confirmation` — whose advance event declares a stamped field, so the golden exercises `on_ok.stamps` rather than pretending advance events are field-less. `handlers.template.json`: a `fsm.handlers/1` table whose `argv[0]` is the placeholder `%STUB%` plus the stub's marker argument, with `on_ok`/`on_failed` naming the machine's events. The `%…%` spelling is deliberate — `{…}` means *effect argument* to the substituter, so a `{stub}` placeholder would be looked up in the effect's args and fail at run time. `session.expected.txt`: the hand-derived ordered action lines.
2. The stub handler is **this test binary re-executed** (`std::env::current_exe()` with a marker argument, the `crash_harness.rs` precedent), never a `.sh` script — CI runs the whole suite on Windows as a full test leg. The test materializes `handlers.json` into its temp dir by replacing `%STUB%` with the resolved path, so the committed fixture stays machine-independent.
3. Implement `crates/fsm-cli/tests/executor_session.rs`: open a temp-dir `Store` under `FixedClock`; define the machine, create the instance, send the event that emits the effect — all via the store directly (this is the "writer" half, playing the role a chat session would) — then **drop that handle**. `service::tick` opens its own writer and the advisory lock is per data dir, not per process, so a still-live test handle would lock the executor out of every tick.
4. Then, with no further writer interaction, drive `fsm_execute::service::tick` against a fresh watcher/scheduler/runner/pipeline built from the materialized table, under the same `FixedClock`, collecting each tick's returned action lines.
5. Byte-compare the collected lines to `session.expected.txt`. The expected stream must show, in order: the pending effect observed by name; the handler spawned; the `ok` ack with `request_id=exec-ack-…`; the advance send with `request_id=exec-ev-…`; and the instance reaching terminal. Action lines carry identifiers only — no path, pid, temp dir, or duration ever enters the golden, which is also what keeps it stable when task `3902` changes the default run mode (the mode line is startup output, not a tick line).
6. Drive the whole thing tick-by-tick (no `thread::sleep`) so the golden is deterministic and wall-clock-free.

**Tests:**

- The byte-comparison passes: the executor halves (observe/ack/advance) appear exactly as authored, including the derived `exec-ack-` and `exec-ev-` request ids.
- The journal verifies clean afterward (reuse the existing verify path on the temp dir).
- `instance_history` on the now-terminal instance shows `effect_acked` before the advance `event_applied`, the ack payload contains the stub's stdout, and the advance payload carries the stamped field.
- No-chat-half determinism: re-running the test from a fresh dir produces the identical `.expected` stream (the writer half is the only scripted part; the executor half is emergent from the loop).
- Portability: the run uses no shell, no exec bit, and no committed absolute path — the same fixtures drive the Linux, macOS, and Windows legs.

- **Done when:** `cargo test -p fsm-cli --test executor_session` byte-compares the golden session, the journal verifies, history shows ack-before-advance with derived ids and the stamped advance payload, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
