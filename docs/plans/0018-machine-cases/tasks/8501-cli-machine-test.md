---
id: cli-machine-test
title: "CLI Machine Test"
workstream: "0085"
kind: task
depends_on:
  - case-expectations
gated: false
touches:
  - crates/fsm-cli/src/cli/machine.rs
  - crates/fsm-cli/src/cli/mod.rs
  - crates/fsm-cli/tests/machine_test_cmd.rs
  - crates/fsm-cli/tests/fixtures/machine_test_output.txt
status: planned
merged_as: ""
---
# CLI Machine Test

The command is the whole interface to this plan, and it has to run where a machine author works — a directory with a definition, a case file, and no store.

**Steps:**

1. Add `fsm machine test <machine.json> --cases <cases.json> [--case <name>] [--json]` to the command tree in `crates/fsm-cli/src/cli/mod.rs`, with the handler in `crates/fsm-cli/src/cli/machine.rs`.
2. **Open no store.** The command reads two files, compiles the definition, runs the cases, and reports. It must work in a directory that has never held a store — that is what lets it run in an editor loop and in CI over a repository of definitions. Assert it, do not merely intend it.
3. Read both files through the existing bounded-read helpers, and report a malformed case file with the format parser's own error rather than a second vocabulary.
4. Report per case: a pass line, or the divergences with their fields and step indices. End with a summary counting passed and failed.
5. Exit zero when every case passes, non-zero when any fails. A test command whose exit code does not track its result is a command CI cannot use.
6. `--case <name>` runs exactly one case, which is what an author does while fixing one; an unknown name is an error listing the case names in the file.
7. `--json` emits the structured shape, built from the core's divergence data so the human and structured outputs agree by construction rather than by parallel formatting code.
8. A definition that fails to compile reports the compiler's findings exactly as `fsm validate` does, before any case runs. An author with a broken definition needs the compiler's error, not ten identical case failures.

**Tests:**

- `crates/fsm-cli/tests/machine_test_cmd.rs`: a passing case file against a committed example machine exits zero and matches `crates/fsm-cli/tests/fixtures/machine_test_output.txt` byte for byte.
- A failing case exits non-zero and its output names the field, the expected value, the found value, and the step index.
- **The command runs in a directory with no store**, asserted by executing it in a fresh temporary directory containing only the two files.
- The command takes no lock, asserted by running it while a writer holds a store elsewhere.
- `--case` with a known name runs one case; with an unknown name it errors and lists the available names.
- A malformed case file reports the format parser's error, naming the offending key.
- A definition that fails to compile reports the compiler findings and runs no case.
- `--json` output parses and carries the same field, expected, found, and step index values as the human output.
- A case file naming a `machine` that differs from the definition under test still runs — the field is for reporting only, and this is what `0086` depends on.
- The `fsm --help` golden in `cli_golden.rs` includes the new command and its flags.

- **Done when:** `cargo test -p fsm-cli --test machine_test_cmd` passes every case above, the command provably runs with no store and takes no lock, the exit code tracks the result, human and `--json` output are built from the same structured data, a broken definition reports compiler findings before any case runs, and `cargo test`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo fmt --check` succeed.
