---
id: done-invoke-events
title: "Done Invoke Events"
workstream: "0048"
kind: task
depends_on:
  - invocation-outbox-and-state-format
gated: false
touches:
  - crates/fsm-core/src/step/micro.rs
  - crates/fsm-core/src/spec/validate/reactive.rs
  - crates/fsm-core/tests/done_invoke_events.rs
status: planned
merged_as: ""
---
# Done Invoke Events

`$done.invoke.<slot>` is the third member of a family plan 0009 already built, and the only one that carries a payload — so this task resolves the name, types the payload, and adds **no** new delivery mechanism.

**Steps:**

1. In `crates/fsm-core/src/spec/validate/reactive.rs`, extend the generated-event resolution `4502` and `4503` built so `on: "$done.invoke.<slot>"` resolves when the machine declares that slot. Anything else `$done.invoke.`-shaped is `def/unknown_event` whose hint lists this machine's real slot names, reusing the same hint-listing helper rather than a second copy.
2. Type the payload from the slot's `returns` projection: each key is a field, each type is the **child's** declared type for the named child context variable. A transition handling the event reads them through the ordinary `evt` binding, so guards and blocks need no new syntax and no new scope rule.
3. In `crates/fsm-core/src/step/micro.rs`, accept a `$done.invoke.<slot>` event as a macrostep **trigger** delivered by the store, exactly as a due deadline is delivered — not as something the core enqueues. The core never learns that a child completed; that is I/O, and `4902` owns it. Add the trigger variant and nothing else.
4. Confirm the external refusal still holds: `step` called with `$done.invoke.review` from outside rejects `req/event_internal` via `4401`'s `$`-prefix rule, with no new code needed.
5. Confirm plan 0009's discard rule applies unchanged: a `$done.invoke.<slot>` with no handling transition is discarded, recorded in the trace as `internal_unhandled`, and the macrostep still succeeds. Add no special case — a parent that ignores its child's result is making a modelling choice, and `5103` reports it as a smell rather than the engine refusing it.

**Tests:**

- `crates/fsm-core/tests/done_invoke_events.rs`: a transition `{from: "await_review", on: "$done.invoke.review", to: "settled"}` validates when the slot exists and reports `def/unknown_event` when it does not, with the slot list in the hint.
- The payload types against the child's declarations: a `returns` projecting a child `{decimal: "2"}` variable makes `evt.amount` a two-scale decimal, and a guard comparing it against a differently-scaled literal reports the existing scale error.
- A `returns` projection naming a child context variable that does not exist is left to `4901`'s catalogue check — assert that this task reports nothing for it, so the two halves do not double-report.
- Delivering the event as a trigger produces an ordinary macrostep whose reaction microsteps behave exactly as any other trigger's.
- An unhandled `$done.invoke.<slot>` is discarded and the macrostep returns `Applied`, with `internal_unhandled` in the trace.
- `step(.., "$done.invoke.review", ..)` from outside rejects `req/event_internal`.
- A machine that declares a slot but has no transition on its done event still validates.

- **Done when:** `cargo test -p fsm-core --test done_invoke_events` passes every case above, the event reuses plan 0009's queue and discard rules with no parallel path, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
