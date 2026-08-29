---
id: store-version-10
title: "Store Version 10"
workstream: "0080"
kind: task
depends_on:
  - archive-operation
gated: false
touches:
  - crates/fsm-store/src/journal_io/init.rs
  - crates/fsm-store/src/journal_io/mod.rs
  - crates/fsm-store/tests/version_10_migration.rs
  - docs/SPEC.md
status: planned
merged_as: ""
---
# Store Version 10

A store that can hold a seal is a store an older build must not open, so the version moves once and the migration says plainly that it does nothing else.

**Steps:**

1. Raise `STORE_VERSION` from 9 to 10 in `crates/fsm-store/src/journal_io/mod.rs`.
2. Extend the forward migration in `crates/fsm-store/src/journal_io/init.rs` so a `VERSION` 1 through 9 store migrates exactly as 1 through 8 do today: fold the complete journal with snapshot caches ignored, then stamp the new version. Records, machine ids, and snapshot caches are never rewritten or reinterpreted.
3. **Say in a comment that the 9-to-10 step is a stamp and nothing else**, because a pre-10 store has no seal and no `BASE` and there is nothing to convert. A migration arm that does no work is one a later reader will assume was left unfinished, and will helpfully complete.
4. Keep the refusal for any other version unchanged: `store/version_mismatch`, refused and never reinterpreted.
5. Register `VERSION` 10 in `docs/SPEC.md`'s store-version paragraph, and add the sealed-store semantics the spec must now state normatively: a seal record marks the boundary, the base file is required rather than cached, the cut point MUST be a `state_checkpoint`, and a verification that did not read the sealed bytes MUST NOT report what a complete walk reports.
6. State the compatibility consequence explicitly in the spec and carry it to the release notes: **a 0.3.0 store is not readable by 0.2.x, sealed or not**, because the version stamp moves on first write regardless of whether anything was ever archived.

**Tests:**

- `crates/fsm-store/tests/version_10_migration.rs`: a store at each supported `VERSION` from 1 through 9 opens, migrates, stamps 10, and folds to the same state it folded to before — one case per version, not a loop over a range, so a failure names the version.
- A migrated store's records are byte-identical to before migration, asserted over the segment bytes.
- Snapshot caches are ignored during migration and a stale one does not become authoritative.
- A `VERSION` 11 store is refused with `store/version_mismatch` and nothing is written.
- A `VERSION` 10 store with no seal and no `BASE` opens normally — the common case after this plan, and the one a reader will assume needs a base.
- A store migrated from 9 can then be sealed successfully, proving the two paths compose.
- The existing `legacy_snapshot_migration` and `state_v3_migration` suites still pass unchanged.
- `cargo test -p fsm-cli --test spec_appendix` passes with the version and the sealed-store semantics documented.

- **Done when:** `cargo test -p fsm-store --test version_10_migration` passes with one named case per supported prior version, migrated records are byte-identical, the pre-existing migration suites are unchanged and green, SPEC states the sealed-store rules normatively and the 0.2.x incompatibility plainly, and `cargo test`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo fmt --check` succeed.
