# Scope — Plan 0005

> The whole engine, ad hoc from a terminal — with the structured output frozen byte-for-byte before MCP ships.

## Why this plan

After plan 0004 the engine and store are reachable only from tests. The CLI is the first human-usable surface, and it deliberately lands before the MCP server: every command's `--json` output is captured into committed fixtures that plan 0006's `structuredContent` must match byte-for-byte — the cheapest possible guarantee that the two surfaces can never drift. The parser is table-driven, and help text renders from the same table that dispatches, so documentation cannot drift from behavior either.

## In scope

- **0023 — Frame.** The table-driven argument parser with generated help, and the single output frame: one renderer for human text, error rendering with hints to stderr, the exit-code map, `--json` mode, and config/data-dir precedence.
- **0024 — Offline Commands.** `validate`, `simulate`, `docs`, `version`, and the pure Mermaid/DOT diagram exporters with an optional instance overlay.
- **0025 — Store Commands.** `machine add|ls|show|analyze` and the full instance lifecycle — `new|send|ack|cancel|annotate|show|ls|history` — plus `explain` for recomputed decision traces.
- **0026 — Ops Commands.** `journal verify|replay`, `doctor`, and `repair --truncate-torn-tail` with the granular integrity exit codes.
- **0027 — Proof.** End-to-end golden sessions against the real binary and the per-command `--json` fixtures that pin the structured-output contract for plan 0006.

## Out of scope

The MCP tool surface and its transcripts (plan 0006), fuzzing and chaos suites (plan 0007), and any new engine or store semantics — the CLI only surfaces what plans 0001–0004 landed.
