---
id: 0008
title: "Effect Executor"
status: planned
---
# Scope — Plan 0008

> The engine decides; the executor only ever does what the journal told it to.

## Why this plan

Plans 0001–0007 ship a complete, auditable statechart engine whose effects are an **outbox, not an executor**: a transition can `emit` a named effect into `effects_pending`, and the documented host flow is that *someone* runs the real work, then calls `effect_ack`, then sends the domain event that the guards use to advance. Until now that "someone" has been a human or an LLM driving the MCP tools one call at a time. That means a workflow stalls dead the moment the chat session ends — and the rollback path (the whole point of modeling failure in the machine) can only fire if a session happens to be alive to send `*_failed`. This plan builds the missing half: a **standalone executor process** that watches a store, runs a closed, operator-configured table of effect handlers as subprocesses, acknowledges each outcome into the journal, and polls due deadlines — so a triggered workflow proceeds gate-to-gate unattended while FSM remains the single source of truth for *where you are* and *what is allowed next*.

The defining constraint is one this plan must respect, not relax: **effects never drive transitions, and the executor never improvises.** Every action the executor takes is either (a) running a handler for an effect the machine emitted, or (b) sending a domain event the machine's spec already declares. It holds no private state that the journal cannot reconstruct — what still needs running is `effects_pending`, and what has already been written is the journal's own claimed-`request_id` map — so a kill -9 at any instant leaves the store coherent and the next executor able to resume, re-running at most the single handler that was in flight and journaling exactly one ack for it.

## In scope

- **0036 — Crate & handler config.** A new `fsm-execute` crate (library, not embedded in `fsm-cli`) honoring the workspace zero-dependency / `forbid(unsafe_code)` posture, and the operator-owned handler table: a config file mapping each declared effect name to exactly one argv template, a timeout, and the machine-declared domain event to send on each outcome. The closed command set is the security boundary — no model-emitted shell, ever.
- **0037 — Effect resolution, watcher & scheduler.** The re-derivation that turns an opaque `{instance}/{seq}/{k}` pending-effect id back into its effect **name and evaluated args** by replaying the one record that emitted it (the store surfaces ids and nothing more); the observe loop over `Store::open_read_only` (no lock, coexists with the MCP writer) that surfaces pending effects, already-acked-but-unadvanced effects, due deadlines, and the journal's claimed request ids; and the deterministic scheduler that decides what to run when — from one `now_ms` handed to it, holding no clock of its own — and composes idempotent `request_id`s. All decision logic lives here, pure and separately testable from any subprocess, and every decision is taken from journal-derived facts so a fresh process reaches the same one.
- **0038 — Runner & ack.** Spawning handler processes with argv-substitution from effect args, capturing stdout/stderr/exit status under a timeout, and committing the outcome via `ack_effect_outcome` (journalling a bounded, digest-backed stdout capture) followed by the table-named, machine-declared success/failure domain event via `send_event`. Kill-on-cancel, kill-children-on-shutdown, and the honest orphan boundary when the executor is killed outright.
- **0039 — CLI & serve integration.** The `fsm execute` subcommand (validate a handler table, then run the loop) and the resolution of the single-writer conflict between a running MCP `serve` and the executor's need to write acks: the executor takes the writer only for the ticks that write, and serve either runs read-only alongside it for monitoring or hosts the same loop in-process.
- **0040 — Proof.** A golden two-process session (writer drives a machine emitting effects; executor observes, runs a stub handler, acks, advances to terminal) and a chaos harness that restarts the executor at each named point mid-effect, mid-ack, and mid-advance, asserting the journal stays coherent and that a fresh executor resumes to **exactly one ack per effect** — at-least-once execution, exactly-once journaling.
- **0041 — Docs.** The normative *Executing workflows* section covering the outbox contract, the handler-table format, the request_id/idempotency rules an operator must know, what a read-only paired serve can and cannot do, and the honest non-claim that the runner is single-node and at-least-once at the process boundary — plus the per-crate row a fifth workspace crate owes `docs/API-POLICY.md`.

## Out of scope

Any change to `fsm-core` semantics — effects, deadlines, and the one-event-one-transition rule are settled and stay untouched. Multi-node coordination, HA, and handler distribution (the store is single-writer by design; the executor inherits that ceiling). Executing handlers reached only through the LLM's own bash tool without persistence (that ad-hoc flow already works and needs no code). Secret management beyond environment-inheritance (handler argv references values, never embeds credentials). Streaming/log-follow UIs.
