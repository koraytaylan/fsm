---
id: golden-sessions
title: "Golden Sessions"
workstream: "0027"
kind: task
depends_on:
  - offline-commands
  - diagram-exporters
  - ops-commands
gated: false
touches:
  - crates/fsm-cli/tests/cli_golden.rs
  - "crates/fsm-cli/tests/fixtures/sessions/**"
  - "crates/fsm-cli/tests/fixtures/structured/**"
status: planned
merged_as: ""
---
# Golden Sessions

The whole command tree is proven end-to-end against the real binary with byte-exact golden stdout, and every command's `--json` output is frozen into the shared contract fixtures that plan 0006's MCP structuredContent must match byte-for-byte.

**Steps:**

1. Author the fixtures first: `crates/fsm-cli/tests/fixtures/sessions/case_review.txt` — the interleaved golden transcript (command lines, expected stdout, expected exit codes) for the full session: validate → machine add → instance new → send docs_ok → a rejected send with its hint visible on stderr → the corrected send → ack of the emitted effect → annotate → history → journal verify; and `crates/fsm-cli/tests/fixtures/structured/*.json` capturing each command's exact `--json` bytes for the same session.
2. Implement `crates/fsm-cli/tests/cli_golden.rs`: run each step against the real binary via `env!("CARGO_BIN_EXE_fsm")` under `FSM_CLOCK_MS` in a fresh temp data dir, byte-comparing stdout to the session transcript and asserting exit codes and the rejection step's stderr error object.
3. Re-run the session's commands with `--json` and byte-compare each output against its `fixtures/structured/*.json` file — the frozen contract plan 0006 asserts parity against.

- **Done when:** `cargo test -p fsm-cli --test cli_golden` byte-matches the full session transcript, all exit codes, and every structured fixture, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
