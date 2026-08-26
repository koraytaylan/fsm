---
id: per-instance-fairness
title: "Per Instance Fairness"
workstream: "0076"
kind: task
depends_on:
  - inflight-concurrency-cap
gated: false
touches:
  - crates/fsm-execute/src/sched.rs
  - crates/fsm-execute/tests/fairness.rs
status: planned
merged_as: ""
---
# Per Instance Fairness

Ordering candidates by `effect_id` alone would let the lexicographically-first instance take every slot forever, which is a starvation bug that only appears in the stores big enough to matter.

**Steps:**

1. In `crates/fsm-execute/src/sched.rs`, replace the flat `effect_id` ordering with a round-robin: order candidates by `(position within their own instance's pending queue, instance_id, effect_id)`. Every instance's first pending effect is considered before any instance's second.
2. Compute the position from the observation alone — the index of an effect within its instance's pending list, itself ordered by `effect_id` — so the ordering is a pure function of the observation and needs no memory between ticks.
3. Keep it fully deterministic and restart-stable: a fresh scheduler fed the same observation must produce the identical ordering, which is what keeps plan 0008's restart-equivalence property intact.
4. Apply the ordering **before** both caps from `7601`, so fairness decides who gets the scarce slots rather than merely who is considered.
5. Confirm the interaction with `max_inflight_per_instance`: an instance already at its per-instance cap is skipped entirely at every position, and its slot goes to the next instance in the round rather than being lost.
6. Confirm the interaction with backoff: an effect inside its backoff window is not a candidate at all, so it neither occupies a position nor blocks its instance's later effects from being considered.
7. Document the ordering in the module doc as the second of exactly two orderings the executor uses — this one for selection, and document order for everything the engine decides — so nobody adds a third.

**Tests:**

- `crates/fsm-execute/tests/fairness.rs`: one instance with 100 pending effects and nine instances with 1 each, `max_inflight: 8` — the nine single-effect instances all start, and the busy instance gets at most its per-instance cap.
- Over ten consecutive ticks with completions, every instance makes progress; none is starved.
- Instances whose ids sort first do not receive disproportionate slots — assert an even distribution across a round.
- An instance at `max_inflight_per_instance` is skipped and its slot goes to the next instance, not lost.
- An effect in backoff is not a candidate and does not block its instance's other effects.
- Determinism: the same observation and `now_ms` produce a byte-identical ordering across 100 runs and across a fresh scheduler.
- With one instance only, the ordering degenerates to `effect_id` order, matching `7601`'s behaviour exactly.
- The ordering is computed from the observation alone — assert by feeding an identical observation to a scheduler with a different tick history and getting the same result.

- **Done when:** `cargo test -p fsm-execute --test fairness` passes every case above, no instance is starved across ten ticks, the ordering is a pure function of the observation, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
