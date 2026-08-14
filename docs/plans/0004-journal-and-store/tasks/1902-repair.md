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
3. Add `crates/fsm-cli/tests/repair.rs` encoding exactly the inventory under **Tests** (each case runs against a temp *copy* of its committed fixture, never the fixture itself).

**Tests:**

- `repair.rs` — torn-tail repair on a copy of `torn_tail/`: the quarantine file exists at exactly `journal/quarantine/<segment>-tail-<first_bad_seq>.bin`; its bytes are byte-equal to the tail that was removed from the segment; the returned `RepairReport { quarantined, bytes, truncated_to_seq }` matches the fixture's pinned values; and a subsequent `open` classifies `Ok` with the folded state equal to the fixture's known clean prefix.
- Evidence-before-truncation: after repair, segment bytes + quarantine bytes reassemble the original segment byte-for-byte (nothing was lost, only relocated).
- Healthy journal (copy of `clean/`): `RepairError::NothingToRepair`, and the directory tree is byte-identical before and after (no quarantine dir created, nothing touched).
- Interior corruption (copy of `interior_flip/`): `RepairError::Interior(ChainBroken { .. })`, and the directory tree is byte-identical before and after — repair refuses without partial action.
- Idempotence: running repair twice on the torn-tail copy — the second run returns `NothingToRepair` and leaves the first run's quarantine file untouched.

- **Done when:** `cargo test -p fsm-cli --test repair` proves quarantine-then-truncate on the torn-tail fixture (with the quarantined bytes byte-equal to the removed tail) and refusal on interior corruption, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
