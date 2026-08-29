---
id: base-state-file
title: "Base State File"
workstream: "0079"
kind: task
depends_on:
  - seal-record-kind
gated: false
touches:
  - crates/fsm-store/src/base.rs
  - crates/fsm-store/src/lib.rs
  - crates/fsm-store/tests/base_state_file.rs
  - crates/fsm-store/tests/fixtures/base_v1.json
status: planned
merged_as: ""
---
# Base State File

A sealed store opens onto state whose records are gone, so that state becomes a required file with its own format — deliberately similar to a snapshot cache and deliberately trusted differently.

**Steps:**

1. Create `crates/fsm-store/src/base.rs` with format constant `fsm.base/1` and an encode/decode pair for the materialized `StoreState` at a seal: machines, instances, `instance_machines`, `last_seq`, `last_hash`, and the surviving `dedup` entries **with their `fp`**.
2. Reuse the shape `crates/fsm-store/src/snapshot/encode.rs::snapshot_material` produces, including omitting `fp` for entries that have none exactly as it does. Do not import it and do not refactor the two into one — they are two formats with two trust rules, and a shared encoder is how one silently acquires the other's rule. Say that in the module doc, and say which is which: **a missing snapshot degrades to a fold; a missing base refuses the open.**
3. Compute `base_state_root` with `fsm_core::replay::state_root_at(&state, sealed_through_seq)`. Call the existing function; do not reimplement it and do not extend it. Three writers now commit a value from that function — the 10 000-boundary record, `state_checkpoint`, and the seal — and they must all call it rather than agree by coincidence.
4. **`base_state_root` is not equal to the `state_root` in the checkpoint record at the same sequence, and must not be asserted equal.** `state_root_at` covers `dedup`, and the base's dedup has the dropped entries removed while the checkpoint's covers the table as it stood. Same function, same sequence, different state, different root — write that down at the site, because it looks exactly like a bug to anyone who checks.
5. Compute `base_dedup_fp_root` as `sha256:` + hex of `domain_hash(BASE_DEDUP_DOMAIN, material)` over canonical `{request_id: fp}` for every surviving entry that has an `fp`, entries without one omitted. Document at the site why this second root exists: `state_root_at` deliberately excludes fingerprints because "the fingerprint lives in the record body that claimed the key, so the hash chain already authenticates it" — and sealing is exactly the operation that removes that record from the live chain.
6. Decoding validates both roots against values the caller supplies from the seal record, and returns `store/base_mismatch` if either disagrees. Decoding **never** repairs, guesses, or falls back: there are no records to fall back to.
7. Decoding enforces the same 16 MiB persistence read unit every other unit obeys, through the existing `read_regular_file_capped` and `PERSISTENCE_READ_CAP`, and validates each instance against its compiled machine the way `snapshot/decode.rs` does — a base that decodes into an invalid instance is a base that must be refused, not served.
8. Export the module from `crates/fsm-store/src/lib.rs`. Writing the file, and choosing which entries survive, belong to later tasks; this one owns the format and both roots.

**Tests:**

- `crates/fsm-store/tests/base_state_file.rs`: a state round-trips through encode and decode to an equal `StoreState`, compared with the existing `store_states_eq`.
- `crates/fsm-store/tests/fixtures/base_v1.json` is a committed golden compared with `include_str!`, so the format cannot drift silently.
- Both roots are recomputed on decode and compared: a base with one byte of an instance's context altered fails on `base_state_root`; a base with one `fp` altered fails on `base_dedup_fp_root`. **Both cases must be asserted** — the second is the entire reason the second root exists, and a suite that only tests the first would pass an implementation that omitted it.
- An entry with no `fp` is omitted from the fingerprint material rather than encoded as null, and a base whose entries all lack fingerprints still produces a stable root.
- A base over the persistence cap is refused with the standard bounded-read error, not read.
- A base whose instance state is invalid for its machine is refused.
- `base_state_root` for a given state equals `state_root_at` for that state at the same seq — assert against the core function directly, so a divergent private reimplementation fails.
- `base_state_root` **differs** from the `state_root` of the checkpoint record at the same sequence whenever any entry was dropped, and the test says why in a comment. This is the assertion that stops a later reader "fixing" the two into agreement.
- Encoding is deterministic: the same state encodes byte-identically twice, and `dedup` and instance ordering are canonical.

- **Done when:** `cargo test -p fsm-store --test base_state_file` passes every case above, the committed golden matches byte for byte, both roots are independently falsifiable by the tests named, decode refuses rather than degrades on every failure path, and `cargo test`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo fmt --check` succeed.
