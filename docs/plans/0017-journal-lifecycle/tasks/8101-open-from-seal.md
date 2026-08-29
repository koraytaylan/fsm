---
id: open-from-seal
title: "Open From Seal"
workstream: "0081"
kind: task
depends_on:
  - store-version-10
gated: false
touches:
  - crates/fsm-store/src/journal_io/load.rs
  - crates/fsm-store/src/store/lifecycle.rs
  - crates/fsm-store/src/snapshot/open.rs
  - crates/fsm-core/src/error.rs
  - crates/fsm-store/tests/open_sealed.rs
  - docs/SPEC.md
status: planned
merged_as: ""
---
# Open From Seal

The loader assumes every journal begins at sequence one with a zero predecessor, and a sealed one does not, so it learns to start from a pair the base supplies and the chain confirms.

**Steps:**

1. `crates/fsm-store/src/journal_io/load.rs::load_records_with_active_meta` begins `expect = 0` and `prev = zeros()`. Give it a starting pair instead of that hard-coded origin, defaulting to the current values so every unsealed path is unchanged.
2. Establish the pair in `crates/fsm-store/src/store/lifecycle.rs` in **this order**, which is the reverse of the intuitive one and is what makes a swapped base detectable:
   1. Read `BASE` if present.
   2. If `BASE` is absent **and** the first live record is not `seq = 1`, refuse with `store/base_missing`. A journal that starts above one with nothing explaining why has had records deleted out from under it, and that must never be mistaken for a seal.
   3. Load the live records using the starting pair from `BASE`.
   4. Require the **first** live record to be the seal, and require its `sealed_through_seq`, `sealed_last_hash`, `base_state_root`, and `base_dedup_fp_root` to match the base. The chain authenticates the seal and the seal authenticates the base; a base swapped for another store's is caught here.
   5. Fold the live suffix onto the base with the existing `fold_from`.
3. Add `store/base_missing` and `store/base_mismatch` to `fsm_core::error::ALL_CODES` and to SPEC Appendix A in this commit, each with a `hint` that states the fix — and for `base_mismatch`, a hint that says plainly there is no repair, because the records the base replaced are not in this directory.
4. **Never fall back to a complete fold when a base is present and wrong.** There is nothing to fall back to. This is the single most important line in the task: a fallback here silently serves a store assembled from a base nobody authenticated.
5. Teach `crates/fsm-store/src/snapshot/open.rs` one rule and no more: skip any snapshot cache whose sequence is at or below the seal, because `snapshot_matches_prefix` folds the records at or below a cache's sequence and those records are gone. Caches above the seal keep their exact current meaning and trust.
6. Keep read-only opens read-only. A sealed store is inspected by monitoring sessions exactly as an unsealed one is: no lock, no file creation, no modification.
7. The per-instance history index that `HistSink::on_record` builds and `lifecycle.rs` rebuilds on both open paths is fed from the records that were loaded, so on a sealed store it covers the live suffix and nothing more. That is correct and must stay correct — do not seed it from the base, whose instances have no records to index. Reporting the resulting shortfall to a caller is `readers-on-sealed-store`'s job; this task's obligation is that the index is built from the same records the fold saw, and a comment saying so.
8. Leave `load_intact_prefix` working for diagnosis on a sealed store, so a damaged sealed journal can still be asked how much of it is intact.

**Tests:**

- `crates/fsm-store/tests/open_sealed.rs`: a sealed store opens and folds to the same state the unsealed store folded to before sealing — the plan's headline property, asserted with `store_states_eq`.
- Every unsealed path is unchanged: an ordinary store opens with the default starting pair, and the existing store suites pass untouched.
- A `BASE` deleted from a sealed store gives `store/base_missing`, not a fold from the seal's sequence.
- A journal whose first record is above one with **no** `BASE` gives `store/base_missing` — the deleted-segments case, which must not look like a seal.
- A `BASE` from a different store gives `store/base_mismatch`, and nothing is served.
- A `BASE` with one context byte altered gives `store/base_mismatch` via `base_state_root`.
- A `BASE` with one `fp` altered gives `store/base_mismatch` via `base_dedup_fp_root` — the case the second root exists for.
- A sealed journal whose first live record is **not** the seal is refused.
- A snapshot cache at or below the seal is skipped and the store still opens; a cache above the seal is still used, proving the fast path survives sealing.
- A read-only open of a sealed store takes no lock and writes nothing, asserted by opening it while a writer holds the lock.
- The per-instance history index after a sealed open covers exactly the live records, and both open paths agree — assert the two rebuild paths produce the same index, since they are two call sites of one rule.
- Idempotency survives: a `request_id` claimed above the cut still replays with `duplicate: true` after a seal **and after a reopen**, so the answer comes from the base and the live suffix rather than an in-memory cache.
- A `request_id` carried in the base is conflict-checked: re-issuing it with different content is refused rather than replayed, which is what the carried fingerprints are for.

- **Done when:** `cargo test -p fsm-store --test open_sealed` passes every case above, a sealed store folds to the pre-seal state, no path falls back to a complete fold when a base is present and wrong, both roots are independently falsifiable through the open path, unsealed behaviour is byte-identical, and `cargo test`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo fmt --check` succeed.
