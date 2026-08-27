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
  - crates/fsm-execute/tests/concurrency.rs
status: done
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

**Landed:** Selection orders candidates by `(position within the instance's own candidate queue, instance_id, effect_id)` in two passes — `effect_id` order first, which assigns each candidate its position, then the round-robin proper. Positions are taken over **candidates**, not over pending effects, so an effect inside its backoff window neither occupies a place in the round nor pushes its instance's later effects out of it; that is asserted directly rather than assumed.

The whole ordering is a pure function of one observation, so a scheduler with five ticks of history and a fresh one produce the identical result from the same input — asserted both ways, since restart equivalence is what would quietly break if the ordering ever remembered anything.

The per-instance skip is `continue`, never `break`, so an instance already at its cap loses its turn and not the slot: the next instance in the round takes it. A row pins both halves — the slot is passed on when someone can use it, and simply unusable when nobody can.

**On "nobody is starved".** The first version of that test fed the same observation back every tick, which models an executor whose acks never land, and it failed. That was the test being wrong rather than the ordering: "with completions" means what it means in the journal — an acked effect leaves its instance's outbox, so the next observation is a smaller one. Rewritten to drain, the plan's own shape (one instance with a hundred, nine with one each, eight slots) serves all ten instances within two rounds.

**The residual, stated rather than hidden.** The round-robin does not *rotate*: the tie-break at each position is `instance_id`, so more permanently-busy instances than global slots leaves the highest-sorting ones waiting until one of the others empties. A rotating cursor would close that window and would cost restart equivalence with it — two executors reading the same journal prefix would disagree about whose turn it was — so this is a deliberate trade, not an oversight. A test asserts the behaviour exactly, and the module doc explains the choice, so `7802` has something true to document. What the ordering does buy is that no instance can convert *more queued work* into *more of the host*, which is the starvation that shows up in real stores.

Two ordering assertions in `concurrency.rs` moved to the new order, which `7601`'s note said would happen. The module doc now names the executor's **two** orderings — document order for everything the engine decides, round-robin for selection — and says why a third would need a source of truth outside the observation.
