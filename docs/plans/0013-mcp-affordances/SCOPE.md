---
id: 0013
title: "MCP Affordances"
status: planned
---
# Scope — Plan 0013

> The server already knows which tools are safe, which ids exist, and which events are enabled. It tells the client none of it.

## Why this plan

Three affordances the protocol offers are unused, and in each case the server is already computing the answer for its own purposes:

- **No tool annotations.** `tools_list_result` emits `name`, `description`, `inputSchema`, and `outputSchema` and nothing else. There is no `title`, and no `annotations` object — no `readOnlyHint`, `destructiveHint`, `idempotentHint`, or `openWorldHint`. The server already maintains `MUTATING_TOOLS`, counted from the store code precisely so the read/write split cannot drift; the host has to guess it. The consequence is concrete: a host that could auto-approve the eight read tools and gate the six writers instead treats `instance_cancel` exactly like `instance_get`. And this server has an unusually strong claim to make — every mutating tool is keyed by `request_id` and is exactly-once by construction — which `idempotentHint` exists to express and which nothing currently says.
- **No completion.** `completion/complete` is not implemented, so a model spelling a machine id, an instance id, or an event name is guessing or round-tripping. The server holds all three. Better, the `2025-06-18` revision passes previously-resolved arguments as context on a completion request, which means an `event` argument can be completed from `enabled_events` of the instance already named — the engine's own analysis, offered at the moment somebody is deciding what to send.
- **No elicitation.** A workflow that reaches a human gate stalls until somebody thinks to send the event. The client can be asked, and the ask can be typed: an event's declared `fields` are already a flat set of primitives, which is exactly the shape MCP elicitation restricts a schema to. The engine can therefore generate the form from the machine definition, and the answer arrives as an ordinary typed payload that is validated and journaled like any other.

None of this changes engine semantics and none of it is speculative work. It is the server publishing facts it already holds.

The design constraint is the project's oldest locked decision: **the server never parses natural language.** Elicitation is compatible with that and only because of how it is used here — the request carries a schema derived from typed declarations, the response is structured data, and the values are validated against the same declarations any external `instance_send` is validated against. No free text is interpreted, no intent is inferred, and the journal records the event, never the conversation that produced it. An elicitation that returned prose for the server to read would violate the rule and is out of scope permanently, not merely for now.

## In scope

- **0062 — Annotations and titles.** `title` and a complete `annotations` object on every tool, each hint derived from a fact the code already owns rather than from a hand-maintained table; and `title` on every resource, resource template, and prompt.
- **0063 — Completion.** The `completions` capability and `completion/complete`; completion of the `{id}` variable in the machine and instance resource templates; and the driving prompts whose arguments make event-name completion reachable, using the resolved-argument context so `event` completes from the named instance's own `enabled_events`.
- **0064 — Elicitation.** Detection of the client's `elicitation` capability from the `initialize` parameters; the inbound-response routing the serve loop needs before it can ask the client anything and wait for an answer; the `instance_elicit` tool; and the schema derivation that turns an event's declared fields into a flat elicitation schema and the response back into a typed payload.
- **0065 — Proof and docs.** Byte-exact goldens for the new list shapes and a full elicitation exchange, and the documentation of what each hint claims and what the elicitation path will and will not do.

## Out of scope

Sampling. A server that asked the model to decide something would be inferring intent, which is the locked decision this plan is careful to respect; the engine decides from guards, and a human decides through elicitation. Completion of **tool** arguments: the protocol defines completion for prompt arguments and resource template variables only, and inventing a fourth reference type would be a private extension no client would call. Roots. Any change to the tool set beyond the one elicitation tool, to engine semantics, to the journal, or to the transport — plan 0015 owns the second transport, and every affordance here must work identically once it exists.
