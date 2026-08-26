---
id: done-region-events
title: "Done Region Events"
workstream: "0045"
kind: task
depends_on:
  - done-state-events
gated: false
touches:
  - crates/fsm-core/src/step/micro.rs
  - crates/fsm-core/tests/done_region_events.rs
  - docs/SPEC.md
  - docs/plans/0009-reactive-semantics/ARCHITECTURE.md
status: done
merged_as: ""
---
# Done Region Events

This is the join. A parallel definition has been able to fork since regions shipped and has never been able to notice that a branch finished; `$done.region.<name>` closes that with no new state concept, because a region's `terminal` leaf already means exactly "this branch is over".

**Steps:**

1. In `crates/fsm-core/src/step/micro.rs`, after a microstep completes, enqueue `$done.region.<region>` with `InternalOrigin::DoneRegion { region }` and an empty payload for each region whose active leaf became `terminal` in that microstep. "Became" is the operative word: a region already terminal before the microstep does not re-raise.
2. Should one microstep terminate more than one region, enqueue in **region document order** — the same total order the candidate scan, the deadline scheduler, and creation already use. There is no second ordering rule in this engine and this task must not introduce one. (A transition changes one region, so this case is unreachable through a transition and is pinned on the function; across microsteps the events enqueue as the regions finish — corrected by this task.)
3. Do **not** generate `$done.machine`. A sequential instance on a terminal leaf, or a parallel instance whose every region is terminal, is `Completed`, every region is inert, and the event could only ever be discarded. The absence is a decision; record it in a comment beside the region logic so it reads as one.
4. Rely on the existing inertness rules rather than adding new ones: SPEC §Semantics 9 already removes schedules sourced from a terminal region's chain and makes completed regions inert to events, so a transition sourced *inside* region X cannot handle `$done.region.X` — the candidate scan skips that region. Confirm this falls out and pin it with a test rather than writing a guard.
5. Confirm `def/cross_region` still forbids a join transition from targeting another region: a join fires in region B, sourced in B, targeting B, triggered by A's completion. Add no exception.
6. Extend event-name resolution so `on: "$done.region.<X>"` resolves when `X` is a declared region name, reusing the hint-listing machinery `4502` built.

**Tests:**

- `crates/fsm-core/tests/done_region_events.rs`: a two-region machine where region A reaches terminal and region B has `{from: "waiting", on: "$done.region.a", to: "proceed"}` — B advances inside the same macrostep, and one record holds both microsteps.
- The instance is **not** complete after A finishes, and becomes complete only when B also reaches terminal.
- A transition inside region A on `$done.region.a` never fires, because the region is inert once terminal — assert the event is discarded rather than handled.
- A join transition targeting a state in region A reports `def/cross_region` at admission.
- Two regions terminating in one macrostep enqueue their done events in the order they finished, and one internal event has one winner across regions, by document order; the same-microstep document order is pinned by a unit test on `done_region_events` (corrected by this task).
- A region already terminal at the start of a macrostep does not re-raise its done event.
- No `$done.machine` is ever enqueued: a machine whose every region terminates in one macrostep produces exactly the two region events and nothing more.
- `on: "$done.region.nosuch"` is `def/unknown_event` with the generated-name list in the hint.
- A **sequential** machine's terminal leaf raises nothing — regions are the only source of `$done.region.*`, and completion carries the sequential case.

- **Done when:** `cargo test -p fsm-core --test done_region_events` passes every case above, a two-region fork/join machine settles in one journal record, no `$done.machine` exists anywhere in the code or tests, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.

**Landed:** `micro.rs::done_region_events` compares each region's active leaf before and after a microstep and enqueues `$done.region.<region>` for the regions that *became* terminal, in region document order, behind the microstep's own raises and `$done.state` events; the driver snapshots the pre-reaction configuration first, so a region terminal before the microstep never re-raises. There is no `$done.machine`, and the comment beside the function records that as a decision. The plan's ordering sentence was falsified: a transition changes one region, so two regions never finish in one microstep, and across microsteps the queue is finish order — a later event cannot precede one the earlier reaction already consumed. ARCHITECTURE §0045 and step 2 now say so; the same-microstep document order is pinned by a unit test on the function, and `two_regions_finishing_in_one_macrostep_enqueue_as_they_finish` pins the finish order together with the one-winner-across-regions rule of §0042 (`x` takes `$done.region.y` ahead of `z`). `on: "$done.region.<X>"` resolution came with 4502's `generated_event_names`; the inert-region and `def/cross_region` behaviour fell out of the existing rules and is pinned by tests, with no new guard. Corrected by 4603: `$done.region.<region>` is raised only when some transition names it, because the inertness suite found a plain parallel machine's trace gaining `internal_unhandled` the moment a region finished; the tests that relied on a discard now declare an inert handler so the event is raised at all.
