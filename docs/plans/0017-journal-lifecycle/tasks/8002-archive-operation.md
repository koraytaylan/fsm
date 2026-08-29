---
id: archive-operation
title: "Archive Operation"
workstream: "0080"
kind: task
depends_on:
  - archive-manifest
gated: false
touches:
  - crates/fsm-store/src/store/seal.rs
  - crates/fsm-store/src/store/mod.rs
  - crates/fsm-store/tests/archive_operation.rs
status: planned
merged_as: ""
---
# Archive Operation

The ordering is the durability contract, so it lives in one function read top to bottom, and every prefix of it leaves a store that opens.

**Steps:**

1. Create `crates/fsm-store/src/store/seal.rs` with the operation, and export it from `crates/fsm-store/src/store/mod.rs`. Name it for what it does to disk (`seal_and_archive`), not for what it prepares.
2. Perform the steps in exactly this order, in one function, with the numbered comments kept — `CONTRIBUTING.md` requires a crash-safe sequence to be readable top to bottom in one place, and this is that sequence:
   1. Take the writer lock; refuse a read-only store with the existing `io/write`.
   2. **Establish the cut.** With no `--before-seq`, append a `state_checkpoint` and then `Journal::force_rotate`, and let `sealed_through_seq` be that checkpoint's sequence. With `--before-seq N`, load and verify, then refuse unless the record at `N` is a `state_checkpoint` **and** the last record of its segment.
   3. Compute the base state at the cut, run `seal_safety` for the carry rule and `seal_pin` for the live-derivation pin, and refuse with `store/archive_refused` if either says no — the hint naming the size remedy or the highest admissible cut, whichever applies.
   4. Write `MANIFEST` into the archive directory; `fsync` the file, and the directory on Unix.
   5. **Copy** each sealed segment into the archive, `fsync` each, then read each copy back and check its digest against the manifest.
   6. Write `BASE.tmp`, `fsync`, rename to `BASE`, `fsync` the journal directory.
   7. **Append the seal record.** This is the commit point.
   8. Remove the copied segments from the live journal; `fsync` the directory.
   9. Drop every snapshot cache at or below the seal, which can no longer be validated against records that are present.
3. Understand why step 2 **creates** the cut rather than searching for one, and say so in a comment. A valid cut must satisfy two conditions at once: its record is a `state_checkpoint`, so the base derives from proven state; and it is the last record of a segment, because `should_rotate` fires on size and a segment the cut falls inside could only be archived by splitting it — which means rewriting published bytes, which this project never does. Nothing produces a sequence meeting both by chance, so the operation produces one from two primitives that already exist.
4. Write the safety argument as a comment above the sequence: **copy, then seal, then remove.** Before step 7 nothing in the chain references any of the new files, so an interrupted run leaves inert bytes a re-run overwrites. After step 7 the removed segments are already in the archive and their records are below the seal, so the loader skips them by sequence and a re-run finishes the removal. An implementation that moves segments before appending the seal has a window where the records are gone and nothing says they were sealed, and a store interrupted in that window never opens again.
5. Refuse an explicit `--before-seq` at or above the current `last_seq`, and refuse `--before-seq 0`. Naming the head as an existing seal point is a contradiction: the head is not yet a rotated checkpoint, and omitting the flag is how an operator asks to seal there.
6. Refuse when the archive directory does not exist rather than creating it. An operator who mistypes a path should not discover a new directory holding their history.
7. Take the same position on Windows the store already takes: there is no portable directory `fsync`, the gap is classified and repaired on the next open rather than trusted, and this operation adds no new platform assumption. State it in a comment rather than inventing a mitigation.
8. Do not delete anything from the archive on any failure path. A partial archive is inert; a partially deleted one is not recoverable.

**Tests:**

- `crates/fsm-store/tests/archive_operation.rs`: a successful seal produces a manifest, copied segments whose digests match, a `BASE` whose roots match the seal, a live journal beginning with the seal record, and no segments below the cut.
- With no `--before-seq`, the operation creates its own cut: a `state_checkpoint` is appended, the segment is rotated, and the seal lands in a fresh segment. Assert the checkpoint is the last record of its segment.
- An explicit `--before-seq` naming a checkpoint that is **not** segment-final is refused, and the hint says omitting the flag seals at the head.
- An explicit `--before-seq` naming a record that is not a `state_checkpoint` is refused, and the error says so.
- A seal point created by one run is a valid `--before-seq` for a later run against a second archive directory.
- The cut is refused when `seal_safety` refuses on size, and the refusal is `store/archive_refused` with both remedies in its hint.
- The cut is refused when `seal_pin` refuses, and the hint names the highest admissible cut — including for the default head cut, where a pending effect means the operation must seal lower than the checkpoint it would otherwise create.
- With no `--before-seq` and a pin below the head, the operation seals at the highest admissible cut rather than refusing outright, and reports that it did. Sealing less is the useful answer; refusing because a workflow is mid-flight is not.
- An explicit cut at `last_seq`, above it, and at 0 is refused.
- The cut is refused against a read-only store, with nothing written to the archive directory.
- The cut is refused when the archive directory does not exist, and when it already holds a `MANIFEST`.
- After a successful seal, the sealed records are readable from the archive and verify as a chain ending in `sealed_last_hash`.
- After a successful seal, a snapshot cache at or below the seal is gone, and one above it survives.
- **Interrupting after step 4, 5, or 6 leaves a store that opens unsealed**, with the partial archive inert, and a re-run of the same command completes successfully. Assert the store's folded state is unchanged by the interrupted attempt.
- **Interrupting between step 7 and step 8 leaves a store that opens sealed**, and a re-run finishes removing the segments. Assert the sealed records are still readable from the archive in this state — a run that removed a segment it had not successfully copied would pass a live-store-only assertion.
- Two seals in sequence, to two different archive directories, produce a store sealed at the later cut whose base chains from the earlier one.

- **Done when:** `cargo test -p fsm-store --test archive_operation` passes every case above, the operation creates its own segment-final checkpoint when no cut is named, the nine steps appear in one function in that order with the safety comment, every interruption point named leaves a store that opens and a command that can be re-run, no failure path deletes archived bytes, and `cargo test`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo fmt --check` succeed.
