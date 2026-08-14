# Scope — Plan 0004

> One hash-chained journal is the whole truth; everything else is a disposable cache.

## Why this plan

Plans 0001–0003 produce a pure engine: nothing persists, so no claim about durability, idempotency, or tamper-evidence is yet checkable. This plan gives every mutation a single commit point — an append-only, hash-chained, fsync'd journal — and materializes machines and instances by folding it back through the pure core, so "replay reproduces state hashes bit-identically" becomes a test rather than a promise. Recovery refuses to guess: a torn tail is quarantined only by an explicit repair command, and interior corruption is reported precisely and never rewritten.

## In scope

- **0017 — Records.** The core record envelope (ten kinds, domain-separated chain hash), pure per-record verification, and the pure replay fold with a sink through which the shell derives rebuildable indexes.
- **0018 — Append.** The append-only segment writer with per-record file fsync and directory fsync, rotation, the genesis record, the injected clock, and the single-writer lock.
- **0019 — Recovery.** The startup verification walk with exact classification — clean, torn tail, interior corruption — and the quarantine-then-truncate repair for torn tails only.
- **0020 — Store.** Machines (content-addressed, idempotent define) and instances (dedup-first request pipeline, per-instance history index, self-hashed snapshots) materialized from the fold.
- **0021 — Proof.** A kill -9 crash harness over 1,000 seeded iterations and replay-determinism suites over the real append path.
- **0022 — Docs.** The normative §Journal section of `docs/SPEC.md`.

## Out of scope

Surfacing verify/replay/repair as commands (plan 0005), the MCP tool surface (plan 0006), fuzzing of record lines (plan 0007), and group-commit batching (a documented future opt-in; v1 fsyncs every record before responding).
