---
id: attempt-record-kind
title: "Attempt Record Kind"
workstream: "0074"
kind: task
depends_on: []
gated: false
touches:
  - crates/fsm-core/src/record.rs
  - crates/fsm-core/src/replay/apply.rs
  - crates/fsm-store/src/store/instance/ack.rs
  - crates/fsm-store/src/store/idempotency.rs
  - crates/fsm-store/tests/effect_attempts.rs
status: planned
merged_as: ""
---
# Attempt Record Kind

A retry kept in process memory is lost by exactly the restart it exists to survive, so every failed attempt becomes a record and the count is derived rather than remembered.

**Steps:**

1. Add the `effect_attempted` record kind to `crates/fsm-core/src/record.rs` with body `{instance_id, effect_id, attempt, outcome, result, request_id, state_hash, state_format}`.
2. Implement the writer beside `ack_effect_outcome_on` in `crates/fsm-store/src/store/instance/ack.rs`, gated by `ensure_writable()` like every mutator and keyed for idempotency like every other request.
3. `attempt` is 1-based and **strictly increasing by exactly one** per `effect_id`. A record whose attempt is not `last + 1` is refused — a gap would make the derived count unreliable, and an unreliable count is worse than no retry at all.
4. `outcome` is always `"failed"`. A successful attempt produces the ordinary `effect_acked` and no attempt record, which is precisely why counting attempt records gives the failed-attempt count directly.
5. The record **clears no pending effect and changes no logical state** beyond claiming its `request_id`: the effect stays in `effects_pending` and the instance stays where it was. That is what makes a retry a retry rather than a re-emit, and it is the property the whole plan rests on.
6. Carry `result` with the same bounded, digest-backed capture an ack carries, so the audit trail holds what each attempt produced rather than only what the last one did.
7. Refuse an attempt record against an effect that is not pending, with the same `req/field_unknown` shape `ack_effect` uses for a settled effect, journaling a `request_rejected` that claims the key so a retry replays the refusal.
8. Leave `effect_acked` completely unchanged in meaning and shape, so every existing consumer, golden, and fold keeps working.
9. **Handle the new kind in the fold.** `crates/fsm-core/src/replay/apply.rs` matches `rec.kind` **exhaustively** (around line 30, with no catch-all arm), so adding a `RecordKind` variant fails to compile until this is done. `effect_attempted` claims its `request_id` and changes nothing else: apply it the way `Annotated` is applied, and confirm a journal containing attempt records folds to the same state as one without them.
10. **Extend `record::instances_touched`** — the exhaustive per-kind mapping plan 0012's `5902` added — with the new kind, returning its `instance_id`. The match is exhaustive, so the build fails until this is done; that is the mechanism working, not an obstacle. Without it, a client subscribed to an instance would never be told an attempt was recorded against it.
11. **Teach duplicate replay about this record kind.** `crates/fsm-store/src/store/idempotency.rs::replay_duplicate` reconstructs a retry's response from the journal with a chain of **kind-specific** branches — and it is `if`/`matches!`, not an exhaustive `match`, so a new kind falls through every arm **silently** rather than failing to compile. Add the `effect_attempted` arm that rebuilds this operation's response. Note the trap before you test it: `replay_duplicate` first consults an in-memory `last_responses` cache, so a same-process retry appears to work with no arm at all; the reconstruction path only runs after a restart, which is exactly the case the executor's resumption and every second client depend on.

**Tests:**

- `crates/fsm-store/tests/effect_attempts.rs`: writing attempt 1 for a pending effect creates the record and leaves the effect pending, the configuration unchanged, and the context unchanged.
- Attempt 2 after attempt 1 succeeds; attempt 3 without a 2 is refused; attempt 1 twice is refused as a duplicate attempt number.
- Idempotency: the same `request_id` replays with `duplicate: true`; the same key with a different capture is refused.
- **Cold-path replay:** drop the `Store`, reopen it, and re-issue the same `request_id` — the reconstruction must produce the same `duplicate: true` response from the journal alone. The warm path is served by an in-memory response cache, so a test that only retries in the same process proves nothing about the case that actually matters.
- An attempt record against a settled effect is refused with a `request_rejected` claiming the key.
- The effect can still be acked after attempts, and the ack clears it normally.
- `state_hash` after an attempt equals the hash before it — assert directly, since "changes no logical state" is the plan's central claim.
- Fold: a journal containing attempt records reconstructs the same state as one without them, apart from the claimed keys — and the fold compiles, which is the exhaustive match doing its job.
- Replay of a journal with three attempts and one ack reproduces every `state_hash`, including the unchanged hashes across the attempts.
- An oversized `result` is `req/payload_too_large`, unjournaled, key unconsumed.
- `instances_touched` returns the instance for an `effect_attempted` record, and a subscriber on that instance is notified when one is written.
- `effect_acked` records are byte-identical to those the pre-change build produced for the same inputs.

- **Done when:** `cargo test -p fsm-store --test effect_attempts` passes every case above, an attempt changes no logical state and leaves the effect pending, the attempt sequence is gapless by construction, cold-path replay reconstructs from the journal, `effect_acked` is unchanged, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
