---
id: read-only-watcher
title: "Read Only Watcher"
workstream: "0037"
kind: task
depends_on:
  - crate-scaffold-and-skeleton
gated: false
touches:
  - crates/fsm-execute/src/watch.rs
  - crates/fsm-execute/tests/watch.rs
status: planned
merged_as: ""
---
# Read Only Watcher

The watcher is the only read-side component and touches the store exclusively through `Store::open_read_only` — no lock, coexists with the MCP writer, one hash-verified consistent prefix per open — so monitoring never perturbs the writer and never goes stale.

**Steps:**

1. Implement `Watcher { data_dir: PathBuf, last_seq: u64 }` and `Watcher::new(data_dir)`.
2. Implement `fn scan(&mut self) -> Result<Observation, ExecError>` that opens a *fresh* `Store::open_read_only(&self.data_dir)` each call, reads `last_seq`, and for every instance reads the `instance_view` fields the outbox needs: `effects_pending`, `deadlines_pending`, `status`, `enabled_events`, `context`, `seq`, `state_hash`.
3. Assemble `Observation { from_seq, to_seq, newly_pending: Vec<PendingEffect>, due_deadlines: Vec<DueDeadline>, cancellations: Vec<String>, instance_states: BTreeMap<String, InstanceSnap> }` where `PendingEffect { instance_id, effect_id, effect_name, args }` and `DueDeadline { instance_id, deadline_name, due_ms }`. `newly_pending` holds effects with seq greater than the previous scan's watermark; `last_seq` is the only state carried across scans; update the watermark to `to_seq` before returning.
4. Map every store open/fold error to `exec/store`, preserving the underlying `ErrorObj` in `details`.
5. Cancellation detection: an instance whose `status` transitions to `cancelled` since the prior scan (per its `instance_states` snap vs. stored watermark view) is added to `cancellations` once.

**Tests:**

- Watching an empty data dir returns `to_seq == 0`, empty `newly_pending`, no panic.
- After a writer (a separate `Store` handle in the same test) defines a machine, creates an instance, and advances it so one effect is pending, a single `scan` reports exactly that effect in `newly_pending` with its `instance_id`, `effect_id`, and `effect_name` — and the watcher's `last_seq` advances to the writer's `last_seq`.
- A second `scan` with no intervening writes returns empty `newly_pending` (watermark held; no re-report).
- A further transition emitting a second effect is reported on the next scan, and only it.
- Concurrency: while the watcher holds no lock, a concurrently-opened writer `Store` in the same process can append between scans and the next scan observes the new prefix — assert the watcher never errors with `store/lock`.
- Cancel: after the writer cancels the instance, the next scan lists its id in `cancellations` exactly once across repeated scans.
- Open failure (data dir does not exist) maps to `exec/store`, not a panic.

- **Done when:** `cargo test -p fsm-execute --test watch` passes every row (fresh-prefix reads, watermark no-re-report, cancel detection, lock-free coexistence with a writer), and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
