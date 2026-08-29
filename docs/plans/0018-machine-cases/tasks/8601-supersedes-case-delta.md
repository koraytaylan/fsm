---
id: supersedes-case-delta
title: "Supersedes Case Delta"
workstream: "0086"
kind: task
depends_on:
  - case-regeneration
gated: false
touches:
  - crates/fsm-core/src/cases/delta.rs
  - crates/fsm-cli/src/cli/machine.rs
  - crates/fsm-cli/tests/supersedes_delta.rs
  - crates/fsm-cli/tests/fixtures/supersedes_delta.txt
status: planned
merged_as: ""
---
# Supersedes Case Delta

A definition that declares it supersedes another is making a checkable claim, and the cases of the machine it replaces are what check it.

**Steps:**

1. Create `crates/fsm-core/src/cases/delta.rs` running one case against two definitions and classifying the result, and add `--against <old.json>` to `fsm machine test` in `crates/fsm-cli/src/cli/machine.rs`.
2. Refuse unless the new definition declares `supersedes` naming the old definition's `machine_id`. Without the mapping these are two unrelated machines and the comparison means nothing; the refusal says exactly that.
3. Translate expected and observed configurations through the `supersedes` mapping using **the same mapping code plan 0011's migration uses**, not a second copy. A report that disagrees with what an actual migration would do is worse than no report, and two implementations will eventually disagree.
4. Classify each case as one of three outcomes: **unchanged**, **changed** with the fields that moved, or **refused** where the new definition rejects a script the old one accepted.
5. Report a state the mapping does not cover as its own outcome, naming the state — this is the same gap `migrate --dry-run` reports for instances, and an author meeting it here can widen the mapping before any instance moves.
6. **Exit zero when the run completes**, whatever the deltas. This is a report, not a gate: a corrected machine usually changes behaviour on purpose, a rule forbidding that would be wrong, and a gate with an override is a gate everyone overrides. Say so in the command's help text, so nobody wires it into CI expecting a failure.
7. Report a non-zero exit only for an actual failure to run — a definition that does not compile, a missing mapping, an unreadable file.
8. Keep the comparison pure and in the core; the CLI reads files and renders.

**Tests:**

- `crates/fsm-cli/tests/supersedes_delta.rs`: a superseding definition that preserves behaviour reports every case unchanged and exits zero.
- A superseding definition that changes one outcome reports that case as changed, naming the fields that moved, and still exits zero.
- A superseding definition that rejects a previously accepted script reports that case as refused.
- A case whose expected configuration names a state the mapping does not cover is reported as uncovered, naming the state.
- The mapping is applied through plan 0011's code — assert that a mapping which `migrate --dry-run` reports as covering a state produces a covered result here, on the same pair of definitions, so the two cannot drift.
- A new definition with no `supersedes` is refused, and the message says the mapping is what makes the comparison meaningful.
- A `supersedes` naming a different `machine_id` than the old definition's is refused.
- The rendered report matches `crates/fsm-cli/tests/fixtures/supersedes_delta.txt` byte for byte.
- `--json` output carries the three outcomes as distinct enumerated values.
- The help text states that the delta is a report and never a gate.

- **Done when:** `cargo test -p fsm-cli --test supersedes_delta` passes every case above, the mapping is shared with plan 0011's migration and pinned to it by a test over the same definition pair, all three outcomes are distinguishable in structured output, a completed run exits zero regardless of deltas and the help text says so, and `cargo test`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo fmt --check` succeed.
