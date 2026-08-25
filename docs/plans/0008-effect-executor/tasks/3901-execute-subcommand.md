---
id: execute-subcommand
title: "Execute Subcommand"
workstream: "0039"
kind: task
depends_on:
  - ack-and-advance-pipeline
gated: false
touches:
  - crates/fsm-execute/src/service.rs
  - crates/fsm-cli/src/args.rs
  - crates/fsm-cli/src/cli/mod.rs
  - crates/fsm-cli/src/cli/execute.rs
  - crates/fsm-cli/Cargo.toml
  - crates/fsm-cli/tests/execute_cmd.rs
status: done
merged_as: ""
---
# Execute Subcommand

The `fsm execute` subcommand composes the `fsm-execute` library into the runnable process inside the single installable `fsm` binary — validate the handler table, then run the scan → decide → spawn → settle loop with no async runtime and a flag-tunable poll interval.

**Steps:**

1. Add `fsm-execute = { path = "../fsm-execute" }` to `crates/fsm-cli/Cargo.toml` `[dependencies]`.
2. Implement `service::run(data_dir, table, poll_interval_ms, clock) -> Result<(), ExecError>` in `fsm-execute`: the plain `loop { let lines = tick(..); emit lines; std::thread::sleep(interval) }` around task `3802`'s `tick`, with no async runtime. It lives in the library so embedded mode (task `3902`) reuses the same driver.
3. Register the subcommand the way this CLI registers subcommands: a `CmdSpec` in `crate::cli::execute::SPECS` (path `&["execute"]`, flag `--handlers` required, flag `--poll-interval-ms` default 250, switch `--check`) added to `args::all_specs()`. `main.rs` needs no edit — it is three lines calling `args::dispatch`. `--data-dir` is already a global flag with an `FSM_DATA_DIR` fallback (`args::resolve_data_dir`), so `execute` inherits it rather than declaring its own.
4. Implement `cli/execute.rs`: parse + validate the table via `fsm_execute::config::HandlerTable::parse` and **abort before opening any store** on `exec/config`; with `--check`, print the resolved handler list (effect → argv, timeout, advance events) and exit 0 without touching the data dir.
5. Otherwise call `service::run(...)`. The writer `Store` is opened inside each tick that has work and dropped before the sleep, so the executor never holds the single-writer lock across an interval; contention surfaces as `store/lock` → `exec/store`, is logged, and is retried on the next tick rather than failing the run.
6. Route every `ExecError` through the existing `fsm-cli` error-rendering path (`render.rs` human/JSON) so failures look like the rest of the CLI; map exit codes (`exec/config` → 2 ad-hoc misuse, persistent `exec/store` → 1) per the CLI's existing convention.
7. Log one startup line naming the run mode — `exclusive` here; `paired` becomes the default in task `3902` — on stderr, outside any tick's action lines, so no golden depends on it.

**Tests:**

- `fsm execute --check --handlers <valid fixture>` prints the resolved handler and exits 0, having created no data dir (assert the directory does not exist afterwards).
- `fsm execute --handlers <bad fixture>` exits before any store open with the `exec/config` error rendered and exit code 2.
- Driven loop: against a temp data dir with a machine whose instance has one pending effect and the test-binary stub handler, calling `service::tick` directly (not the sleeping loop) journals `effect_acked` then the advance `event_applied` within a bounded number of ticks. Drop the writer handle the test used to set that up **before** the first tick — `tick` opens its own writer and the lock is per data dir, not per process.
- The loop does not hold the writer lock between ticks: a second writer `Store` opened immediately after `tick` returns succeeds rather than `store/lock`-ing out.
- `--json` frames the action lines per the CLI's output-frame convention (2302 precedent); the startup mode line goes to stderr and never into that frame.

- **Done when:** `cargo test -p fsm-cli --test execute_cmd` passes the check-mode, pre-open config failure, driven-loop journaling, and inter-tick lock-release rows, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
