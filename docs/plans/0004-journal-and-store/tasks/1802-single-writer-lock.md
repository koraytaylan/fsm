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
4. Write the inline test module encoding exactly the inventory under **Tests**.

**Tests:**

- Inline in `journal_io.rs` — mutual exclusion: while the first handle holds the lock, a second acquisition attempt through a separately opened `File` fails with `JournalIoError::Locked { pid }` whose `pid` equals the current process id, and whose rendered message is exactly "another process owns this store (pid N)" with N substituted.
- Metadata: after a successful acquisition, `journal/LOCK` contains a single JSON line with `pid` equal to the current process id and a `started_ts` field.
- Reacquisition: after dropping the first handle (releasing the lock), a fresh acquisition succeeds and rewrites the metadata (old contents fully truncated — no trailing bytes from a longer previous line).
- Documentation presence (review, not code): the module docs state the advisory-release rationale and that pid metadata is never trusted for liveness — checked by the reviewer against step 3, since no mechanical test can assert prose.

- **Done when:** inline lock tests prove mutual exclusion and clean reacquisition after release, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
