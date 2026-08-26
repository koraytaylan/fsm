# Plan 0009 — Reactive Semantics — ✅ Complete

The roll-up row in [../STATUS.md](../STATUS.md) must stay in sync with this file. Task-level truth lives in [tasks/](tasks/) frontmatter; Makina's integration coordinator updates both layers.

- **Status:** ✅ Complete.

- **Goal:** a bounded run-to-completion macrostep — eventless transitions, engine-internal events raised from blocks, and generated done events for finished compounds and regions — so a machine can react to itself between external events, sealed in one atomic journal record, with a definition that uses none of it producing byte-identical bytes.
- **Root cause:** `on` is mandatory, `emit` only reaches the external outbox, and completion is a status rather than a signal — so every advance requires a caller who already knows the answer, there is fork without join, and a compound state cannot report that its inner workflow finished.
- **Approach:** one new pure loop (`step/micro.rs`) around the existing pipeline primitives, running eventless selection before the internal FIFO and rejecting the whole macrostep atomically on any failure or on the `MAX_MICROSTEPS` ceiling; a queue that lives only in a stack frame so `fsm.state/2` never moves; the `$` prefix `def/reserved_ident` already reserves spent on the `$always` key and the `$done.*` generated events; one optional `microsteps` record key whose *absence* on a non-reactive machine is the compatibility anchor; and a naive-oracle differential extended to macrosteps.
- **Progress:** 20/20 tasks done; 0 blocked; 0 dropped.
- **Integration:** `complete`; run — (implemented directly rather than through a Makina run: one commit per task on `develop`, a stable host gate before each, follow-ups as their own `fix:`/`refactor:` commits); base `develop` @ `6f690a97a2a10c7b355db09e88c2383753b21842`; validation base `c5b5620` (the plan as written, corrected in the task notes where the harness falsified it); mode `direct-to-develop`; final integration `32af23c`.
- **Exceptions:** — (coordinator-owned blocked/dropped reasons are recorded here).
- **Outcome:** A machine models a decision as a decision instead of an event somebody has to send, a parallel definition can finally join, and a workflow that used to need three round trips through the outbox settles in one record.

_Task frontmatter is authoritative; this file is the roll-up._
