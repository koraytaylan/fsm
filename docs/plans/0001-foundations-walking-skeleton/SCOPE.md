# Scope — Plan 0001

> Lay zero-dependency bedrock — JSON, hashing, decimals — and prove the MCP wire end-to-end before any engine exists.

## Why this plan

`fsm` is being built as a deterministic statechart engine exposed over MCP, with a hard constraint: zero runtime dependencies (std only), every line auditable. That means the JSON parser, the SHA-256 implementation, and the exact-decimal arithmetic are ours, and each one replaces a battle-tested crate — so each lands fixtures-first against an external source of truth. The single highest-uncertainty risk is MCP host compatibility (framing, handshake, stdout hygiene), so a walking `fsm serve` skeleton ships in this plan, not at the end, and is pinned by a byte-exact golden transcript from day one.

## In scope

- **0001 — Scaffold.** A two-crate cargo workspace (`fsm-core` pure library, `fsm-cli` binary `fsm`) with pinned manifests, pinned toolchain, and machine-checked policy gates (no unsafe, no third-party dependencies, no stray prints, no floats or clocks in the core).
- **0002 — JSON.** A hand-rolled JSON value model and parser that keeps every number token as its raw string, rejects duplicate keys and lone surrogates, and enforces depth/size limits; plus the single canonical writer (FSM-CJSON) that every hash in the system will depend on.
- **0003 — Hashing.** SHA-256 per FIPS 180-4 with NIST test vectors, plus hex encoding.
- **0004 — Decimal.** Fixed-point decimal on an i128 mantissa with explicit scale: checked add/sub/mul, u256-widened comparison and correctly-rounded division, seven rounding modes, canonical string form — cross-checked against an independent Python `decimal`/integer oracle.
- **0005 — MCP Skeleton.** Newline-delimited JSON-RPC 2.0 types and a blocking `fsm serve` loop that negotiates protocol revision 2025-06-18, answers `initialize`/`ping`/`tools/list`, and serves one stub tool — proven by a byte-exact recorded transcript.

## Out of scope

The expression language, statechart semantics, journal/persistence, the real tool surface, and all documentation beyond code-level docs are later plans (0002–0007). Adding any external dependency is out of scope forever — a guard test enforces it from this plan onward.
