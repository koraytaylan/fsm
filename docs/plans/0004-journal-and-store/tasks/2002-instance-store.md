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
status: planned
merged_as: ""
---
# Instance Store

The instance pipeline enforces the load-bearing check order — dedup lookup before the expect_seq check, registration only after the fsynced append — so a lost-response retry always receives its original outcome and the double-apply hole cannot open; snapshots stay disposable caches.

**Steps:**

1. Implement `create_instance`, `send_event`, `ack_effect`, `cancel_instance`, and `annotate` in `crates/fsm-cli/src/store.rs`, each following the seven-stage pipeline from architecture: dedup lookup (verbatim re-rendered response, `duplicate: true`) → expect_seq (`req/seq_mismatch`, unjournaled, request_id not consumed) → core validation ([NJ] recomputed) → pure core call → append + fsync → in-memory commit + dedup registration + history index → respond.
2. Implement snapshots per architecture: `snapshots/snap-<seq>.json`, self-hashed under `fsm:snapshot:1`, unique-name temp → sync → unique-name rename (no rename-over) → directory fsync, every 10,000 records and on clean shutdown, immediate reload-and-verify, keep newest 3, corrupt-snapshot fallback to older or full replay; connect the snapshot fast-path into open.
3. Add inline unit tests for the check order — including the lost-response scenario: apply, drop the response, retry the same request_id with the stale expect_seq, and assert the original outcome returns with `duplicate: true` rather than `req/seq_mismatch`.
4. Add inline unit tests for snapshot reload-and-verify, the keep-3 rotation, and the corrupt-snapshot fallback.

- **Done when:** inline pipeline tests prove the dedup-first order (lost-response retry returns the original outcome) and snapshot round-trip/fallback behavior, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
