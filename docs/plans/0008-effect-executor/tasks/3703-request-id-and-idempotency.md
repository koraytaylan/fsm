---
id: request-id-and-idempotency
title: "Request Id And Idempotency"
workstream: "0037"
kind: task
depends_on:
  - deterministic-scheduler
gated: false
touches:
  - crates/fsm-execute/src/sched.rs
  - crates/fsm-execute/tests/request_ids.rs
status: planned
merged_as: ""
---
# Request Id And Idempotency

The executor survives its own death by deriving every `request_id` deterministically from content it already knows, so a restarted executor re-issues the identical key and the store replays (`duplicate: true`) instead of double-applying; a changed intent under a recycled key is refused as `req/request_id_conflict`.

**Steps:**

1. In `sched.rs` (re-exported for `run.rs`), implement `pub fn ack_rid(effect_id: &str) -> String` → `exec-ack-{effect_id}`.
2. Implement `pub fn event_rid(effect_id: &str, event: &str) -> String` → `exec-ev-{effect_id}-{event}`.
3. Implement `pub fn poll_rid(instance_id: &str, deadline: &str, due_ms: i64) -> String` → `exec-poll-{instance_id}-{deadline}-{due_ms}` (due_ms in the key so a new due gets a new key, per SPEC §Idempotency's "new request_id for a new observation").
4. Document at the call sites that the store keys idempotency on `(request_id, content-fingerprint)`: because both derive from instance state, restart re-issue is exact; a deliberately *different* effect content under one effect_id yields a conflict that the pipeline surfaces as `exec/store` and halts that directive rather than replaying.

**Tests:**

- Determinism: same inputs → byte-identical ids across N calls, and independent of process restarts (pure string functions).
- Distinctness: `ack_rid` for two effect_ids differ; `poll_rid` differs when only `due_ms` differs (the same deadline re-due gets a fresh key); `event_rid` differs when only the event name differs.
- Uniqueness guarantee: an id derived from `{instance}/{seq}/{k}` (the effect_id shape) is globally unique per (instance, seq, k) — assert by construction over a small generated set.
- End-to-end idempotency: against a real `Store`, ack a pending effect with `ack_rid(...)`; then call `ack_effect_outcome_on` again with the same derived id and same content → second call returns `duplicate: true` and the journal gains no second `effect_acked` record; the same id with a *different* `result` payload → `req/request_id_conflict`.

- **Done when:** `cargo test -p fsm-execute --test request_ids` passes determinism, distinctness, and the real-store duplicate/conflict behaviour, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
