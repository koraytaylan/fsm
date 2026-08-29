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
  - crates/fsm-store/src/archive.rs
  - crates/fsm-core/src/record/body_shape.rs
  - crates/fsm-core/src/record.rs
  - crates/fsm-core/tests/seal_record.rs
  - crates/fsm-store/tests/archive_manifest.rs
  - crates/fsm-store/tests/fixtures/archive_manifest_v1.json
  - crates/fsm-store/tests/archive_operation.rs
status: done
merged_as: ""
---
# Archive Operation

The ordering is the durability contract, so it lives in one function read top to bottom, and every prefix of it leaves a store that opens.

**Steps:**

1. Create `crates/fsm-store/src/store/seal.rs` with the operation, and export it from `crates/fsm-store/src/store/mod.rs`. Name it for what it does to disk (`seal_and_archive`), not for what it prepares.
2. Perform the steps in exactly this order, in one function, with the numbered comments kept — `CONTRIBUTING.md` requires a crash-safe sequence to be readable top to bottom in one place, and this is that sequence:
   1. Take the writer lock; refuse a read-only store with the existing `io/write`.
   2. **Establish the cut.** With no `--before-seq`, append a `state_checkpoint` and then `Journal::force_rotate`, and let `sealed_through_seq` be that checkpoint's sequence. With `--before-seq N`, load and verify, then refuse unless the record at `N` is a `state_checkpoint` **and** the last record of its segment.
   3. Compute the base state at the cut — **folding from the existing base when there is one**, not from an empty state, and only over the records above the base's own sequence. After one seal the live journal is a suffix, and folding it from `StoreState::default()` produces a base missing everything the first seal put there. Nothing catches it on its own: no fold checks for a genesis record, a `journal_sealed` record applies as a no-op, and a suffix that is only that record folds *successfully* to an empty state whose roots then agree with themselves forever. Ask `chain_start` whether the journal begins above the origin, exactly as the loader does, so an inert `BASE` from an interrupted earlier attempt cannot poison the next seal. Take `definition_limits` from that base's header for the same reason: genesis is below every cut, so after one seal the journal can no longer answer. Then run `seal_safety` for the carry rule and `seal_pin` for the live-derivation pin, and refuse with `store/archive_refused` if either says no — the hint naming the size remedy or the highest admissible cut, whichever applies.
   4. Write `MANIFEST` into the archive directory; `fsync` the file, and the directory on Unix.
   5. **Copy** each sealed segment into the archive, `fsync` each, then read each copy back and check its digest against the manifest.
   6. Write the new base to **`BASE.pending`** durably, and `fsync` the journal directory. **Not to `BASE`.** On a second seal that file holds the base the *previous* seal committed, and overwriting it before the new seal record exists destroys the only thing making the store openable: a crash in that window left a directory whose base named a cut no record in the chain named, which no reader can resolve, and the store did not open at all. The pending file is inert because nothing in the chain names it.
   7. **Append the seal record.** This is the commit point.
   8. Promote the pending base with an atomic `rename`, then `fsync` the directory. A crash between 7 and here leaves the store open-able at the *previous* seal with this run's bytes inert beside it — the same shape as an interruption before the commit point.
   9. Remove the copied segments from the live journal; `fsync` the directory.
   10. Drop every snapshot cache at or below the seal, which can no longer be validated against records that are present, and bring this handle's own record set down to what the directory now holds — so a store that has just sealed answers exactly as one reopened after the same seal, rather than keeping an `explain` and a `verify` that speak for records that are gone.
