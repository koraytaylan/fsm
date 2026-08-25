---
id: read-only-watcher
title: "Read Only Watcher"
workstream: "0037"
kind: task
depends_on:
  - pending-effect-metadata
gated: false
touches:
  - crates/fsm-execute/src/watch.rs
  - crates/fsm-execute/tests/watch.rs
status: done
merged_as: ""
---
# Read Only Watcher

The watcher is the only read-side component and touches the store exclusively through `Store::open_read_only` — no lock, coexists with the MCP writer, one hash-verified consistent prefix per open — so monitoring never perturbs the writer and never goes stale. Every fact a decision needs is read here, from the journal, so a restarted executor observes the same world its predecessor did.

**Steps:**

1. Implement `Watcher { data_dir: PathBuf, last_seq: u64, resolved: BTreeMap<String, PendingEffect> }` and `Watcher::new(data_dir)`.
2. Implement `fn scan(&mut self) -> Result<Observation, ExecError>` that opens a *fresh* `Store::open_read_only(&self.data_dir)` each call, reads `journal.last_seq`, and for **every** instance — not only the running ones — reads what the outbox needs straight from `store.state.instances`, whose `InstanceState` fields are public: `pending`, `status`, and `deadlines` (already `name → due_ms` integers). Do **not** go through `instance_view`: it builds a full response per instance and evaluates `enabled_events` under a step budget for each, which this component never uses and which would cost a full analysis per instance per tick. `enabled_events` is the pipeline's concern, on the one instance it just acked. A transition into a terminal state emits its entry-block effects like any other, so a completed instance can hold pending work; status gates the *advance*, not the run, and the pipeline makes that check itself (it must test `status == running` explicitly, because a cancelled instance keeps its configuration and so still reports enabled events). Deadlines are collected for running instances only, since the engine rejects a poll against a completed or cancelled one.
3. Assemble `Observation { from_seq, to_seq, pending: Vec<PendingEffect>, settled: Vec<SettledEffect>, due_deadlines: Vec<DueDeadline>, cancellations: Vec<String>, claimed_request_ids: BTreeSet<String>, instance_states: BTreeMap<String, InstanceSnap> }`:
   - `pending` holds **every** effect id currently in any instance's `effects_pending`, each resolved through `effect::resolve` and memoized in `self.resolved` so a long-pending effect is re-derived once, not once per scan. It is deliberately not watermark-filtered: a fresh executor must see a long-pending effect on its first scan. Expose `pub fn resolved_count(&self) -> usize` so the memo is assertable from an integration test.
   - `settled` holds `SettledEffect { instance_id, effect_id, effect_name, outcome, seq }` so the scheduler can find an ack whose advance event was never sent. It carries the resolved **name** for the same reason `pending` does — a handler cannot be looked up by id — and `effect::resolve` still works after the ack because it replays the emitting record, which the journal keeps forever. Bound the list or it grows without limit: only acks of **running** instances whose `seq` is greater than that instance's latest `event_applied`/`deadline_applied` seq. An ack the instance already transitioned past cannot be the interrupted one; in the interrupted case the ack is the instance's newest record, so this includes exactly what recovery needs.
   - `claimed_request_ids` holds the `store.state.dedup` keys that start with `exec-` (a public map folded from the journal) — the durable answer to "did any process already write this?", filtered because the full map holds every request the store ever served and the executor only ever asks about its own derived keys.
   - `due_deadlines` holds `DueDeadline { instance_id, deadline_name, due_ms }` straight from `InstanceState::deadlines`, for entries at or past `now_ms` on running instances only — the engine rejects a poll against a completed or cancelled instance.
   - `instance_states` holds `InstanceSnap { status, pending: usize, deadlines: usize }` per observed instance — counts and a status, nothing hashed or rendered — so an action line can name what it acted on without a second store open.
   - `last_seq` advances to `to_seq` and is used for logging and the `from_seq`/`to_seq` span only — never to decide work.
4. Map every store open/fold error to `exec/store`, preserving the underlying `ErrorObj` in `details`; an unresolvable effect id surfaces as `exec/effect_unresolved` for that effect without failing the whole scan.
5. Cancellation detection: an instance whose `status` is `cancelled` and was `running` at the prior scan is added to `cancellations` once.

**Tests:**

- Watching an empty data dir returns `to_seq == 0`, empty `pending`, no panic.
- After a writer (a separate `Store` handle in the same test) defines a machine, creates an instance, and advances it so one effect is pending, a single `scan` reports exactly that effect in `pending` with its `instance_id`, `effect_id`, resolved `effect_name`, and `args` — and `last_seq` advances to the writer's `last_seq`.
- A second `scan` with no intervening writes reports the **same** still-pending effect again (pending is state, not an edge) and `resolved_count()` stays at 1 — the memo answered, no second fold.
- Terminal-state emit: a machine whose entry into a terminal state emits an effect leaves that effect in `pending` after the instance completes — the row that proves the scan is not filtered by status.
- `claimed_request_ids` holds only `exec-`-prefixed keys: a request id written by the CLI (`req-…`) never appears in it.
- A further transition emitting a second effect reports both on the next scan.
- After the writer acks an effect, the next scan drops it from `pending` and lists it in `settled` with its resolved `effect_name`, outcome, and seq, and `claimed_request_ids` contains the ack's `request_id`.
- `settled` is bounded: after the instance advances past the ack (an `event_applied` with a higher seq), the next scan no longer lists it; an ack on a completed instance is never listed.
- Concurrency: while the watcher holds no lock, a concurrently-opened writer `Store` in the same process can append between scans and the next scan observes the new prefix — assert the watcher never errors with `store/lock`.
- Cancel: after the writer cancels the instance, the next scan lists its id in `cancellations` exactly once across repeated scans.
- Open failure (data dir does not exist) maps to `exec/store`, not a panic.

- **Done when:** `cargo test -p fsm-execute --test watch` passes every row (fresh-prefix reads, still-pending re-report, terminal-state emit, settled/claimed-key surfacing, cancel detection, lock-free coexistence with a writer), and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
