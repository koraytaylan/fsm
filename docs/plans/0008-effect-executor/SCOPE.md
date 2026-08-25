---
id: 0008
title: "Effect Executor"
status: planned
---
# Scope — Plan 0008

> The engine decides; the executor only ever does what the journal told it to.

## Why this plan

Plans 0001–0007 ship a complete, auditable statechart engine whose effects are an **outbox, not an executor**: a transition can `emit` a named effect into `effects_pending`, and the documented host flow is that *someone* runs the real work, then calls `effect_ack`, then sends the domain event that the guards use to advance. Until now that "someone" has been a human or an LLM driving the MCP tools one call at a time. That means a workflow stalls dead the moment the chat session ends — and the rollback path (the whole point of modeling failure in the machine) can only fire if a session happens to be alive to send `*_failed`. This plan builds the missing half: a **standalone executor process** that watches a store, runs a closed, operator-configured table of effect handlers as subprocesses, acknowledges each outcome into the journal, and polls due deadlines — so a triggered workflow proceeds gate-to-gate unattended while FSM remains the single source of truth for *where you are* and *what is allowed next*.

The defining constraint is one this plan must respect, not relax: **effects never drive transitions, and the executor never improvises.** Every action the executor takes is either (a) running a handler for an effect the machine emitted, or (b) sending a domain event the machine's spec already declares. It holds no private state that the journal cannot reconstruct; a kill -9 at any instant leaves the store coherent and the next executor able to resume.

## In scope

- **0036 — Crate & handler config.** A new `fsm-execute` crate (library, not embedded in `fsm-cli`) honoring the workspace zero-dependency / `forbid(unsafe_code)` posture, and the operator-owned handler table: a config file mapping each declared effect name to exactly one argv template plus timeout policy. The closed command set is the security boundary — no model-emitted shell, ever.
- **0037 — Watcher & scheduler.** The observe loop over `Store::open_read_only` (no lock, coexists with the MCP writer) that surfaces newly pending effects and due deadlines, and the deterministic scheduler that owns wall-clock, decides what to run when, and composes idempotent `request_id`s. All decision logic lives here, pure and separately testable from any subprocess.
- **0038 — Runner & ack.** Spawning handler processes with argv-substitution from effect args, capturing stdout/stderr/exit status under a timeout, and committing the outcome via `ack_effect_outcome` (journalling a bounded stdout digest) followed by the machine-declared success/failure domain event via `send_event`. Kill-on-cancel and crash-resume/orphan policy.
- **0039 — CLI & serve integration.** The `fsm execute` subcommand (validate a handler table, then run the loop) and the resolution of the single-writer conflict between a running MCP `serve` and the executor's need to write acks: the executor owns its writer handle, and serve hands off effect-ack/write paths or runs read-only alongside it.
- **0040 — Proof.** A golden two-process session (writer drives a machine emitting effects; executor observes, runs stub handlers, acks, advances to terminal) and a chaos harness that kills the executor mid-effect and mid-ack, asserting the journal stays coherent and a fresh executor resumes without double-running an effect.
- **0041 — Docs.** The normative *Executing workflows* section covering the outbox contract, the handler-table format, the request_id/idempotency rules an operator must know, and the honest non-claim that the runner is single-node and at-least-once at the process boundary.

## Out of scope

Any change to `fsm-core` semantics — effects, deadlines, and the one-event-one-transition rule are settled and stay untouched. Multi-node coordination, HA, and handler distribution (the store is single-writer by design; the executor inherits that ceiling). Executing handlers reached only through the LLM's own bash tool without persistence (that ad-hoc flow already works and needs no code). Secret management beyond environment-inheritance (handler argv references values, never embeds credentials). Streaming/log-follow UIs.
