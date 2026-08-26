---
id: naive-caller-suite
title: "Naive Caller Suite"
workstream: "0031"
kind: task
depends_on:
  - tool-dispatch
gated: false
touches:
  - crates/fsm-cli/tests/naive_caller.rs
  - crates/fsm-core/src/error.rs
status: done
merged_as: ""
---
# Naive Caller Suite

"Every error teaches the fix" is a testable claim: for each error code, a scripted wrong call must produce a hint whose correction — built mechanically from the error's own data — succeeds in exactly one step.

**Steps:**

1. Ensure `fsm_core::error` exports `pub const ALL_CODES: &[&str]` enumerating every stable code (add it in `crates/fsm-core/src/error.rs` if not yet present).
2. Author `crates/fsm-cli/tests/naive_caller.rs` encoding exactly the inventory under **Tests**: each table row is (wrong call → expected `code` → correction derived only from the error's `details`/`hint`/embedded data → success on the retry).
3. Add the coverage assertion and its allowlist per the inventory.

**Tests:**

- One-step-recovery rows in `naive_caller.rs`, each asserting the expected code on the wrong call and success on the single corrected retry: `req/number_token` (payload `0.10` as a raw JSON number → resent as the string from the hint's rewrite); `req/event_unknown` (typo'd event name → the suggestion from `details`); `req/field_missing` and `req/field_unknown`; `req/field_scale` (`"1.505"` at scale 2 → the hint's exact-scale rewrite); `run/not_enabled` (guard-failing payload → corrected using the guard trace's variable bindings); `run/unhandled` (event undeclared along the chain → an event picked from the response's `enabled_events`); `run/instance_completed` (send to a terminal instance → `instance_create` then send); `req/machine_ambiguous` (bare name with two versions → a full id from the listed candidates); `req/seq_mismatch` (stale `expect_seq` → re-read, retry with the **same** `request_id` and current seq — also pinning the not-consumed rule); an ack of an unknown effect id → corrected with a listed pending id; a machine_create with duplicate-key spec JSON and one with a `def/*` structural error, each corrected from the finding's `path` + `hint`; `def/limit_*` (an oversized definition → the cap named in the hint respected); `expr/unknown_var` (the Levenshtein suggestion applied verbatim); `expr/scale_cap` (corrected with the `round(…)` rewrite from the hint).
- Coverage assertion: every code in `ALL_CODES` is exercised by this suite or appears in a golden transcript, minus an explicit allowlist for infrastructure codes (`io/*`, `store/*` recovery classes, `internal/*`), where each allowlisted entry carries a one-line justification string; an allowlist entry not present in `ALL_CODES` fails the test (the allowlist cannot rot).
- `ALL_CODES` hygiene, inline in the same suite: non-empty, sorted, duplicate-free.

- **Done when:** `cargo test -p fsm-cli --test naive_caller` passes with the one-step-recovery assertion for every scripted code and the coverage check green, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
