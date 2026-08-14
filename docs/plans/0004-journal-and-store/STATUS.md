# Plan 0004 — Journal & Store — 📋 Planned

The roll-up row in [../STATUS.md](../STATUS.md) must stay in sync with this file. Task-level truth lives in [tasks/](tasks/) frontmatter; Makina's integration coordinator updates both layers.

- **Status:** 📋 Planned.

- **Goal:** give every mutation a single durable commit point and make every determinism claim checkable by refolding the chain.
- **Root cause:** plans 0001–0003 leave a pure engine with no persistence; durability, idempotency, tamper-evidence, and crash-safety all hinge on an append-only hash-chained journal that does not yet exist.
- **Approach:** land the record envelope and pure replay fold in core, the append/fsync/lock/recovery machinery in the shell, materialize machine and instance stores from the fold with the dedup-first check order, and prove the stack with a kill -9 harness and replay-determinism suites — fixtures-first throughout.
- **Progress:** 0/10 tasks done; 0 blocked; 0 dropped.
- **Integration:** `planned`; run —; base `develop` @ `2d2f8ce57b53bf773ab80b2200e25ab40a8f4afd`; validation base —; mode —; final integration —.
- **Exceptions:** — (coordinator-owned blocked/dropped reasons are recorded here).
- **Outcome:** Every mutation commits through a hash-chained fsync'd journal that recovers, verifies, and replays bit-identically, surviving a kill -9 crash harness.

_Last updated: 2026-08-14, against `develop` @ `2d2f8ce`._
