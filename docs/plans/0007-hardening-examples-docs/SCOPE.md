# Scope — Plan 0007

> Attack it, randomize it, document it — then call it initial release.

## Why this plan

Plans 0001–0006 prove correctness on curated fixtures. Nothing yet attacks the hand-rolled parsers with hostile bytes, runs randomized whole-stack operation sequences, measures determinism and latency on generated machines, or explains the engine to a first-time user. This plan closes those gaps: an out-of-workspace fuzz crate (the one documented exception to zero dependencies, never part of the shipped binary's graph), an in-tree zero-dependency chaos suite, seeded machine generators feeding a determinism and performance suite, three worked example machines with full walkthroughs, and the README, SPEC appendices, licenses, and release checklist that make initial release shippable.

## In scope

- **0032 — Adversarial.** The `fuzz/` side crate with six cargo-fuzz targets over the parsers and the serve loop, plus an in-tree seeded chaos suite driving random valid-and-invalid operation sequences through the full stack with journal verification after every sequence.
- **0033 — Property.** The ~120-line xorshift64* generator for well-formed random machines and event sequences, and the determinism suite that refolds generated journals with and without snapshots asserting bit-identical state hashes, plus the worst-case performance smoke test against the ~250 ms per-request budget.
- **0034 — Examples.** Three worked machines under `examples/` — `expense_approval`, `order_lifecycle`, `invoice_matching` — validated and driven by tests, then documented in `docs/EXAMPLES.md` with full CLI transcripts, which doubles as the `fsm://docs/examples` resource content.
- **0035 — Docs & Release.** The README (thesis, 60-second demo, MCP client setup, the guarantees table with honest non-claims), SPEC.md completion (error-code, limits, and format-version appendices), the license files, and `docs/RELEASE.md` with the initial release definition of done.

## Out of scope

Parallel regions and deadline timers (spec-reserved for a later version), group-commit durability modes, a streamable HTTP transport, and any packaging or registry publishing beyond `cargo install --path`. No new engine semantics of any kind: this plan only hardens, proves, and documents what plans 0001–0006 built.
