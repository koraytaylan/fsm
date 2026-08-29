---
id: all-targets-clippy-clean
title: "All-Targets Clippy Clean"
workstream: "0087"
kind: chore
depends_on: []
gated: false
touches:
  - crates/fsm-core/tests/enumerate_small.rs
  - crates/fsm-core/tests/enumerate_reactive.rs
  - crates/fsm-core/tests/oracle
  - crates/fsm-core/tests/spec_validate.rs
  - crates/fsm-core/tests/canon_golden.rs
  - crates/fsm-core/tests/tree_build.rs
  - crates/fsm-core/tests/compile_machine.rs
  - crates/fsm-core/tests/diagram_golden.rs
  - crates/fsm-cli/src/cli/diagram.rs
  - crates/fsm-cli/src/cli/machine.rs
  - crates/fsm-cli/tests/crash_harness.rs
  - crates/fsm-cli/tests/cli_golden.rs
  - crates/fsm-cli/tests/naive_caller
  - crates/fsm-cli/tests/mcp_full.rs
status: planned
merged_as: ""
---
# All-Targets Clippy Clean

The lint that is supposed to guard the riskiest change in this repository currently fails to compile one of its own test targets, and everything behind that failure is unreported.

**Steps:**

1. Fix the hard error first: `crates/fsm-core/tests/enumerate_small.rs:796` uses `eprintln!` against the workspace-level `print_stderr = "deny"`. Nothing behind it is visible until this compiles.
2. That line prints a genuine summary an author wants when running the suite by hand, so **do not delete it**. Use whatever mechanism the workspace already provides for a test that must write to a stream; if none applies, `#[allow(clippy::print_stderr)]` with a reason comment naming why this test prints. The reason is the point — a bare `#[allow]` moves the problem instead of resolving it.
3. Re-run `cargo clippy --workspace --all-targets` and work the remaining findings, roughly 95 across the files listed in `touches`, all predating plan 0009. Plans 0009 through 0016's own files were cleared in `46450d0` and should stay clean.
4. Prefer the mechanical fix the lint suggests. The dominant kinds are `useless_conversion` on `&str` literals in flag tables and `type_complexity` on tuple-heavy test helpers.
5. **A fix that changes what a test asserts is not a fix.** Where removing an `.into()` would change a type in a way that changes behaviour, `#[allow]` it and say why in the comment.
6. For `type_complexity`, introduce a named type alias or struct where it makes the helper readable, and `#[allow]` with a reason where the tuple is genuinely clearer. `CONTRIBUTING.md` treats craft guidance as heuristics with a stated purpose, and "following it here made the code harder to read" is a complete answer — written down, not assumed.
7. Change **no behaviour**. This is a cleanup commit and it asserts that no byte written to disk changed.
8. Do not widen the committed gate here. That is the next task, and keeping them separate means a reviewer can see the cleanup without the policy change.

**Tests:**

- `cargo clippy --workspace --all-targets -- -D warnings` exits zero. This is the task's headline and it is currently a compile error.
- **Every golden fixture in the repository is byte-identical to before this task**, across `crates/fsm-core/tests/fixtures/` and `crates/fsm-cli/tests/fixtures/`. This is the proof that nothing changed, and it is stronger than review.
- `cargo test --workspace --no-fail-fast` passes, and so does the release-profile run — a lint fix that only compiles in one profile is not done.
- `enumerate_small` still prints its summary when run directly, asserted by running it and observing the line.
- Every `#[allow]` introduced carries a reason comment; a grep for a bare `#[allow]` in the touched files returns nothing.
- `cargo test -p fsm-cli --test zero_deps` and `cargo test -p fsm-embed-acceptance` pass unchanged.
- `scripts/oversized-files.sh` passes — a type alias extracted into a file near the ceiling must not push it over.

- **Done when:** `cargo clippy --workspace --all-targets -- -D warnings` exits zero, every golden is byte-identical, the debug and release test runs both pass, every introduced `#[allow]` states its reason, `enumerate_small` still prints its summary, and `cargo test`, `cargo fmt --check`, and `scripts/oversized-files.sh` succeed.
