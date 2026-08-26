# Plan 0007 — Hardening, Examples & Docs — ✅ Complete

The roll-up row in [../STATUS.md](../STATUS.md) must stay in sync with this file. Task-level truth lives in [tasks/](tasks/) frontmatter; Makina's integration coordinator updates both layers.

- **Status:** ✅ Complete.

- **Goal:** harden the engine adversarially, prove determinism and latency budgets on generated machines, and ship the worked examples, completed documentation, licenses, and release checklist for the initial release.
- **Root cause:** plans 0001–0006 prove correctness on curated fixtures only — no hostile-input fuzzing, no randomized whole-stack sequences, no scale determinism evidence, and no first-user documentation exist yet.
- **Approach:** land the out-of-workspace fuzz crate and the in-tree chaos suite in parallel, feed a seeded machine generator into the determinism and performance suite, then finish the examples with test-driven walkthroughs and complete the README, SPEC appendices, licenses, and release checklist in dependency order.
- **Progress:** 8/8 tasks done; 0 blocked; 0 dropped.
- **Integration:** `complete`; run — (the single Makina run, `01M018Z4PKM8RQSARHTNJ824TX`, marked the plan failed and every later task skipped after task 0101 tripped the reviewer cap on a footprint false positive — four test-output files sat outside its `touches`. The plan was therefore landed directly on `develop`, one commit per task in dependency order, and sealed here after the fact.); base `develop` @ `2d2f8ce57b53bf773ab80b2200e25ab40a8f4afd`; validation base —; mode `direct-to-develop`; final integration `d36b954` (the `v0.1.0` tag commit).
- **Exceptions:** — (coordinator-owned blocked/dropped reasons are recorded here).
- **Outcome:** Fuzzing, chaos, and determinism suites guard the engine; three worked example machines, the completed SPEC, and the README make the initial release shippable.

_Last updated: 2026-08-14, against `develop` @ `2d2f8ce`._
