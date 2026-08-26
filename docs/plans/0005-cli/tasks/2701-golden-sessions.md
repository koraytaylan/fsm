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
status: done
merged_as: ""
---
# Golden Sessions

The whole command tree is proven end-to-end against the real binary with byte-exact golden stdout, and every command's `--json` output is frozen into the shared contract fixtures that plan 0006's MCP structuredContent must match byte-for-byte.

**Steps:**

1. Author `crates/fsm-cli/tests/fixtures/sessions/case_review.txt` (the interleaved golden transcript: `$ fsm …` command lines, expected stdout, expected exit codes) and `crates/fsm-cli/tests/fixtures/structured/*.json` first, encoding exactly the session under **Tests** — expected output derived by hand from the render and schema contracts, not recorded from a run.
2. Implement `crates/fsm-cli/tests/cli_golden.rs`: run each step against the real binary via `env!("CARGO_BIN_EXE_fsm")` under `FSM_CLOCK_MS` in a fresh temp data dir (`std::env::temp_dir()` + pid + counter; no third-party temp crate), byte-comparing stdout, asserting exit codes, and asserting the rejection step's stderr error object.
3. Re-run the session's commands with `--json` and byte-compare each output against its `fixtures/structured/*.json` file — the frozen contract plan 0006 asserts parity against.

**Tests:**

- The session transcript, exchange by exchange with pinned exit codes, byte-compared stdout per step: `validate` the reference spec → 0; `machine add` → 0 (id + `created: true` printed); `instance new` → 0 (instance id, initial leaf, request id); `send docs_ok` → 0 (transition summary, enabled events); a rejected send (an event with no candidate from the current leaf) → **1**, stdout empty, stderr carrying the full error object with code and hint (the one step whose stderr is byte-asserted); the corrected send → 0; `instance ack` of the effect emitted on entering the composite → 0 (pending emptied); `annotate` → 0; `instance history` → 0 (every prior record listed in seq order); `journal verify` → 0.
- Structured parity fixtures: one `fixtures/structured/<step>.json` per session command, each byte-equal to that command's `--json` rerun — the frozen plan-0006 contract; the test fails if a structured fixture exists with no matching session step or vice versa (no orphan contracts).
- Hygiene assertions across all steps: success steps produce empty stderr; the failure step produces empty stdout; every output ends with exactly one trailing newline.
- Determinism: executing the entire session twice in fresh temp dirs under the same `FSM_CLOCK_MS` produces byte-identical stdout streams and byte-identical `--json` reruns — the transcripts cannot flake.
- Isolation: the temp data dir is created fresh per run and left behind only on failure (printed for diagnosis), so reruns never inherit state.

- **Done when:** `cargo test -p fsm-cli --test cli_golden` byte-matches the full session transcript, all exit codes, and every structured fixture, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
