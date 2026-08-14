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

1. Author the fixtures first: committed journal directories under `crates/fsm-cli/tests/fixtures/journals/{clean,torn_tail,interior_flip,seq_gap,non_canonical}/`, and `crates/fsm-cli/tests/recovery_classification.rs` asserting each opens or refuses with exactly its classification, including segment, byte offset, and — for the torn tail — the literal remedy `fsm repair --truncate-torn-tail`.
2. Implement `JournalHealth` (Ok | TornTail | ChainBroken | StateHashMismatch | NonCanonical | LockIo) in `crates/fsm-cli/src/journal_io.rs` — the typed classification plan 0005 later maps to exit codes 0/2/3/4/5/6.
3. Implement `open(dir, sink)` per architecture: lock first, scan segments in filename order, `record::verify_line` each record, fold semantically through `replay::fold_with`, refusing on torn tail (with remedy) and on interior corruption (with seq, segment, offset, expected-vs-found hash, and blast radius).
4. Implement `verify(dir) -> VerifyReport` — full read-only re-verification ignoring snapshots, returning the health plus record/machine/instance counts and final state hashes.

- **Done when:** `cargo test -p fsm-cli --test recovery_classification` proves every fixture's exact classification and message contents, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
