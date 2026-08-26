---
id: audit-surface-proof
title: "Audit Surface Proof"
workstream: "0068"
kind: task
depends_on:
  - instance-annotate-tool
  - degraded-tool-gating
  - explain-step-tool
gated: false
touches:
  - crates/fsm-cli/tests/audit_golden.rs
  - crates/fsm-cli/tests/fixtures/audit/session.expected
status: planned
merged_as: ""
---
# Audit Surface Proof

These tools are only worth having if they are right about a **broken** store, so the suite builds corrupted stores deliberately and checks each tool against the health SPEC says it should report.

**Steps:**

1. Create `crates/fsm-cli/tests/audit_golden.rs` exercising every audit tool against two fixtures: a healthy store and a deliberately corrupted one.
2. **Build the corrupted stores in the test, never commit them as binaries** — flip a byte inside a record for `NonCanonical`, truncate mid-line for `TornTail`, rewrite a `prev` for `ChainBroken` — reusing the technique `crates/fsm-cli/tests/recovery_classification.rs` already establishes. A committed corrupt binary is a fixture nobody can review.
3. Byte-compare a full healthy-store session against `fixtures/audit/session.expected`: `explain_step`, `journal_verify`, `journal_replay`, `store_doctor`, and `instance_annotate` in one stream.
4. For each corrupted fixture, assert the health each tool reports is the one `docs/SPEC.md §Recovery` names, and that the `remedy` string matches SPEC's command **verbatim** rather than approximately.
5. Assert degraded mode end to end: a server against each corrupted store starts, serves the three diagnostic tools with correct results, refuses the rest with the health and remedy, and still accepts `machine_create` with `dry_run`.
6. Assert cross-surface agreement: `explain_step` matches `fsm explain --json`, `journal_verify`'s health matches `fsm journal verify`, and `store_doctor`'s health matches `fsm doctor`, for every fixture. Divergence between the CLI and the tool surface is the failure most likely to go unnoticed.
7. Assert the read-only property directly: after every read-side tool runs against a store, the journal bytes, `VERSION`, and snapshot files are unchanged.
8. Keep the fixture free of any absolute path, temp directory, pid, or timestamp so it compares identically on all three CI platforms.

**Tests:**

- The healthy-session byte comparison against the committed fixture.
- Each of the three corrupted stores reports SPEC's health from `journal_verify` and `store_doctor`.
- Each `remedy` string matches the literal command in `docs/SPEC.md`.
- Degraded mode against each corrupted store: server starts, three tools work, others refuse with health and remedy, `machine_create --dry-run` succeeds.
- Cross-surface agreement for all three tool/CLI pairs across every fixture.
- No read-side tool mutates the store — journal bytes, `VERSION`, and snapshots unchanged.
- `journal_replay` reports `matches: false` on a store whose `state_hash` was tampered, while `journal_verify` on a byte-clean-but-semantically-divergent store reports `Ok` — the pair that justifies both tools existing.
- The suite passes on all three CI operating systems from one fixture.

- **Done when:** `cargo test -p fsm-cli --test audit_golden` passes, corrupted stores are built in-test, every health and remedy matches SPEC verbatim, degraded mode is proven end to end, cross-surface agreement holds, no read tool mutates anything, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
