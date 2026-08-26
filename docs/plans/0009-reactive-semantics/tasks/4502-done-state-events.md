---
id: done-state-events
title: "Done State Events"
workstream: "0045"
kind: task
depends_on:
  - final-state-shape
  - internal-queue-semantics
gated: false
touches:
  - crates/fsm-core/src/step/micro.rs
  - crates/fsm-core/src/tree.rs
  - crates/fsm-core/src/spec/validate/reactive.rs
  - crates/fsm-core/src/spec/compile.rs
  - crates/fsm-core/tests/done_state_events.rs
  - docs/SPEC.md
status: done
merged_as: ""
---
# Done State Events

`$done.state.<compound>` is the signal a compound state has never been able to send: my inner workflow finished, including its final state's entry actions, and something outside me may now act on that.

**Steps:**

1. In `crates/fsm-core/src/tree.rs`, add a lookup from a state to its `final` descendant children and from a `final` state to its owning compound, computed once at tree build and stored beside the existing name and chain tables. Nothing in the hot loop may walk the spec to answer "is this final and whose is it".
2. In `crates/fsm-core/src/step/micro.rs`, after a microstep's entry pipeline has run **completely** — every entry block executed and every raise those blocks produced already enqueued — enqueue `$done.state.<parent>` for each entered `final` state, with `InternalOrigin::DoneState { compound }` and an **empty** payload. The ordering is deliberate and normative: `$done.state.X` asserts that X's inner workflow finished *including its final state's actions*, so anything the final state did must already be visible to the handler.
3. Enqueue in entry order when one microstep enters more than one final state — impossible in a sequential machine, reachable in a parallel one — using the same entry-order rule the deadline scheduler already uses.
4. In `crates/fsm-core/src/spec/validate/reactive.rs`, extend event-name resolution so `on: "$done.state.<X>"` resolves when `X` is a compound owning at least one `final` child. Anything else `$done.`-shaped is `def/unknown_event` whose hint **lists the generated names this machine can actually produce** — that list is the whole discoverability of the feature, so build it properly rather than emitting a generic message.
5. Confirm the external refusal from `4401` still holds: `step` with `$done.state.review` as the event name rejects `req/event_internal` regardless of whether the machine generates it.
6. Bind `evt` to an empty object for a transition handling a done event, and state in a comment that a join needing data reads `ctx`, which the finishing sub-workflow already wrote.

**Tests:**

- `crates/fsm-core/tests/done_state_events.rs`: a compound `review` with a `final` child `approved`; entering `approved` enqueues `$done.state.review`, and a transition `{from: "review", on: "$done.state.review", to: "settled"}` fires in the same macrostep, producing two microsteps in one record.
- Entry-action visibility: `approved` has an entry block setting `ctx.decided = true`; the handling transition's guard `ctx.decided` is true, proving the done event was enqueued after the entry pipeline and not before.
- The whole instance does **not** complete when the compound finishes — status stays `running` — which is the entire difference from `terminal`.
- Nesting: a `final` state inside a compound inside a compound raises only its immediate parent's done event; the grandparent's fires only if the parent itself has a `final` child that is entered.
- `on: "$done.state.nosuch"` is `def/unknown_event` and the hint lists the machine's real generated names.
- `on: "$done.state.review"` where `review` owns no `final` child is `def/unknown_event`.
- A done event nobody handles is discarded per `4403` and the macrostep still returns `Applied`.
- `step(.., "$done.state.review", ..)` from outside rejects `req/event_internal`.
- A machine with `final` states but no `$done.` transitions runs to quiescence with one discarded internal event and no reaction microsteps in the record.

- **Done when:** `cargo test -p fsm-core --test done_state_events` passes every case above including the entry-action-visibility ordering and the non-completion rule, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.

**Landed:** `Tree.final_owner` — each state's owning compound when it is a `final` leaf, filled in the same build pass as the name and chain tables — with `final_owner()` and `final_children()` accessors. `micro.rs::done_state_events` appends `$done.state.<parent>` for every entered final leaf, with an empty payload and `InternalOrigin::DoneState`, *after* the microstep's own raises for the trigger and for every reaction alike, so the entry pipeline is complete and its writes are visible before a handler's guard runs. `validate/reactive.rs::generated_event_names` is the one list of names a machine generates (`$done.state.<compound>` per compound owning a final child, then `$done.region.<region>` per region, for 4503); `on: "$done…"` resolves only against it, and any other `$`-shaped name is `def/unknown_event` whose hint prints that list. Step 6 needed `compile.rs` as well: generated names enter the compile-time event table with no fields, so a done handler's guard and block see an empty `evt` and a field reference is `expr/unknown_field` exactly as for a declared fieldless event, rather than the "this scope has no event" refusal a scope without any event produces. Step 3's several-finals-in-one-microstep case is unreachable in practice — a microstep enters one leaf per region and only leaves are final — but the code follows entry order regardless. A creation that lands in a final state through an eventless reaction raises the done event before the first sealed state (`creation_that_lands_in_a_final_state_reacts_before_the_first_sealed_state`). Corrected by 4603: the done event is raised only when some transition names it — a compound finishing with no `$done.state.<compound>` handler raises nothing, so `a_done_event_nobody_handles_is_never_raised` replaced the discard test.
