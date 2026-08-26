---
id: 0009
title: "Reactive Semantics"
status: planned
---
# Scope — Plan 0009

> A machine that can only be pushed is not a statechart; it is a lookup table with a journal.

## Why this plan

Every advance `fsm` can make today requires somebody outside the engine to name an event and send it. Three absences cause that, and they are the same absence wearing three faces:

- **`on` is mandatory.** `TransitionSpec.on` is a `String` (`crates/fsm-core/src/spec/mod.rs:221`), so there is no way to write "when this guard becomes true, move on." A state that is merely a decision point still has to be poked by a caller who already knows the answer.
- **`emit` only reaches the outbox.** A block's effects leave the process. A machine cannot signal *itself*, so every internal decomposition — validate, then route; settle, then notify — has to leave through the executor and come back as an external event, paying a journal record and a subprocess for a decision the engine could have made in nanoseconds.
- **Completion is a status, not a signal.** SPEC §Semantics 10 completes a parallel instance when every region's leaf is terminal, but nothing can transition *on* that fact. There is fork and there is no join. A compound state cannot report that its inner workflow finished, because `terminal` means "the machine is over", not "this sub-workflow is over".

The result is that the engine's own reachability analysis knows more about where a workflow is going than the workflow does. This plan closes that by adding a **bounded run-to-completion macrostep**: the triggering event still selects at most one transition, and then the engine runs eventless transitions and engine-internal events to quiescence before it seals a single journal record.

The defining constraint is the one the release already promises and this plan must not break: **a definition that uses none of these features must produce byte-identical journal records, state hashes, and traces.** The new machinery is inert by construction — no eventless transitions, no `raise`, no `final` states means exactly one microstep and no new record key. Every existing store keeps folding, every golden keeps passing, and `fsm.state/2` does not move, because the internal event queue is drained inside the macrostep and is empty at every sealed state. That inertness is not a hope; workstream 0046 makes it a test.

The second constraint is determinism, and it is why this plan bounds everything it adds. Run-to-completion is where naive engines hang: a guardless eventless cycle spins forever, and a `raise` loop fills memory. Here the cycle is refused at admission when it is statically certain (`def/eventless_cycle`), and the loop is bounded at run time by a microstep ceiling whose exhaustion rejects the **whole** macrostep atomically rather than sealing a half-run workflow.

## In scope

- **0042 — Macrostep foundations.** The `step/micro.rs` driver that turns the existing single-transition `step`, `create`, and `poll_deadline` into macrosteps: an ordered quiescence loop, the internal event queue that lives only for the duration of one macrostep, the `MAX_MICROSTEPS` ceiling with `run/microstep_limit`, the macrostep evaluation budget and its admission argument, and all-or-nothing atomicity across every microstep. Plus the split of the 746-line `spec/validate.rs` into a module directory, so the three feature workstreams below can each own their own validation file instead of queueing behind one another.
- **0043 — Eventless transitions.** `on` becomes optional; an omitted `on` is an eventless transition, keyed internally under the already-reserved `$always` sentinel. Selection order, shadowing, and the interaction with guards; the static cycle analysis that refuses a definitionally non-terminating machine (`def/eventless_cycle`) and warns about the guarded case it cannot decide.
- **0044 — Internal events.** An event declaration may be marked `internal: true`, which makes it raiseable from a block and refuses it from the external send path (`req/event_internal`). The new `raise` block key alongside `do` and `emit`, its typed `with` payload, its per-block limit, and the FIFO ordering rule that makes a queue drained across exit → transition → entry blocks deterministic.
- **0045 — Done events.** `final: true` on a leaf, distinct from and orthogonal to `terminal: true`: entering it ends its **parent compound** rather than the machine, and raises the engine-generated `$done.state.<compound>`. A region whose leaf becomes `terminal` raises `$done.region.<region>` — the join primitive parallel definitions have never had. Both ride the internal queue built in 0044, both use the `$` prefix `def/reserved_ident` has always reserved, and neither is externally sendable.
- **0046 — Persistence & compatibility.** The optional `microsteps` array on `event_applied` / `deadline_applied` / `instance_created` bodies, whose absence is the compatibility anchor; fold and replay of macrostep records; and the inertness suite that proves a non-reactive definition's bytes did not move.
- **0047 — Surface & proof.** Microstep-aware traces and `explain`, the analysis and diagram rendering of eventless and done transitions, `simulate` and `enabled_events` under cascades, the naive-oracle differential extended to macrosteps, and the SPEC and README restatement of the one-event guarantee.

## Out of scope

Cross-instance signalling and sub-machine invocation — a `raise` reaches this instance and nothing else; composition is plan 0010, which depends on the done events this plan builds. Any change to the MCP surface beyond what the new trace fields imply (plans 0012–0014). Deadline semantics: a deadline still fires only when a caller polls, and a macrostep neither reads a clock nor creates one. History semantics: capture still binds from the pre-transition configuration, now meaning pre-*macrostep*, which 0046 pins. Migration of running instances onto a definition that gained reactive features (plan 0011). Any relaxation of the single-transition rule for the *triggering* event: exactly one transition is still selected for the event a caller sends, and everything after it is the machine reacting to itself.
