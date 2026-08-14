---
id: record-envelope
title: "Record Envelope"
workstream: "0017"
kind: task
depends_on: []
gated: false
touches:
  - crates/fsm-core/src/lib.rs
  - crates/fsm-core/src/record.rs
  - crates/fsm-core/src/replay.rs
  - crates/fsm-core/tests/record_golden.rs
  - "crates/fsm-core/tests/fixtures/records/**"
status: planned
merged_as: ""
---
# Record Envelope

Every guarantee about audit and replay rests on the record envelope and the pure fold: ten record kinds, a domain-separated chain hash, byte-canonical verification, and semantic re-application that must reproduce every journaled state hash — pinned by golden and tampered fixtures authored first.

**Steps:**

1. Author the fixtures first under `crates/fsm-core/tests/fixtures/records/`: `chain_golden.jsonl` (define → create → applied event with effects → rejection → ack over the `case_review` reference machine) plus `tampered_body.jsonl`, `tampered_hash.jsonl`, `seq_gap.jsonl`, and `non_canonical.jsonl`; and `crates/fsm-core/tests/record_golden.rs` asserting the valid chain folds with matching hashes and each tampered variant fails with exactly its error kind.
2. Add `pub mod record; pub mod replay;` to `crates/fsm-core/src/lib.rs`.
3. Implement `RecordKind`, `Record`, `seal` (hash = H("fsm:record:1", envelope-minus-hash); genesis pins format and limits), and `verify_line` (byte-canonical equality, seq consecutiveness, prev linkage, hash recompute, per-kind `RecordError`) in `crates/fsm-core/src/record.rs`.
4. Implement `RecordSink`, `StoreState`, and `fold_with` in `crates/fsm-core/src/replay.rs`, re-applying each record through the pure engine and re-verifying the recorded `state_hash`, `exited`, `entered`, and `source_state` per architecture.

- **Done when:** `cargo test -p fsm-core --test record_golden` passes the valid chain and rejects every tampered fixture with its exact error kind, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
