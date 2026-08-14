# Plan 0007 — Hardening, Examples & Docs — 📋 Planned

The roll-up row in [../STATUS.md](../STATUS.md) must stay in sync with this file. Task-level truth lives in [tasks/](tasks/) frontmatter; Makina's integration coordinator updates both layers.

- **Status:** 📋 Planned.

- **Goal:** harden the engine adversarially, prove determinism and latency budgets on generated machines, and ship the worked examples, completed documentation, licenses, and release checklist for initial release.
- **Root cause:** plans 0001–0006 prove correctness on curated fixtures only — no hostile-input fuzzing, no randomized whole-stack sequences, no scale determinism evidence, and no first-user documentation exist yet.
- **Approach:** land the out-of-workspace fuzz crate and the in-tree chaos suite in parallel, feed a seeded machine generator into the determinism and performance suite, then finish the examples with test-driven walkthroughs and complete the README, SPEC appendices, licenses, and release checklist in dependency order.
- **Progress:** 0/8 tasks done; 0 blocked; 0 dropped.
- **Integration:** `planned`; run —; base `develop` @ `2d2f8ce57b53bf773ab80b2200e25ab40a8f4afd`; validation base —; mode —; final integration —.
- **Exceptions:** — (coordinator-owned blocked/dropped reasons are recorded here).
- **Outcome:** Fuzzing, chaos, and determinism suites guard the engine; three worked example machines, the completed SPEC, and the README ship a releasable initial release.

_Last updated: 2026-08-14, against `develop` @ `2d2f8ce`._
