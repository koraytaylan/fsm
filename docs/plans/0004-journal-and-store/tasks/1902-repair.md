---
id: repair
title: "Repair"
workstream: "0019"
kind: task
depends_on:
  - open-and-verify
gated: false
touches:
  - crates/fsm-cli/src/journal_io.rs
  - crates/fsm-cli/tests/repair.rs
status: planned
merged_as: ""
---
# Repair

The only permitted repair is truncating a torn tail, and only after quarantining the torn bytes as evidence; interior history is never rewritten under any circumstances.

**Steps:**

1. Implement `repair_truncate_torn_tail(dir) -> Result<RepairReport, RepairError>` in `crates/fsm-cli/src/journal_io.rs`: take the exclusive lock, re-classify, and only on `TornTail` copy the torn bytes to `journal/quarantine/<segment>-tail-<first_bad_seq>.bin` (directory created and fsynced) **before** truncating the segment to the last valid record, then fsync file and directory.
2. Return `RepairError::NothingToRepair` on a healthy journal and `RepairError::Interior(JournalHealth)` on interior corruption — with no partial action taken in either case.
3. Add `crates/fsm-cli/tests/repair.rs`: repairing a copy of the `torn_tail` fixture quarantines the exact torn bytes and the store then opens clean; the `interior_flip` fixture is refused untouched.

- **Done when:** `cargo test -p fsm-cli --test repair` proves quarantine-then-truncate on the torn-tail fixture (with the quarantined bytes byte-equal to the removed tail) and refusal on interior corruption, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
