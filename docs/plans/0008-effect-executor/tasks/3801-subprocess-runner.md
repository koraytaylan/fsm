---
id: subprocess-runner
title: "Subprocess Runner"
workstream: "0038"
kind: task
depends_on:
  - deterministic-scheduler
gated: false
touches:
  - crates/fsm-execute/src/run.rs
  - crates/fsm-execute/tests/run.rs
  - crates/fsm-execute/tests/fixtures/stub_handler.sh
status: planned
merged_as: ""
---
# Subprocess Runner

The runner is the only component that spawns processes. It is deliberately thin — start a `Start` directive, stop a `Kill`, report a `RunOutcome` — with no success/failure policy (that is the ack pipeline's job), no shell, and a guarantee that every spawned child is reaped or killed.

**Steps:**

1. Implement `const ACK_OUTPUT_CAP: usize = 4096` and `struct BoundedBytes { bytes: Vec<u8>, truncated: bool, sha256: Option<String> }` capturing at most `ACK_OUTPUT_CAP` bytes; when output exceeds the cap, store the first 4096 bytes, set `truncated`, and record the hex SHA-256 (via `fsm_core`) of the full stream so the journal keeps a tamper-evident digest of large output without storing it (mirrors SPEC §Payload size).
2. Implement `enum RunOutcome { Completed { status: i32, stdout: BoundedBytes, stderr: BoundedBytes }, TimedOut, SpawnFailed }`.
3. Implement `Runner { children: BTreeMap<String, Child> }`; `fn spawn(&mut self, effect_id: String, argv: &[String]) -> Result<(), ExecError>` runs `Command::new(&argv[0]).args(&argv[1..]).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()`, records the child, and returns `exec/spawn` (naming `argv[0]`) on io error. No shell, no `exec`-family call.
4. Implement `fn poll(&mut self, effect_id: &str) -> Option<RunOutcome>` non-blocking: `try_wait`; on exit, drain both pipes into `BoundedBytes` and return `Completed { status: code, ... }`; the only place `try_wait` is called.
5. Implement `fn kill(&mut self, effect_id: &str) -> RunOutcome` — `child.kill()` then reap; returns `TimedOut`. Ensure no zombie: every spawned child is either reaped via `poll` or killed via `kill`, asserted in tests.

**Tests:**

- Spawn the fixture `stub_handler.sh` (prints a fixed line to stdout, a second to stderr, exits 0): `poll` returns `Completed { status: 0 }` with both streams captured byte-exact, `truncated == false`.
- Spawn a stub that exits 3 → `Completed { status: 3 }`.
- Spawn a non-existent binary → `spawn` returns `exec/spawn` naming the binary; no child recorded.
- Kill: spawn a stub that sleeps, `kill` it, assert `RunOutcome::TimedOut` and that the child is reaped (subsequent `poll` returns `None`; no lingering process observable via `try_wait` on a retained handle returning `Ok(Some(..))` post-kill).
- Output cap: a stub printing more than 4096 bytes → `truncated == true`, `bytes.len() == ACK_OUTPUT_CAP`, and `sha256` equals an independently computed digest of the full fixture output (fixtures commit the expected full output and digest).
- No zombie: after `Completed` and `kill` paths, the runner's `children` map no longer holds the entry.

- **Done when:** `cargo test -p fsm-execute --test run` passes every row including the output-cap digest and no-zombie assertions, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
