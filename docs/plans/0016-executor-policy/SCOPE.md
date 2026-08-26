---
id: 0016
title: "Executor Policy"
status: planned
---
# Scope — Plan 0016

> `fsm` is an MCP server that cannot call an MCP tool, and an effect runner with no policy between "it worked" and "give up".

## Why this plan

Plan 0008 built the executor deliberately thin, and that was right: it proved the loop, the idempotency derivation, and the crash resumption before adding anything that could complicate them. Three gaps were left, and they are now the ones an operator meets first.

- **A transient failure is a permanent one.** `HandlerSpec` has exactly five keys — `effect`, `argv`, `timeout_ms`, `on_ok`, `on_failed` — and no retry. A handler whose network blipped acks `failed`, the machine's failure path fires, and a workflow takes its compensating branch over something that would have succeeded a second later. The only remedy today is to model retry inside the machine, which puts an infrastructure concern in a business definition and burns journal records doing it.
- **Nothing bounds concurrency.** There is no in-flight cap anywhere in `fsm-execute`; `MAX_SETTLED_PER_INSTANCE` bounds settle batching and nothing else. An outbox holding five hundred pending effects spawns five hundred subprocesses, and one instance with a long queue can starve every other instance in the store.
- **`fsm` is never an MCP client.** Effects run as subprocess `argv` and nothing else. The obvious composition — an effect whose handler *is another MCP server's tool* — does not exist, so a workflow engine that speaks MCP fluently in one direction has to shell out to reach the ecosystem it already belongs to. That is the gap with the largest ratio of value to code, because the executor already spawns subprocesses and the workspace already has a hand-rolled JSON-RPC implementation.

The design constraint is the one plan 0008 established and this plan must not weaken: **the journal is the executor's only memory.** A retry counter in process memory would be lost on the restart that retries exist to survive, and two executors would disagree about how many attempts had happened. So every attempt is journaled, the attempt count and the backoff deadline are both derived from records, and a fresh process reaches the same conclusion its killed predecessor did. That costs one record per attempt, and it buys an audit trail that can answer "how many times did we try, and when" without inference.

The second constraint is that the handler table stays **the security boundary**. Adding a second handler kind must not widen it: an MCP handler names a literal rooted `argv[0]` exactly as a subprocess handler does, names one tool, and passes a template the table declares. No handler may be constructed from data a machine emitted, then or now.

## In scope

- **0074 — Attempt accounting and retry.** The `effect_attempted` record kind and the store operation that writes it; the `retry` block on a handler with the failure classes it applies to; and the scheduler rules that derive attempt count from the journal so a restarted executor resumes mid-retry rather than restarting the count.
- **0075 — Backoff and dead letters.** The deterministic backoff schedule computed from the last attempt's record timestamp; exhaustion, which acks `failed` with a distinguishable cause so a machine's failure path still fires; and the dead-letter report an operator needs to find effects that gave up.
- **0076 — Concurrency and fairness.** A global in-flight cap and a per-instance cap, both applied deterministically so the same observation always produces the same directives; and round-robin fairness across instances so one busy workflow cannot starve the store.
- **0077 — MCP handlers.** A second handler kind that launches an MCP server as a subprocess, performs `initialize` and one `tools/call`, and maps the structured result into the ack — with the same timeout, the same output bounds, and the same closed-command security boundary the subprocess kind already has.
- **0078 — Proof and docs.** A chaos harness over interrupted retries, exhaustion, and MCP handlers, and the operator documentation for every new table key.

## Out of scope

Retry policy expressed in a machine definition. Retry is an infrastructure concern and the handler table is where infrastructure concerns belong; a machine that wants a bounded number of *business* attempts models them as states, and that remains the right way to express it. Circuit breaking, adaptive rate limiting, and any policy that depends on aggregate history rather than one effect's own record — those need state the journal does not hold and would make a tick's decisions depend on something other than its inputs. Distributing handlers across hosts: the executor is single-node and stays so. Long-lived MCP client connections pooled across effects — each effect gets its own process, for the same reason each subprocess handler does. Any change to engine semantics or to the rule that an ack never drives a transition.
