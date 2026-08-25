---
id: execute-subcommand
title: "Execute Subcommand"
workstream: "0039"
kind: task
depends_on:
  - ack-and-advance-pipeline
gated: false
touches:
  - crates/fsm-cli/src/args.rs
  - crates/fsm-cli/src/cli/mod.rs
  - crates/fsm-cli/src/cli/execute.rs
  - crates/fsm-cli/src/main.rs
  - crates/fsm-cli/Cargo.toml
  - crates/fsm-cli/tests/execute_cmd.rs
status: planned
merged_as: ""
---
# Execute Subcommand

The `fsm execute` subcommand composes the `fsm-execute` library into the runnable process inside the single installable `fsm` binary — validate the handler table, then run the scan → decide → spawn → settle loop with no async runtime and a flag-tunable poll interval.

**Steps:**

1. Add `fsm-execute = { path = "../fsm-execute" }` to `crates/fsm-cli/Cargo.toml` `[dependencies]`.
2. In `args.rs`, add the `Execute` subcommand: `--data-dir <dir>` (falls back to the existing data-dir resolution), `--handlers <file>` (required), `--poll-interval-ms <n>` (default 250), and `--check`.
3. Implement `cli/execute.rs`: on startup parse + validate the table via `fsm_execute::config::HandlerTable::parse` and **abort before opening any store** on `exec/config`; with `--check`, print the resolved handler list (effect → argv, timeout, success/failure events) and exit 0 without touching the store.
4. Otherwise run `service::run(...)`: a plain `loop { let lines = tick(...); print lines; sleep(poll_interval) }` using `std::thread::sleep`, no async. The writer `Store` is opened only for each tick's settle phase then dropped, so the executor never holds the single-writer lock across a sleep; contention surfaces as `store/lock` → `exec/store`, logged, retried next tick rather than failing the run.
5. Route every `ExecError` through the existing `fsm-cli` error-rendering path (`render.rs` human/JSON) so failures look like the rest of the CLI; map exit codes (`exec/config` → 2 ad-hoc misuse, `exec/store` persistent → 1) per the CLI's existing convention.
6. `main.rs` wires the new subcommand; a startup log line names the run mode (`exclusive` here; `paired` arrives in task `3902`) so the proof session can assert it.

**Tests:**

- `fsm execute --check --handlers fixtures/handlers/valid_min.json` prints the resolved handler and exits 0, having touched no data dir.
- `fsm execute --handlers <bad fixture>` exits before any store open with the `exec/config` error rendered and non-zero status.
- `fsm execute` against a temp data dir with a machine whose instance has one pending effect and a stub handler runs the loop and, within a bounded number of ticks (driven, not wall-clock — the test calls `service::tick` directly), journals `effect_acked` then the advance `event_applied`.
- The loop does not hold the writer lock between ticks: a second writer opening the data dir concurrently between ticks succeeds rather than `store/lock`-ing out (assert via acquiring the lock after the tick returns).
- Startup log names the mode; `--json` output frames the action lines per the CLI's output-frame convention (2302 precedent).

- **Done when:** `cargo test -p fsm-cli --test execute_cmd` passes the check-mode, pre-open config failure, driven-loop journaling, and inter-tick lock-release rows, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
