---
id: append-and-fsync
title: "Append And Fsync"
workstream: "0018"
kind: task
depends_on:
  - record-envelope
gated: false
touches:
  - crates/fsm-cli/src/lib.rs
  - crates/fsm-cli/src/journal_io.rs
  - crates/fsm-cli/src/store.rs
  - crates/fsm-cli/src/clock.rs
status: planned
merged_as: ""
---
# Append And Fsync

The journal writer is the sole commit point in the system: one canonical line per record, fsynced to the file before any response and to the directory after any segment change, with write failures poisoning the journal rather than risking divergence.

**Steps:**

1. Add `pub mod journal_io; pub mod store; pub mod clock;` to `crates/fsm-cli/src/lib.rs` (the crate's library target, established in plan 0001), creating `store.rs` as a stub (filled by workstream 0020).
2. Implement `clock::now_ms()` in `crates/fsm-cli/src/clock.rs` — the only wall-clock read in the system, with the `FSM_CLOCK_MS` deterministic mode (fixed start plus per-call increment) for byte-stable test transcripts.
3. Implement `Journal { init, append }` in `crates/fsm-cli/src/journal_io.rs` per architecture: genesis on init (pinning `fsm.journal/1` and the limits table), append-only segments `journal/seg-<first_seq, 20-digit zero-padded>.jsonl`, `File::sync_all` before returning from every append, directory fsync after segment creation, rotation at 64 MiB or 65,536 records, and poisoned-on-failure semantics making later appends fail fast.
4. Add inline unit tests: genesis shape, rotation boundary at the record cap, poisoned behavior after an injected write failure (via a full-disk simulation on a tiny in-test wrapper or a read-only directory), and `FSM_CLOCK_MS` determinism.

- **Done when:** inline `journal_io` tests prove genesis, per-record durability ordering (append returns only after `sync_all`), rotation at the configured thresholds, and fail-fast poisoning, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
