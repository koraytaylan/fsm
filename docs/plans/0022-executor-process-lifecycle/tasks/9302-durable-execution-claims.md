---
id: durable-execution-claims
title: "Durable Execution Claims"
workstream: "0093"
kind: task
depends_on:
  - lifecycle-containment-feasibility
gated: false
touches:
  - crates/fsm-core/src/record.rs
  - crates/fsm-core/src/record/
  - crates/fsm-core/src/replay/
  - crates/fsm-core/src/error.rs
  - crates/fsm-core/tests/
  - crates/fsm-store/src/
  - crates/fsm-store/tests/
  - crates/fsm-embed-acceptance/src/
  - crates/fsm-embed-acceptance/tests/
  - crates/fsm-cli/tests/fixtures/
  - docs/SPEC.md
  - docs/API-POLICY.md
  - docs/EMBEDDING.md
  - docs/RELEASE.md
status: in_progress
merged_as: ""
---
# Durable Execution Claims

One durable unresolved claim must exclude every competing launch for its
effect, even if no executor currently holds the writer.

**Steps:**

1. Specify the claimed/stopped record schemas, store-scoped monotonic run
   identity, pending-effect precondition, immutable handler fingerprint,
   bounded outcome, retry-policy snapshot and native domain identity before
   implementing them; stopped-but-unsettled results retain exclusive ownership.
2. Implement store mutators that allocate claims atomically under the writer,
   reject duplicate unresolved ownership, stopped-but-unsettled predecessors,
   ineligible journaled retry/backoff and stale run completions, and fold
   interrupted or unknown results without inventing acknowledgements; consume
   a stopped result atomically with its ack, failed-attempt or explicit
   interruption disposition so neither the result nor retry count can be
   applied twice, and interruption preserves pending state and retry count.
3. Preserve claims and stopped-but-unsettled results through snapshots,
   reconstruction, seals, archives, read-only inspection and verification;
   introduce bounded counts and serialized-field limits with exact accounting.
4. Version the store and forward migration so older binaries cannot ignore
   claims, keep old records/hash bytes unchanged, and specify the offline,
   verified-quiescence prerequisite for stores used by pre-claim executors.
5. Register new stable errors, update the normative/API/embedding/release
   contracts, and expose usable public constructors without adding I/O to core.

**Tests:**

- New store integration tests race two writers for the same effect and prove
  exactly one claim; cancelling or acknowledging after observation but before
  the claim makes the stale claim fail without allocating a run.
- Pause immediately after a stopped record becomes durable but before its
  ack/failed-attempt disposition: a competing production claim is refused,
  and after atomic single-consumption settlement the next claim is accepted
  only at the recorded retry-eligibility boundary, never one tick earlier.
- Crash/torn-tail cases at every claim and stopped-record durability boundary
  preserve exclusion, identity monotonicity and the original journal chain.
- Each previously supported store version migrates with unchanged historical
  bytes; newer-format refusal, stale snapshots and read-only non-mutation pass.
- Seal/archive/reopen retains active claims and stopped results; record
  tampering, count/size limit-plus-one and mismatched effect/run identities
  are rejected through production mutators and loaders.
- Core determinism/goldens, spec appendix, zero dependencies, external embed
  acceptance and the complete stable/MSRV native store gate remain green.

- **Done when:** the production store API and its crash/migration/seal tests prove that ownership excludes every successor launch through verified stop and atomic single-consumption settlement, admits a retry only when its durable policy allows it, and preserves historical bytes and documented compatibility.
