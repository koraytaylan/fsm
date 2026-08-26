---
id: 0012
title: "Live MCP Surface"
status: planned
---
# Scope — Plan 0012

> The server can never speak first. Everything else in this plan follows from that one sentence.

## Why this plan

`initialize_result` in `crates/fsm-cli/src/mcp/serve.rs` advertises `tools.listChanged: false`, `resources.subscribe: false`, `resources.listChanged: false`, and `prompts.listChanged: false`. There is no `logging` capability. `notifications/cancelled` is written to stderr and discarded. There is no `notifications/progress`. The protocol loop blocks in `read_capped_line` until the client sends a line, so nothing the server learns can reach the client until the client asks again.

For most MCP servers that is fine — they answer questions about things that only change when asked. This one is the opposite. Its entire premise is that **things happen while nobody is watching**: a deadline comes due, the executor runs a handler and acks it, a workflow advances three gates overnight. The README sells `fsm serve --read-only` as letting the model "watch its acks and transitions arrive live", and there is no live: `refresh_read_only` re-opens the journal once per incoming request, so the model only ever sees what changed since the last time it asked. The engine is a push system behind a pull-only door.

The second half of the same gap is that **instances are not addressable**. `resources.rs` exposes exactly three things: `fsm://docs/spec`, `fsm://docs/examples`, and `fsm://machine/{id}`. The live objects — the running workflows, the thing the whole system is about — have no URI. There is nothing to subscribe to, nothing to return as a `resource_link` from a tool result, and nothing for a user to attach to a conversation in a client that supports it.

This plan closes both. It gives the server a way to speak — one output multiplexer, one background thread, no async runtime and no new dependency — and gives it something worth saying: instance resources, subscriptions on them, list-changed notifications, structured logging, progress on long calls, and cancellation that is honest about what it can and cannot interrupt.

The design constraint is the one that governs every process in this workspace: **blocking, zero-dependency, and deterministic where it can be.** The change feed is a poll loop over `Store::open_read_only` on the same interval the executor already uses, not a filesystem watcher — `open_read_only` takes no lock and returns one consistent prefix, so watching never perturbs a writer, and a poll loop behaves identically on all three CI platforms. Notifications are unordered with respect to responses by protocol, but this plan pins the one ordering that matters — a notification is never interleaved *inside* another message's bytes — with a single mutex around whole-line writes.

The honest limit, stated up front and documented rather than discovered: **a single tool call is not interruptible mid-step.** The engine's operations are bounded by the evaluation budget and are short by construction; cancellation applies between tool calls and at the coarse loop boundaries that genuinely take time. A plan that claimed otherwise would need to thread a cancellation token through the pure core, which would buy nothing and cost the core its purity.

## In scope

- **0057 — Speaking at all.** The output multiplexer that lets more than one producer write whole JSON-RPC lines to stdout safely; the capability negotiation and `initialize` result that advertise what this plan adds; and the shutdown path that joins the background thread cleanly on EOF so a client disconnect never leaves an orphan thread writing to a closed pipe.
- **0058 — Addressable instances.** `fsm://instance/{id}` and `fsm://instance/{id}/history` as first-class resources with templates, and `resource_link` content in the results of the tools that produce or touch an instance, so a model that creates a workflow gets a handle to it rather than a string it has to reassemble.
- **0059 — Subscriptions and the change feed.** `resources/subscribe` and `resources/unsubscribe`, the subscription registry, the background journal poller that turns a change in `last_seq` into `notifications/resources/updated` for exactly the subscribed URIs affected, and `notifications/resources/list_changed` when a machine is defined or an instance created.
- **0060 — Logging, progress, and cancellation.** The `logging` capability with `logging/setLevel` and `notifications/message`, so executor ticks and store warnings reach the client instead of only stderr; `notifications/progress` for calls that carry a `progressToken`; and a cancellation registry that is checked between tool calls and at the coarse boundaries where time is actually spent.
- **0061 — Proof and docs.** Byte-exact golden transcripts for a live session including notifications, an ordering and interleaving suite, and the documentation of what the server now pushes and what it still will not.

## Out of scope

Tool annotations, `completion/complete`, and elicitation — plan 0013, which builds on this plan's capability plumbing. The audit tools that will want progress notifications — plan 0014. HTTP transport, sessions, and authentication — plan 0015; this plan is stdio-only and everything it adds must work identically once a second transport exists. `notifications/tools/list_changed` remains `false` and the tool set stays static: a per-machine tool surface would make `tools/list` depend on store contents, and no client is required to re-read it. Any change to engine semantics, the journal, or the store's write path.