3. Understand why step 2 **creates** the cut rather than searching for one, and say so in a comment. **It does both, and the plan's rule that a cut must be a `state_checkpoint` had to give.** Only a pending effect pins the cut, and a live store almost always has one — the executor settles each within a tick, so at any instant a few are in flight with their emitting records near the head. Requiring a checkpoint cut would then refuse every seal a running store ever asked for, which is the mistake the first shape of the carry rule made and was discarded for. So the cut is a **segment boundary**: the operation creates a fresh one at the head (checkpoint plus `force_rotate`, which is the better cut and costs nothing) when the pin allows it, and otherwise seals through the highest boundary that already exists below the pin. Sealing whole segments is the natural granularity anyway, since a segment the cut fell inside could only be archived by splitting it.

   Two consequences follow and both are recorded rather than discovered. A seal taken below the head is **not adjacent** to the prefix it seals, so `7901`'s join assertion — `sealed_last_hash` equals the seal's own `prev` — is asserted only when `sealed_through_seq + 1` equals the seal's own sequence, and otherwise the body-shape check requires only a well-formed hash while the chain and the base file check the value. And `8101`'s rule that the first live record **is** the seal holds only for a head cut; the general rule is that the first live record's `prev` equals the base's `last_hash` and the seal is the first `journal_sealed` record in the live suffix.

   The manifest gained a `first_prev_hash` for the same reason: a store's second archive does not begin at genesis, so its chain walk needs a predecessor it can check — and recording it lets whoever holds two archives of one store chain them together. A valid cut must satisfy two conditions at once: its record is a `state_checkpoint`, so the base derives from proven state; and it is the last record of a segment, because `should_rotate` fires on size and a segment the cut falls inside could only be archived by splitting it — which means rewriting published bytes, which this project never does. Nothing produces a sequence meeting both by chance, so the operation produces one from two primitives that already exist.
4. Write the safety argument as a comment above the sequence: **copy, then seal, then remove** — and *promote* the base only after sealing, which is what makes the argument hold for the second seal as well as the first. Before step 7 nothing in the chain references any of the new files, so an interrupted run leaves inert bytes a re-run overwrites. After step 7 the removed segments are already in the archive and their records are below the seal, so the loader skips them by sequence and a re-run finishes the removal. An implementation that moves segments before appending the seal has a window where the records are gone and nothing says they were sealed, and a store interrupted in that window never opens again.
5. `--before-seq N` is an **assertion**, not a choice: it names the sequence the seal will seal through, exactly as `--dry-run` reported it, and the operation refuses if the answer has moved since. That is `expect_seq`'s pattern, and it is what stops a preview and a run from disagreeing about which prefix moved. Choosing a lower cut by hand is not offered, because the cut is determined by the pin and the segment boundaries and a hand-picked one would have to be re-validated against both anyway.
6. Refuse when the archive directory does not exist rather than creating it. An operator who mistypes a path should not discover a new directory holding their history.
7. Take the same position on Windows the store already takes: there is no portable directory `fsync`, the gap is classified and repaired on the next open rather than trusted, and this operation adds no new platform assumption. State it in a comment rather than inventing a mitigation.
8. Do not delete anything from the archive on any failure path. A partial archive is inert; a partially deleted one is not recoverable.

**Tests:**

- `crates/fsm-store/tests/archive_operation.rs`: a successful seal produces a manifest, copied segments whose digests match, a `BASE` whose roots match the seal, a live journal beginning with the seal record, and no segments below the cut.
- With no `--before-seq`, the operation creates its own cut: a `state_checkpoint` is appended, the segment is rotated, and the seal lands in a fresh segment. Assert the checkpoint is the last record of its segment.
- An explicit `--before-seq` naming a checkpoint that is **not** segment-final is refused, and the hint says omitting the flag seals at the head.
- An explicit `--before-seq` naming a record that is not a `state_checkpoint` is refused, and the error says so.
- A seal point created by one run is a valid `--before-seq` for a later run against a second archive directory.
- **A second seal interrupted before its commit point leaves the first seal intact**, and the store opens at it. This is the case the original ordering got wrong, and it is invisible to any test that only seals once.
- **A second seal interrupted before its commit point leaves the first seal intact**, and the store opens at it. This is the case the original ordering got wrong, and no test that seals only once can see it.
- **A second seal on a store reopened between the two keeps every machine and every instance.** Sealing twice on one in-memory handle proves nothing here — that handle still holds the full record set — and a suite that only does that would pass an implementation which silently empties the store. Assert the counts across a real reopen.
- A second seal carries the historical definition ceiling forward, asserted on the base header before and after.
- A preview on a store that has already been sealed counts only the records it would archive, not `cut + 1`: preview and run must report the same number, and the preview's count must not exceed what the live journal holds.
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
