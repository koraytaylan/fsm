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
status: done
merged_as: ""
---
# Subprocess Runner

The runner is the only component that spawns processes. It is deliberately thin — start a `Start` directive, stop a `Kill`, report a `RunOutcome` — with no success/failure policy (that is the ack pipeline's job), no shell, and a guarantee that every spawned child is reaped, killed, or an acknowledged orphan of a killed executor.

**Steps:**

1. Implement `const ACK_OUTPUT_CAP: usize = 4096` and `struct BoundedBytes { bytes: Vec<u8>, truncated: bool, sha256: Option<String> }` capturing at most `ACK_OUTPUT_CAP` bytes; when output exceeds the cap, store the first bytes up to the cap, set `truncated`, and record the hex SHA-256 (`fsm_core::sha256::sha256` + `to_hex`) of the full stream so the journal keeps a tamper-evident digest of large output without storing it (mirrors SPEC §Payload size).
2. Implement `BoundedBytes::to_json_string()`: truncate at or below the cap **on a UTF-8 character boundary**, then `String::from_utf8_lossy`. Handler output is arbitrary bytes and a record body is canonical JSON; this is the only conversion, and it must never fail.
3. Implement `enum RunOutcome { Completed { status: i32, stdout: BoundedBytes, stderr: BoundedBytes }, Killed { reason: KillReason }, SpawnFailed { argv0: String } }` with `enum KillReason { Timeout, Cancelled }` — a timeout and a cancel are different facts about the run and the ack records which. Give `RunOutcome` a `fn ack_result(&self) -> Value` producing the deterministic payload the ack fingerprints: `Completed` → `{"status", "stdout", "stderr"}`; `Killed` → `{"status": -1, "error": "exec/timeout" | "exec/cancelled"}`; `SpawnFailed` → `{"status": -1, "error": "exec/spawn", "argv0"}`. No timestamp, duration, or pid may enter it — the store fingerprints the ack over this object, so anything that varies between the write and a later re-issue turns a replay into a conflict.
4. Implement `Runner { scratch: PathBuf, children: BTreeMap<String, Child> }`. **Capture to files, not pipes**: a piped child that writes past the OS pipe buffer (~64 KiB) blocks until someone reads, and this runner only reads after `try_wait` reports exit, so a chatty handler would hang until its timeout — and the output-cap rule guarantees somebody eventually writes that much. Draining incrementally would need a reader thread per stream; a file needs neither. Create one scratch dir at startup (`std::env::temp_dir().join(format!("fsm-exec-{pid}"))`); `fn spawn(&mut self, effect_id: String, argv: &[String]) -> Result<(), ExecError>` runs `Command::new(&argv[0]).args(&argv[1..])` with `.stdout(File)`/`.stderr(File)` over `<scratch>/<effect_id with '/' → '-'>.out|.err`, records the child, and returns `exec/spawn` (naming `argv[0]`) on io error. No shell, no `exec`-family call.
5. Implement `fn poll(&mut self, effect_id: &str) -> Option<RunOutcome>` non-blocking: `try_wait`; on exit, read the two capture files into `BoundedBytes` (first `ACK_OUTPUT_CAP` bytes kept, whole file streamed through the hasher when longer), delete them, and return `Completed { status: code, ... }`; the only place `try_wait` is called. A child killed by a signal, which has no exit code, reports `Completed { status: -1, .. }` so the pipeline acks it `failed` rather than panicking on `None`.
6. Implement `fn kill(&mut self, effect_id: &str, reason: KillReason) -> RunOutcome` — `child.kill()` then reap; returns `Killed { reason }`. Implement `impl Drop for Runner` killing and reaping every remaining child and removing the scratch dir, so a clean shutdown leaves nothing behind; document in the same place that no *signalled* shutdown runs `Drop` — not `kill -9`, and not Ctrl-C, since Rust's default handler terminates without unwinding — so the orphaned handler keeps running and the next executor starts a fresh one rather than adopting it. That is the plan's at-least-once boundary, stated where the code makes it true.

**Tests:**

- The stub handler is this test binary re-executed with a marker argument (`std::env::current_exe()`, the `crash_harness.rs` precedent) — never a `.sh` fixture, because CI runs the whole suite on Windows as a full test leg. An early return at the top of the file plays the stub: print a fixed stdout line and a fixed stderr line, exit with the code the marker requests.
- Spawn the stub with the exit-0 marker: `poll` returns `Completed { status: 0 }` with both streams captured byte-exact, `truncated == false`.
- Spawn the stub with the exit-3 marker → `Completed { status: 3 }`.
- Spawn a non-existent binary → `spawn` returns `exec/spawn` naming the binary; no child recorded.
- Kill: spawn the sleeping-stub marker, `kill` it with each `KillReason`, assert `RunOutcome::Killed { reason }` carries the reason it was given and that the child is reaped (the entry is gone and no zombie remains).
- `ack_result()` is deterministic: calling it twice on the same outcome yields byte-identical JSON, and the `Killed`/`SpawnFailed` forms carry the documented `error` codes and no varying field.
- Output cap: the big-output marker prints more than `ACK_OUTPUT_CAP` bytes → `truncated == true`, `bytes.len() <= ACK_OUTPUT_CAP`, and `sha256` equals an independently computed digest of the full output.
- Past the pipe buffer: a marker printing 256 KiB — well beyond any OS pipe buffer — still completes, is reaped on the first `poll` after exit, and digests correctly. This is the row that would have caught a piped implementation deadlocking, so it is not optional.
- Scratch hygiene: after a `Completed` and after a `kill`, the capture files are gone; dropping the `Runner` removes the scratch directory.
- Invalid UTF-8: the binary-output marker prints a lone `0x80` byte and a multi-byte character straddling the cap → `to_json_string()` returns a valid `String`, the truncation lands on a character boundary, and the digest still covers the true bytes.
- No zombie: after both the `Completed` and `kill` paths, the runner's `children` map no longer holds the entry; dropping a `Runner` with a live child kills and reaps it.

- **Done when:** `cargo test -p fsm-execute --test run` passes every row including the output-cap digest, the past-the-pipe-buffer row, the invalid-UTF-8 row, and the no-zombie/Drop assertions, the stub needs no shell and no exec bit, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
