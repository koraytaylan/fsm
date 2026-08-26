---
id: 0010
title: "Machine Composition"
status: planned
---
# Scope — Plan 0010

> A definition ceiling of 256 states is a promise that composition exists. It does not.

## Why this plan

`def/limit_states` caps a machine at 256 nodes and `def/limit_depth` at 12 levels. Those are good limits — they are what make admission's budget argument work — and they are only defensible if a large workflow can be **built out of small machines**. Today it cannot:

- **There is no invocation.** No state can start another machine and wait for it. A workflow that decomposes into "gather, then review, then settle" has to be one flat definition, or three definitions that no artefact connects.
- **There is no correlation.** `instance_create` takes `tags`, and nothing else relates two instances. A store with ten thousand instances holds no edges at all.
- **There is no signalling.** One instance cannot tell another that something happened; the only path between them runs out through the outbox, through an operator's handler table, and back in through `instance_send` — a subprocess round trip to deliver a fact the engine already had.

Plan 0009 built the missing half of the mechanism without knowing it: `$done.state.*` and `$done.region.*` proved that "a sub-workflow finished" is expressible as an engine-generated internal event on a bounded queue. This plan spends that machinery on a third generated event, `$done.invoke.<slot>`, and adds the persistence and enactment that a *cross-instance* sub-workflow needs.

The design constraint is the one the codebase has already answered twice: **the core is pure, so it cannot create an instance.** It can only say that one should exist. This plan therefore follows the outbox pattern exactly as effects follow it — the core emits a pending invocation, the shell enacts it, the outcome is journaled, and the parent advances on a declared event. Nothing hidden, one record per operation, one fsync per record, every request idempotent under a derived key. Composition costs two extra journal records per child, and that is the honest price of an audit trail that can answer "why does this instance exist" without inference.

The second constraint is determinism, and it is what bounds signalling. A signal targets **exactly one instance id**, evaluated at emit time. There is no broadcast, no tag fan-out, no "send to every instance matching a query" — the set of instances matching a query grows over time, so a replay of the same record would deliver to a different set, and the whole store stops being replayable. The engine already refuses broadcast between parallel regions for the same reason; this plan refuses it between instances for the same reason.

## In scope

- **0048 — Invocation in the core.** The `invoke` declaration on a state — a slot `id`, a **content-addressed** `machine` reference, a typed `with` context projection inward, and a typed `returns` projection outward — with the validation that keeps it honest. The `invocations_pending` outbox that a pure `create`/`step`/`poll_deadline` emits into, the `fsm.state/3` shape that persists it, and the `$done.invoke.<slot>` generated event that plan 0009's queue delivers.
- **0049 — Store enactment.** Two new idempotent store operations, each journaling exactly one record: `invoke_child`, which derives the child instance id from the parent and slot, creates it under the projected context, and journals `instance_invoked`; and `invocation_return`, legal only against a settled child, which journals `invocation_returned` and delivers `$done.invoke.<slot>` into the parent as a macrostep. Plus the cancel cascade, the orphan rules, and the `fsm.state/3` + store `VERSION 9` migration.
- **0050 — Cross-instance signals.** The `signal` block action, its single-target rule, the `signals_pending` outbox, and the `signal_deliver` operation that applies a declared event to the target instance and journals the delivery with both instance ids in one record.
- **0051 — Surface.** Executor directives so composition runs unattended by default; the CLI and MCP tools for a session that drives it by hand; and the instance-tree view and diagram overlay that make a composed workflow legible — including `instance_get` gaining its parent and children.
- **0052 — Proof and docs.** A composition chaos harness that kills between every enactment point, and the SPEC section that makes all of it normative.

## Out of scope

Recursive invocation depth beyond a fixed ceiling, and any cycle in the invocation graph — refused at admission, because a content-addressed `machine` reference makes the graph statically knowable and a cycle statically detectable. Distributed or multi-store composition: a parent and child live in one data directory, single-writer, as everything else here does. Dynamic machine selection — the invoked machine is pinned by hash at authoring time, never chosen at run time from a context value, because that would make a parent's identity stop determining its behaviour. Broadcast signalling and query-based targeting, for the determinism reason above. Migration of a running parent onto a definition whose invocations changed, which is plan 0011.
