---
id: request-id-and-idempotency
title: "Request Id And Idempotency"
workstream: "0037"
kind: task
depends_on:
  - crate-scaffold-and-skeleton
gated: false
touches:
  - crates/fsm-execute/src/rid.rs
  - crates/fsm-execute/tests/request_ids.rs
status: planned
merged_as: ""
---
# Request Id And Idempotency

The executor survives its own death by deriving every `request_id` deterministically from content it already knows, so a restarted executor re-issues the identical key and the store replays (`duplicate: true`) instead of double-applying; a changed intent under a recycled key is refused as `req/request_id_conflict`. The derivations live in their own module because the scheduler, the pipeline, and the watcher's claimed-key checks all need them.

**Steps:**

1. Fill the `rid` module the scaffold declared: `pub fn ack_rid(effect_id: &str) -> String` → `exec-ack-{effect_id}`.
2. Implement `pub fn event_rid(effect_id: &str, event: &str) -> String` → `exec-ev-{effect_id}-{event}`.
3. Implement `pub fn poll_rid(instance_id: &str, deadline: &str, due_ms: i64) -> String` → `exec-poll-{instance_id}-{deadline}-{due_ms}` (due_ms in the key so a new due gets a new key, per SPEC §Deadline poll's "a caller uses a new `request_id` for a new observation"). The store applies whichever deadline is next due and takes no name, so the name is a key ingredient, not a selector.
4. Document at the call sites that the store keys idempotency on `(request_id, content-fingerprint)`: because both derive from journaled state, restart re-issue is exact. Name the one collision that can still occur — two writers racing the same effect ack produce equal keys with different captured stdout, and the loser is refused with `req/request_id_conflict`, which the pipeline surfaces as `exec/store` and halts rather than replaying.

**Tests:**

- Determinism: same inputs → byte-identical ids across N calls, and independent of process restarts (pure string functions).
- Distinctness: `ack_rid` for two effect_ids differ; `poll_rid` differs when only `due_ms` differs (the same deadline re-due gets a fresh key); `event_rid` differs when only the event name differs.
- Uniqueness guarantee: an id derived from `{instance}/{seq}/{k}` (the effect_id shape) is globally unique per (instance, seq, k) — assert by construction over a small generated set.
- End-to-end idempotency: against a real `Store`, ack a pending effect with `ack_rid(...)`; then call `ack_effect_outcome_on` again with the same derived id and same content → second call returns `duplicate: true` and the journal gains no second `effect_acked` record; the same id with a *different* `result` payload → `req/request_id_conflict`.
- Journal-derived visibility: after that ack, `store.state.dedup` contains the derived key — the fact the scheduler relies on to know a write already happened without keeping process memory.

- **Done when:** `cargo test -p fsm-execute --test request_ids` passes determinism, distinctness, the real-store duplicate/conflict behaviour, and the dedup-visibility row, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
