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
status: done
merged_as: ""
---
# Record Envelope

Every guarantee about audit and replay rests on the record envelope and the pure fold: ten record kinds, a domain-separated chain hash, byte-canonical verification, and semantic re-application that must reproduce every journaled state hash — pinned by golden and tampered fixtures authored first.

**Steps:**

1. Author the fixtures under `crates/fsm-core/tests/fixtures/records/` and `crates/fsm-core/tests/record_golden.rs` first, encoding exactly the inventory under **Tests**.
2. Add `pub mod record; pub mod replay;` to `crates/fsm-core/src/lib.rs`.
3. Implement `RecordKind`, `Record`, `seal` (hash = H("fsm:record:1", envelope-minus-hash); genesis pins format and limits), and `verify_line` (byte-canonical equality, seq consecutiveness, prev linkage, hash recompute, per-kind `RecordError`) in `crates/fsm-core/src/record.rs`.
4. Implement `RecordSink`, `StoreState`, and `fold_with` in `crates/fsm-core/src/replay.rs`, re-applying each record through the pure engine and re-verifying the recorded `state_hash`, `exited`, `entered`, and `source_state` per architecture.

**Tests:**

- `chain_golden.jsonl` (define → create → applied event with effects → rejection → ack over the `case_review` reference machine), asserted by `record_golden.rs`: `verify_line` accepts every line; `fold_with` re-applies the chain and every recomputed `state_hash` equals the journaled one; the final `StoreState` holds exactly one machine, one instance on the expected leaf, and the two request ids in `dedup`; the ack record changes `pending` only — leaf, ctx, and history are untouched by it.
- Tampered fixtures, each failing with exactly its named variant: `tampered_hash.jsonl` (hash field edited) → `RecordError::HashMismatch` with the failing seq; `non_canonical.jsonl` (one inserted space, chain otherwise valid) → `RecordError::NonCanonical` with the byte offset; `seq_gap.jsonl` (a seq skipped, later records re-sealed) → `RecordError::SeqGap`; `tampered_body.jsonl` (a `ctx_after` value altered **and the chain re-sealed consistently**, so hashes verify) → `ReplayError::StateHashMismatch { seq, expected, found }` — the semantic net catching what the chain net cannot; `tampered_field.jsonl` (`exited` list altered, chain re-sealed) → `ReplayError::FieldMismatch { seq, field: "exited" }`.
- `record.rs` inline unit tests: `seal` on a fixed input produces pinned canonical bytes (field order `hash`, `kind`, `prev`, `seq`, `ts` inside the sorted envelope — asserted byte-for-byte); genesis has seq 0, `prev` of sixty-four `0`s, and a body carrying `format: "fsm.journal/1"` and every key of the `limits.rs` table; `verify_line` yields `Parse` on malformed JSON, `PrevMismatch` on a wrong `prev`, `BodyInvalid` on a body missing its kind's required fields, and `SeqGap` when `expect_seq` disagrees.
- `replay.rs` inline unit tests: a `RecordSink` sees every record exactly once in seq order with the post-record state; a rejection record re-verifies the *unchanged* `state_hash`; folding an empty iterator yields the empty `StoreState` with `last_seq` 0 semantics per architecture.

- **Done when:** `cargo test -p fsm-core --test record_golden` passes the valid chain and rejects every tampered fixture with its exact error kind, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
