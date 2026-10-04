---
id: 0020
title: "Autonomous Embedded Execution"
status: planned
---
# Scope — Plan 0020

> A workflow accepted by an embedded server must continue while its client is quiet.

## Why this plan

The current embedded stdio loop calls `drive_executor` after an incoming
request; a pending handler, due deadline, retry, or interrupted advance waits
for another request before the next tick observes it. Subscribing does not
drive execution. The discovery resource accurately reports
`progress: "client_requests"`, but that is a requirement for client
cooperation in work the server has already accepted.

HTTP has a separate gap: `http::run_http` opens the writer for
`ServeMode::Embedded` but does not retain its executor, and the endpoint
serializes calls through `SerializedWriter` without running that executor.
Transport choice therefore changes execution behavior.

Adding a timer around the existing request loop is insufficient: the writer
must still have one owner, handler termination and client elicitation must
not block that owner, and a client that stops reading must not consume
unbounded memory or stop every other client's workflow.

## In scope

- **0089 — The execution host.** One owner of the writer and executor state,
  bounded command admission, handler work separated from store mutation,
  a fair timer-driven loop, and bounded session channels whose client waits
  never hold the writer.
- **0090 — Transport integration and the public contract.** Connect stdio
  and HTTP to that host; preserve mode restrictions, request idempotency,
  notification ordering, and HTTP session isolation; distinguish a
  transport session ending from the host stopping; publish truthful
  discovery metadata and replace the client-polling instructions.

Completion means that real stdio and HTTP clients can submit a neutral
workflow and then send no further requests while its handlers, retries,
deadlines, and compensation reach the expected durable state, with another
eligible request still answerable while a handler is running.

## Out of scope

There are no background timers, clocks, or I/O in `fsm-core`: the host is a
caller that supplies `now_ms` and explicitly invokes the existing deadline
poll operation. Transition selection, state hashes, journal records,
request-id derivation, and at-least-once external effects retain their
existing contracts. This plan does not promise a hard latency bound during
an operating-system or durable-storage stall.

Distributed execution, multiple writers, an async runtime, new dependencies,
and dynamic conversion of handler output into domain events are excluded.
Machine–handler compatibility checking belongs to plan 0021; process-tree
containment, termination, shutdown, and restart exclusion belong to plan
0022; release and sustained operational evidence belong to plan 0023.

## Integration prerequisites

Plan 0022's lifecycle contract is an external prerequisite for final
integration: host stop and session teardown must use its supervised
shutdown path, and an execution slot cannot be reused until its previous
attempt has reached that path's safe terminal state. The tasks here may
develop against an injectable supervisor adapter, but mock lifecycle tests
do not satisfy the final real-process transport tests. Cross-plan ordering
is coordinator-owned and is not represented by unresolved task IDs in this
plan's DAG.
