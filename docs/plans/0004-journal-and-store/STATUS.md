# Plan 0004 — Journal & Store — ✅ Complete

The roll-up row in [../STATUS.md](../STATUS.md) must stay in sync with this file. Task-level truth lives in [tasks/](tasks/) frontmatter; Makina's integration coordinator updates both layers.

- **Status:** ✅ Complete.

- **Goal:** give every mutation a single durable commit point and make every determinism claim checkable by refolding the chain.
- **Root cause:** plans 0001–0003 leave a pure engine with no persistence; durability, idempotency, tamper-evidence, and crash-safety all hinge on an append-only hash-chained journal that does not yet exist.
- **Approach:** land the record envelope and pure replay fold in core, the append/fsync/lock/recovery machinery in the shell, materialize machine and instance stores from the fold with the dedup-first check order, and prove the stack with a kill -9 harness and replay-determinism suites — fixtures-first throughout.
- **Progress:** 10/10 tasks done; 0 blocked; 0 dropped.
- **Integration:** `complete`; run — (the single Makina run, `01M018Z4PKM8RQSARHTNJ824TX`, marked the plan failed and every later task skipped after task 0101 tripped the reviewer cap on a footprint false positive — four test-output files sat outside its `touches`. The plan was therefore landed directly on `develop`, one commit per task in dependency order, and sealed here after the fact.); base `develop` @ `2d2f8ce57b53bf773ab80b2200e25ab40a8f4afd`; validation base —; mode `direct-to-develop`; final integration `d36b954` (the `v0.1.0` tag commit).
- **Exceptions:** — (coordinator-owned blocked/dropped reasons are recorded here).
- **Outcome:** Every mutation commits through a hash-chained fsync'd journal that recovers, verifies, and replays bit-identically, surviving a kill -9 crash harness.

_Last updated: 2026-08-14, against `develop` @ `2d2f8ce`._
