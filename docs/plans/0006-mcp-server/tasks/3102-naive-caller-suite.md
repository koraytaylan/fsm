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
status: planned
merged_as: ""
---
# Naive Caller Suite

"Every error teaches the fix" is a testable claim: for each error code, a scripted wrong call must produce a hint whose correction — built mechanically from the error's own data — succeeds in exactly one step.

**Steps:**

1. Ensure `fsm_core::error` exports `pub const ALL_CODES: &[&str]` enumerating every stable code (add it in `crates/fsm-core/src/error.rs` if not yet present).
2. Author `crates/fsm-cli/tests/naive_caller.rs`: a table of scripted wrong calls — number-token payload where a decimal string is required, unknown event name, guard-failing payload, send to a completed instance, ambiguous machine reference, stale `expect_seq`, unknown effect id, malformed and duplicate-key spec JSON, oversized definition, unknown identifier in a guard, scale violation, and the remaining reachable codes — each asserting the expected `code` and that the corrected call derived from the error's `details`/`hint` succeeds in one step.
3. Add the coverage assertion: every code in `ALL_CODES` is exercised by this suite or the golden transcripts, minus an explicit allowlist of infrastructure codes (`io/*`, `store/*`, `internal/*`) each carrying a one-line justification string in the test.

- **Done when:** `cargo test -p fsm-cli --test naive_caller` passes with the one-step-recovery assertion for every scripted code and the coverage check green, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
