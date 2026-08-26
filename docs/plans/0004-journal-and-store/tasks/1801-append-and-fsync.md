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
status: done
merged_as: ""
---
# Append And Fsync

The journal writer is the sole commit point in the system: one canonical line per record, fsynced to the file before any response and to the directory after any segment change, with write failures poisoning the journal rather than risking divergence.

**Steps:**

1. Add `pub mod journal_io; pub mod store; pub mod clock;` to `crates/fsm-cli/src/lib.rs` (the crate's library target, established in plan 0001), creating `store.rs` as a stub (filled by workstream 0020).
2. Implement `clock::now_ms()` in `crates/fsm-cli/src/clock.rs` — the only wall-clock read in the system, with the `FSM_CLOCK_MS` deterministic mode (fixed start plus per-call increment) for byte-stable test transcripts.
3. Implement `Journal { init, append }` in `crates/fsm-cli/src/journal_io.rs` per architecture: genesis on init (pinning `fsm.journal/1` and the limits table), append-only segments `journal/seg-<first_seq, 20-digit zero-padded>.jsonl`, `File::sync_all` before returning from every append, directory fsync after segment creation, rotation at 64 MiB or 65,536 records with the decision extracted as a pure `fn should_rotate(seg_bytes: u64, seg_records: u32) -> bool`, and poisoned-on-failure semantics making later appends fail fast.
4. Write the inline test module encoding exactly the inventory under **Tests**.

**Tests:**

- Inline in `journal_io.rs` — init: a fresh dir gains `journal/seg-00000000000000000000.jsonl` containing exactly the genesis line (seq 0, `prev` of sixty-four `0`s, body with `format` and the limits table), LF-terminated, byte-canonical.
- Durability ordering: after `append` returns, re-reading the segment through a *fresh* file handle already shows the complete line — the record is on disk before the caller can respond.
- Line discipline: every appended line equals its canonical re-serialization and contains no interior newline; `last_seq`/`last_hash` advance to the sealed record's values.
- Rotation, tested at the decision level (65,536 fsynced appends would be needlessly slow in-test — the decision function carries the thresholds): `should_rotate` is false at 65,535 records and true at 65,536, false at 64 MiB − 1 and true at 64 MiB; plus one direct invocation of the rotation routine on a small journal asserting the new segment is named `seg-<next_seq>` zero-padded to 20 digits and the old segment is left closed and unmodified.
- Poisoning: with the flag set directly, `append` fails fast with the poisoned error without touching disk (segment bytes unchanged); `#[cfg(unix)]` — after making the journal directory read-only, a real `append` fails, sets `poisoned`, and the *next* append fails fast even after permissions are restored.
- Clock: with `FSM_CLOCK_MS=5000`, two appends carry `ts` 5000 and 5001 (fixed start, per-call increment), and a repeat run produces identical bytes; without it, `now_ms` is non-decreasing across calls.

- **Done when:** inline `journal_io` tests prove genesis, per-record durability ordering (append returns only after `sync_all`), rotation at the configured thresholds, and fail-fast poisoning, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
