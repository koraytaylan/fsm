---
id: case-regeneration
title: "Case Regeneration"
workstream: "0085"
kind: task
depends_on:
  - cli-machine-test
gated: false
touches:
  - crates/fsm-cli/src/cli/machine_test_regen.rs
  - crates/fsm-cli/tests/machine_test_regen.rs
status: planned
merged_as: ""
---
# Case Regeneration

Regeneration is how a case file agrees with the code by construction, which makes it either the most useful command here or the one that quietly destroys the plan's value.

**Steps:**

1. Create `crates/fsm-cli/src/cli/machine_test_regen.rs` implementing regeneration for `fsm machine test`, driven by `FSM_REGEN_FIXTURES=1` — this repository's established idiom, which cases join rather than replacing with a new flag.
2. Regeneration rewrites each case's `expect` block from observed behaviour, preserving the file's key order, its formatting, and every field the author wrote that the runner does not produce.
3. **Refuse to regenerate unless the case file is tracked by version control and has no uncommitted modifications.** This is the safeguard the whole plan rests on: a regeneration that cannot be reviewed as a diff produces a file that agrees with the code by construction and proves nothing. Say that in the refusal message, not just in the code.
4. Regenerate only the fields the case already names. An `expect` block asserting only `configuration` keeps asserting only configuration — regeneration must not silently widen a case into asserting everything, because the author's choice of what to pin is information.
5. Print what changed, per case and per field, so the terminal output and the version-control diff say the same thing.
6. Refuse to regenerate a case that **errored** rather than diverged — a case whose script names a non-pending effect, or whose definition does not compile, has no observed behaviour to write down, and writing the error into the file would encode the bug.
7. Exit non-zero if nothing was regenerated because nothing diverged, so a regeneration run in CI cannot pass silently.
8. Leave the ordinary run path untouched: without the environment variable the command never writes.

**Tests:**

- `crates/fsm-cli/tests/machine_test_regen.rs`: regenerating a diverging case rewrites its `expect` block and the file then passes.
- Regeneration **refuses** against a case file with uncommitted modifications, and the message states why review matters.
- Regeneration refuses against an untracked case file.
- A case asserting only `configuration` still asserts only `configuration` after regeneration — the field set is not widened.
- Key order, indentation, and unrelated fields survive regeneration byte for byte, asserted against the original file outside the rewritten blocks.
- A case that errors rather than diverges is not regenerated, and the error is reported.
- A run with nothing to regenerate exits non-zero.
- Without `FSM_REGEN_FIXTURES`, a diverging case leaves the file unmodified — assert the bytes, not just the exit code.
- Regeneration is idempotent: running it twice leaves the file unchanged the second time.
- The regenerated file parses under the format parser, so regeneration cannot emit something the reader refuses.

- **Done when:** `cargo test -p fsm-cli --test machine_test_regen` passes every case above, regeneration provably refuses on a dirty or untracked file with a message that says why, the asserted field set is never widened, formatting and unrelated fields survive byte for byte, the ordinary path still never writes, and `cargo test`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo fmt --check` succeed.
