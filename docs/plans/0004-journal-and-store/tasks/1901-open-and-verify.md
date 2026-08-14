---
id: open-and-verify
title: "Open And Verify"
workstream: "0019"
kind: task
depends_on:
  - single-writer-lock
gated: false
touches:
  - crates/fsm-cli/src/journal_io.rs
  - crates/fsm-cli/tests/recovery_classification.rs
  - "crates/fsm-cli/tests/fixtures/journals/**"
status: planned
merged_as: ""
---
# Open And Verify

Opening a store re-verifies the chain record by record — parse, byte-canonical equality, seq, prev, hash, and semantic state-hash re-application — and classifies any failure exactly: a torn tail names its repair command, interior corruption is refused with no repair offered, and nothing is ever guessed or rewritten.

**Steps:**

1. Author the committed journal directories under `crates/fsm-cli/tests/fixtures/journals/{clean,torn_tail,interior_flip,seq_gap,non_canonical}/` and `crates/fsm-cli/tests/recovery_classification.rs` first, encoding exactly the inventory under **Tests**.
2. Implement `JournalHealth` (Ok | TornTail | ChainBroken | StateHashMismatch | NonCanonical | LockIo) in `crates/fsm-cli/src/journal_io.rs` — the typed classification plan 0005 later maps to exit codes 0/2/3/4/5/6.
3. Implement `open(dir, sink)` per architecture: lock first, scan segments in filename order, `record::verify_line` each record, fold semantically through `replay::fold_with`, refusing on torn tail (with remedy) and on interior corruption (with seq, segment, offset, expected-vs-found hash, and blast radius).
4. Implement `verify(dir) -> VerifyReport` — full read-only re-verification ignoring snapshots, returning the health plus record/machine/instance counts and final state hashes.

**Tests:**

- `recovery_classification.rs` over the committed fixtures — `clean/`: opens `Ok`, and the folded counts (records, machines, instances) plus the final state hashes equal the pinned values recorded alongside the fixture.
- `torn_tail/` (final line of the final segment truncated mid-record, nothing after): refuses with `TornTail { segment, offset, bytes }` matching the fixture's pinned values, and the message contains the literal remedy `fsm repair --truncate-torn-tail`.
- `interior_flip/` (one hash-breaking byte flip with valid records after it): refuses with `ChainBroken { seq, segment, offset, expected, found }` matching pinned values; the message carries the blast radius ("records ≥ N unverifiable") and does **not** mention the repair command.
- `seq_gap/` (a record removed mid-chain, later records intact): refuses as interior corruption — `ChainBroken` naming the first missing seq (a gap is a broken chain, not a torn tail).
- `non_canonical/` (one inserted space in an interior record, hashes untouched): refuses with `NonCanonical { seq, segment, offset }` matching pinned values — byte-level tampering is detected even when JSON-equivalent.
- The distinguishing case: a fixture variant with an invalid line *followed by a valid record* classifies as interior corruption, never `TornTail` — torn tails are strictly final.
- `verify` on every fixture returns the same `JournalHealth` as `open`, read-only (the directory's bytes are identical before and after), and on `clean/` its `VerifyReport` counts and final hashes equal the pinned values.

- **Done when:** `cargo test -p fsm-cli --test recovery_classification` proves every fixture's exact classification and message contents, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
