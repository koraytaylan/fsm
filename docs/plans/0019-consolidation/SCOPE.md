---
id: 0019
title: "Consolidation"
status: planned
---
# Scope — Plan 0019

> The gate that is about to prove the riskiest change in this repository's history currently fails on its own test targets.

## Why this plan

`docs/plans/0017-journal-lifecycle` moves the journal format, a store version, recovery, durability, and idempotency fingerprinting. `CONTRIBUTING.md` calls that the high-risk path and demands a specific proof for it. That proof is only as good as the gate it runs against, and the gate has three holes that were invisible while nothing depended on them:

- **`cargo clippy --workspace --all-targets` does not pass — it errors.** A workspace-level `deny(clippy::print_stderr)` is violated by `crates/fsm-core/tests/enumerate_small.rs:796`, so the command fails to compile that target, and roughly 95 further findings sit behind it across files that predate plan 0009. The committed gate in `CONTRIBUTING.md` and `.github/workflows/ci.yml` runs `clippy --workspace` **without** `--all-targets`, which is why this has never been red. Test code is compiled code; a lint that does not reach it does not cover the half of this repository that is tests, and by line count the tests are the larger half.
- **Performance has one measurement and no guard.** `crates/fsm-store/tests/append_latency.rs` is the only performance signal in the workspace, it is `#[ignore]`d, and `docs/RELEASE.md` runs it by hand at release time. Plan 0017 adds a code path that runs during append-adjacent work and a store that opens differently; an unguarded append cost is exactly what regresses under that kind of change, and a number nobody compares is a number nobody notices moving.
- **A provisional API can grow without anybody deciding it did.** `docs/API-POLICY.md` marks `fsm-execute` provisional because it has no outside-workspace acceptance check. That is an honest label, and it is currently unbounded: nothing detects a new public item appearing, so "provisional" quietly means "whatever it happens to expose this release".

None of this is new work in the product sense. It is the difference between a gate that reports and a gate that proves, and it costs far less before plan 0017 than during it.

## Why it goes first

Running a format change against a lint gate that is failing on its own test targets means discovering gate problems and format problems in the same diff, on the highest-risk change in the repository, where the reviewer is the last line of defence. Clear the gate, then spend it.

The same argument applies to the latency guard: a baseline established **before** the journal changes is a baseline; one established after is a description of the new behaviour.

## In scope

- **0087 — The gate.** Clear the `--all-targets` findings starting with the one that is a hard error; widen the committed gate in `CONTRIBUTING.md` and `.github/workflows/ci.yml` so test code is held to the same lints as production code, permanently; and turn the latency harness into a guard with a committed baseline and a tolerance, measured on the CI host rather than guessed.
- **0088 — The provisional boundary.** Enumerate `fsm-execute`'s public surface, declare it, and make an undeclared addition fail a test — so "provisional" bounds what it names instead of whatever the crate happens to expose.

## Out of scope

Changing any behaviour. Every task here is a no-functional-change task, and the standing rule from `CONTRIBUTING.md` applies with full force: a cleanup commit asserts that no byte written to disk changed. Where a lint fix would alter behaviour, the fix is an `#[allow]` with a stated reason, not a rewrite smuggled into a cleanup.

Stabilising `fsm-execute`. It still has no outside-workspace acceptance check, and plan 0017 moves the store underneath it; the honest answer this release is to bound the provisional surface, not to promise it. Stabilisation is a decision for the release after the archive path has actually run.

Adding new lints beyond `--all-targets`, adopting a formatter configuration, or restructuring the CI matrix. The matrix in `.github/workflows/ci.yml` is authoritative for platform coverage and this plan changes what runs in a job, never which jobs exist.

Optimising anything the latency guard measures. The guard's job is to notice a change; acting on one is the work of whichever plan caused it.
