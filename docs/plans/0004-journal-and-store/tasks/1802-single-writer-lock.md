---
id: single-writer-lock
title: "Single Writer Lock"
workstream: "0018"
kind: task
depends_on:
  - append-and-fsync
gated: false
touches:
  - crates/fsm-cli/src/journal_io.rs
status: planned
merged_as: ""
---
# Single Writer Lock

Exactly one process may mutate a store: an advisory `journal/LOCK` held via std file locking for the process lifetime, with pid metadata for diagnostics and no stale-lock heuristics because the OS releases the lock on process death.

**Steps:**

1. In `crates/fsm-cli/src/journal_io.rs`, acquire `journal/LOCK` with `File::try_lock()` at store open and hold it for the process lifetime; on success, truncate and write `{"pid":…,"started_ts":…}`.
2. On conflict, read the pid line and return `JournalIoError::Locked { pid }`, rendered as "another process owns this store (pid N)".
3. Document in module docs why no manual stale-lock handling exists (advisory locks release on process death; the pid metadata is diagnostic only, never trusted for liveness).
4. Add inline unit tests: a second open in the same process family fails with `Locked` while the first holds the lock, and reopening after drop succeeds.

- **Done when:** inline lock tests prove mutual exclusion and clean reacquisition after release, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
