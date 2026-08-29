---
id: seal-record-kind
title: "Seal Record Kind"
workstream: "0079"
kind: task
depends_on: []
gated: false
touches:
  - crates/fsm-core/src/record.rs
  - crates/fsm-core/src/record/body_shape.rs
  - crates/fsm-core/src/hashes.rs
  - crates/fsm-core/src/replay/apply/mod.rs
  - crates/fsm-store/src/store/idempotency.rs
  - crates/fsm-core/tests/seal_record.rs
  - docs/SPEC.md
status: done
merged_as: ""
---
# Seal Record Kind

A prefix that is detached without a record saying so is a prefix that was deleted, so sealing begins by making the seal a first-class record and teaching every reader that already assumes it has seen them all.

**Steps:**

1. Add `RecordKind::JournalSealed` to `crates/fsm-core/src/record.rs`, wire name `journal_sealed`, with body `{sealed_through_seq, sealed_last_hash, base_state_root, state_root_format, base_dedup_fp_root, base_dedup_format, archive_id, records_sealed}`.
2. Extend the body-shape check (the `match kind` that ends in the `StateCheckpoint` arm — it lives in `crates/fsm-core/src/record/body_shape.rs` after the split that made room for this kind): `sealed_through_seq` and `records_sealed` parse as `u64`, the three roots satisfy the existing `is_state_hash` predicate, and `state_root_format` equals `fsm_core::replay::STATE_ROOT_FORMAT` exactly. **This plan introduces no new version of that format**, and pinning the constant here is what makes an accidental bump a red test.
3. Assert in the same check that `sealed_last_hash` equals the record's own `prev_hash`. This needs `body_ok` to take the record's `prev`, which it did not; the envelope's `prev` is bare 64-hex while every hash in a *body* carries the `sha256:` prefix, so the comparison adds it. Note also that `root_format_ok` refuses any record declaring `state_root_format` without a `state_root` — a seal declares the format of `base_state_root` and carries a `state_root` only when it lands on a 10 000th sequence, so that predicate needs the kind too. The seal is appended in the ordinary way at `sealed_through_seq + 1`, so the join already exists in the chain; the body **asserts** it rather than creating it, and a record where the two disagree is corrupt.
4. Add the two new hash domains to `crates/fsm-core/src/hashes.rs` beside the existing ones: `BASE_DEDUP_DOMAIN = "fsm:base-dedup:1"` and `ARCHIVE_DOMAIN = "fsm:archive:1"` — plus `BASE_DEDUP_FORMAT = "fsm.base-dedup/1"`, which is paired with the first exactly as `STATE_FORMAT` is paired with `STATE_DOMAIN` and which the seal's shape check needs in this same commit — each with a doc comment saying what it covers and — for the first — why it exists instead of a new version of `fsm:state-root:3`. Tasks `base-state-file` and `archive-manifest` consume these; introducing both here keeps one task owning the identifier namespace this plan adds.
5. Add `Self::JournalSealed` to `RecordKind::all`.
6. Extend `record::instances_touched` with the new kind, returning **empty** — a seal is about the journal, not an instance. It joins the `Genesis | MachineDefined | StateCheckpoint` arm. The match is exhaustive, so the build fails until this is done; that is the mechanism working.
7. Handle the kind in `crates/fsm-core/src/replay/apply/mod.rs`, which matches exhaustively with no catch-all. The seal changes **no state**: apply it exactly as `StateCheckpoint` is applied. It is a marker the loader reads *before* folding, never a mutation the fold performs.
8. **Teach duplicate replay about the kind.** `crates/fsm-store/src/store/idempotency.rs::replay_duplicate` rebuilds a retry's response through a chain of **kind-specific** `if`/`matches!` branches — not an exhaustive `match` — so a new kind falls through every arm **silently**. A seal claims no `request_id` (it is an operator action, not a request), so the correct arm is an explicit one that says so and returns the not-a-request answer rather than falling through by accident. Write the arm and the comment; a silent fallthrough that happens to be harmless today is the same shape as one that is not.
9. Register the kind in `docs/SPEC.md`'s `### Record kinds` table with its body fields and the rule that it changes no logical state. In the same commit, register the three formats this plan introduces — `fsm.base/1`, `fsm.archive/1`, `fsm.base-dedup/1` — and the two new domains in SPEC's format and hash-domain tables, so no later task ships a format the spec does not name.
10. **Decide what happens when a seal lands on a 10 000th sequence, and pin it.** `crates/fsm-store/src/store/commit.rs` folds a provisional record on that boundary and inserts `state_root` and `state_root_format` into its body before appending. A seal declares `state_root_format` itself, so the two meet: the format value agrees, and the body gains a `state_root` the seal never declared. Record bodies are **not** closed — the shape check validates required fields, not the absence of others — so this is survivable, and it must be survivable *deliberately*. Accept the extra field, state in the shape check's comment that a boundary seal carries it, and add the test below. Note for the reader that this `state_root` is the root over the state at the seal's own sequence with the **full** dedup table, which is not `base_state_root`; they are two different values and a later reader must not assert them equal.
11. Do **not** write a seal anywhere yet. This task adds the kind, the domains, and the readers; the writer arrives with the archive operation.

**Tests:**

- `crates/fsm-core/tests/seal_record.rs`: a well-formed seal body passes the shape check; each field individually removed or mistyped fails it.
- A seal whose `sealed_last_hash` differs from its `prev_hash` is refused by the shape check.
- A seal whose `state_root_format` is anything but the current `STATE_ROOT_FORMAT` is refused.
- `instances_touched` returns empty for a seal, and the arm is reached — assert against a constructed record, not against the match arm's existence.
- `RecordKind::all` contains the kind, and the `as_str` / `from_str` round-trip holds for it.
- Folding a journal containing a seal produces the identical `StoreState` as the same journal without it, including `last_hash` divergence only where the extra record changes the chain — assert state equality field by field, since "changes no logical state" is this task's central claim.
- The two new domain constants have the exact byte values named above, asserted as string literals so a typo cannot ship.
- `cargo test -p fsm-cli --test spec_appendix` passes with the kind documented, via the both-directions assertion plan 0010 added.
- **A seal constructed at a sequence that is a multiple of 10 000 passes the shape check with the injected `state_root` present**, and one at any other sequence passes without it. Both directions, since this is the interaction step 10 exists for.
- Every existing golden in `crates/fsm-core/tests/fixtures/` and `crates/fsm-store/tests/fixtures/` is byte-identical to before this task: adding a variant must move no hash and no fixture.

- **Done when:** `cargo test -p fsm-core --test seal_record` passes every case above, all five kind-dispatch sites named in the steps are extended, `spec_appendix` passes with the kind and the three formats documented, every pre-existing golden is byte-identical, and `cargo test`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo fmt --check` succeed.
