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
  - crates/fsm-cli/tests/fixtures/executor/handlers.json
  - crates/fsm-cli/tests/fixtures/executor/session.expected.txt
  - crates/fsm-cli/tests/fixtures/executor/stub_ok.sh
status: planned
merged_as: ""
---
# Golden Two Process Session

Fixtures-first golden session that makes "a workflow runs unattended" byte-checkable: a writer drives a machine into a pending effect, the executor (not a chat turn) observes, runs a stub handler, acks, and advances to terminal — the abstract tick trace byte-compared, with derived `request_id`s visible proving idempotency is engaged.

**Steps:**

1. Author the fixtures first: `machine.json` (a machine that on entering a state emits one effect and offers the handler-advance as a declared event, modeled on `order_lifecycle`'s `request_confirmation` with `success_event`/`failure_event` declared); `handlers.json` (a `fsm.handlers/1` table mapping that effect to `stub_ok.sh`); `stub_ok.sh` (prints a fixed stdout line, exits 0); and `session.expected.txt` (the hand-derived ordered action lines, one per tick step).
2. Implement `crates/fsm-cli/tests/executor_session.rs`: open a temp-dir `Store` under `FixedClock`; define the machine, create the instance, send the event that emits the effect — all via the store directly (this is the "writer" half, playing the role a chat session would).
3. Then, with no further writer interaction, drive `fsm_execute::service::tick` against a fresh watcher/scheduler/runner/pipeline built from `handlers.json` and the stub, under the same `FixedClock`, collecting each tick's returned action lines.
4. Byte-compare the collected lines to `session.expected.txt`. The expected stream must show, in order: the pending effect observed; the handler spawned; the `ok` ack with `request_id=exec-ack-…`; the advance `send_event` with `request_id=exec-ev-…`; and the instance reaching terminal.
5. Drive the whole thing tick-by-tick (no `thread::sleep`) so the golden is deterministic and wall-clock-free.

**Tests:**

- The byte-comparison passes: the executor halves (observe/ack/advance) appear exactly as authored, including the derived `exec-ack-` and `exec-ev-` request ids.
- The journal verifies clean afterward (reuse the existing verify path on the temp dir).
- `instance_history` on the now-terminal instance shows `effect_acked` before the advance `event_applied` and the emitted effect's ack payload contains the stub's stdout.
- No-chat-half determinism: re-running the test from a fresh dir produces the identical `.expected` stream (the writer half is the only scripted part; the executor half is emergent from the loop).

- **Done when:** `cargo test -p fsm-cli --test executor_session` byte-compares the golden session, the journal verifies, history shows ack-before-advance with derived ids, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
