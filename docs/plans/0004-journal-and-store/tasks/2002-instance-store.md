---
id: instance-store
title: "Instance Store"
workstream: "0020"
kind: task
depends_on:
  - machine-store
gated: false
touches:
  - crates/fsm-cli/src/store.rs
status: done
merged_as: ""
---
# Instance Store

The instance pipeline enforces the load-bearing check order — dedup lookup before the expect_seq check, registration only after the fsynced append — so a lost-response retry always receives its original outcome and the double-apply hole cannot open; snapshots stay disposable caches.

**Steps:**

1. Implement `create_instance`, `send_event`, `ack_effect`, `cancel_instance`, and `annotate` in `crates/fsm-cli/src/store.rs`, each following the seven-stage pipeline from architecture: dedup lookup (verbatim re-rendered response, `duplicate: true`) → expect_seq (`req/seq_mismatch`, unjournaled, request_id not consumed) → core validation ([NJ] recomputed) → pure core call → append + fsync → in-memory commit + dedup registration + history index → respond.
2. Implement snapshots per architecture: `snapshots/snap-<seq>.json`, self-hashed under `fsm:snapshot:1`, unique-name temp → sync → unique-name rename (no rename-over) → directory fsync, every 10,000 records and on clean shutdown, immediate reload-and-verify, keep newest 3, corrupt-snapshot fallback to older or full replay; connect the snapshot fast-path into open.
3. Write the inline test module encoding exactly the inventory under **Tests**.

**Tests:**

- Inline in `store.rs` — **the load-bearing ordering test** (`lost_response_retry_returns_original`): apply an event with request_id `R` and a correct `expect_seq`; discard the response; retry `R` verbatim with the now-stale `expect_seq`; assert the ORIGINAL applied outcome returns with `duplicate: true` — never `req/seq_mismatch` — and the journal grew by exactly one record across both calls. This is the double-apply hole closed; if this test fails, the check order is wrong.
- Dedup verbatim: the duplicate response's canonical bytes equal the original response's (byte-compare), plus the `duplicate: true` marker.
- `req/seq_mismatch` semantics: a *fresh* request_id with a stale `expect_seq` → `req/seq_mismatch` (retryable, hint per architecture), journal length unchanged; the same request_id then retried with the current seq applies cleanly — proof it was not consumed.
- [NJ] recomputation: a payload-invalid send (unknown field) errors with journal length unchanged, and an identical retry recomputes the identical error.
- Pipeline coverage per operation: `create_instance` appends `instance_created` and runs the entry chain (case_review lands on `docs_review`, `visits = 1`); `send_event` applied and rejected paths append their kinds; `ack_effect` on a pending id empties it from `pending` and changes nothing else (leaf, ctx, history identical); ack of an unknown effect id appends `request_rejected`; `cancel_instance` → status `Cancelled` and a further send → `run/instance_cancelled`; `annotate` appends with no semantic change.
- Snapshots: clean shutdown writes `snap-<seq>.json` that immediately reloads and verifies (self-hash plus state-hash comparison); reopening through the snapshot fast-path yields final state hashes bit-identical to a full refold; a corrupted snapshot (one flipped byte) is discarded and open falls back — still `Ok`, same hashes; after four snapshots only the newest three remain; snapshot writes never rename over an existing path (unique names asserted).
- History index: an instance's page equals the exact seq list of its records in order; reopening rebuilds an identical index (it is derived, never persisted authoritative).

- **Done when:** inline pipeline tests prove the dedup-first order (lost-response retry returns the original outcome) and snapshot round-trip/fallback behavior, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
