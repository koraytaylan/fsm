# Scope — Plan 0006

> Turn the walking skeleton into the full tool surface — every response carries the whole situation, every error teaches its fix.

## Why this plan

Plans 0001–0005 proved the transport handshake, the engine, the journal, and the CLI. What an LLM actually touches is this plan: thirteen tools whose schemas and descriptions are the model's entire manual, an error channel where every domain failure arrives in-band with a mechanically generated hint, and resources/prompt/instructions that teach the golden loop. The surface is pinned two ways — byte-exact per-revision transcripts, and a naive-caller suite proving that following each error's hint fixes the call in one step.

## In scope

- **0028 — Protocol.** Harden the serve loop to the full lifecycle: the complete version-negotiation table, the initialize gate, batch rejection under every revision, the notification policy, the panic hook (stderr + abort), EOF shutdown, and the error-channel decision rule (JSON-RPC errors only for envelope faults; all domain errors in-band).
- **0029 — Tools.** The 13-tool registry with input/output schemas as canonical values, argument validation with field-by-field diagnostics, the shipped description prose under a hard token budget, and dispatch into the store and core with full post-state in every mutating response.
- **0030 — Extras.** Resources (`fsm://docs/spec`, `fsm://docs/examples`, `fsm://machine/{id}`), the single `author_machine` prompt, and the `initialize.instructions` text that teaches the golden loop.
- **0031 — Proof.** Byte-exact full-session golden transcripts per negotiated revision, the CLI-parity test (`--json` fixtures byte-match `structuredContent`), and the naive-caller error-recovery suite with error-code coverage.

## Out of scope

Streamable HTTP transport, `listChanged`/`subscribe` notifications, sampling, elicitation, and JSON-RPC batching (rejected by design). Journal verification and repair stay CLI-only — operator actions, not tool surface. Example content beyond a placeholder for `fsm://docs/examples` lands in plan 0007.
