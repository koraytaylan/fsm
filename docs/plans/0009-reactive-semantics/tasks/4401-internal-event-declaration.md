---
id: internal-event-declaration
title: "Internal Event Declaration"
workstream: "0044"
kind: task
depends_on:
  - eventless-transition-shape
  - macrostep-driver
gated: false
touches:
  - crates/fsm-core/src/spec/parse/decls.rs
  - crates/fsm-core/src/spec/mod.rs
  - crates/fsm-core/src/spec/machine_impl.rs
  - crates/fsm-core/src/step/validate.rs
  - crates/fsm-core/src/error.rs
  - crates/fsm-core/tests/internal_events.rs
  - crates/fsm-cli/tests/naive_caller/one_step_every_non_infra_code.rs
  - crates/fsm-cli/tests/naive_caller/tool_outcomes.rs
  - crates/fsm-cli/tests/naive_caller/reactive_flows.rs
  - crates/fsm-cli/tests/naive_caller/main.rs
  - docs/SPEC.md
status: done
merged_as: ""
---
# Internal Event Declaration

An internal event is an ordinary typed event that only the machine may raise; marking it in the declaration is what lets the engine refuse it from the external send path and keep it out of the list a caller reads to decide what to send.

**Steps:**

1. Add `pub internal: bool` to the event declaration struct in `crates/fsm-core/src/spec/mod.rs`, defaulting to `false`.
2. In `crates/fsm-core/src/spec/parse/decls.rs`, accept an optional `"internal": true` key on an event declaration. A non-boolean is `def/shape`. Everything else about an event declaration — name rules, `fields`, type checking, `MAX_EVENTS`, `MAX_FIELDS` — is unchanged, and an internal event counts against the same ceilings as any other.
3. Serialization omits `internal` when `false`, for the same canonical-identity reason `4301` omits an absent `on`: a machine that does not use the feature must keep its `machine_id`.
4. In `crates/fsm-core/src/step/validate.rs`, extend `validate_event` — the function `step` calls before any scan — to refuse an event declared `internal: true` with the new `req/event_internal`. The hint names the states whose blocks raise it, computed from the definition, so the caller learns what actually produces it rather than being told "no".
5. In the same function, refuse **any** `$`-prefixed event name from the external path with `req/event_internal` rather than `req/event_unknown`. This is what stops a caller sending `$done.state.review` by hand once `4502` starts generating it, and it is written here so the rule exists before the generator does.
6. Do **not** edit `docs/SPEC.md` here. `4201` already landed the appendix row for `req/event_internal`, and the `### run/* catalogue` prose row that describes its trigger and hint policy belongs to `4705`, which owns every SPEC narrative edit in this plan so the document is revised once rather than eight times.

**Tests:**

- `crates/fsm-core/tests/internal_events.rs`: an event declared `internal: true` parses, type-checks its fields, and may be the `on` of a transition.
- `step` with that event name rejects `req/event_internal`, and the hint names at least one state that raises it.
- `step` with `$done.state.anything` rejects `req/event_internal`, not `req/event_unknown`, even when nothing in the machine generates it.
- An undeclared, non-`$` event name still rejects `req/event_unknown` with its existing suggestion behaviour.
- `"internal": "yes"` is `def/shape` at the right pointer.
- Identity: a machine with no internal events serializes without the key and keeps its committed `machine_id`; a machine with one serializes with `"internal": true` and its `machine_id` changes (it is a different machine, which is correct).
- An internal event with 33 fields still reports `def/limit_fields`, and 129 events of which some are internal still reports `def/limit_events`.

- **Done when:** `cargo test -p fsm-core --test internal_events` passes every case above, every `examples/` machine keeps its committed `machine_id`, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.

**Landed:** `EventDecl.internal`, parsed only on events (an `internal` key on an effect is `def/unknown_key`), serialized only when true (`machine_impl.rs`, where event serialization lives). `validate_event` refuses a declared-internal event and any `$`-prefixed name with `req/event_internal`; the hint lists the sendable events and, once `4402` lands `raise`, the sites that raise it — until then it says so. Contrary to step 6, `req/event_internal` and its SPEC rows landed here with the code, per the `4201` correction, and the naive-caller flow sends an internal event, reads the hint, and sends a listed one. That flow pushed `one_step_every_non_infra_code.rs` past the 1000-line ceiling, so plan 0009's flows for both every-code suites moved into `naive_caller/reactive_flows.rs`, which the later tasks extend instead.
