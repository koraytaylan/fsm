---
id: case-expectations
title: "Case Expectations"
workstream: "0084"
kind: task
depends_on:
  - case-script-runner
gated: false
touches:
  - crates/fsm-core/src/cases/expect.rs
  - crates/fsm-core/tests/case_expectations.rs
status: done
merged_as: ""
---
# Case Expectations

A failure that prints two states and leaves the reader to spot the difference has done the easy half, so the matcher compares field by field and names what moved.

**Steps:**

1. Create `crates/fsm-core/src/cases/expect.rs` comparing an `expect` block against a run observation and returning a list of divergences, each naming its field, its expected value, and its found value.
2. Compare only the fields the case names. An absent field asserts nothing — that is what keeps a case readable when the author cares about one thing, and it is stated in the format's own contract.
3. Apply the engine's own ordering rules, and **say which is which at the site**, because a reader will assume all four behave the same:
   - `effects` compare in **emission order**, because that order is deterministic and load-bearing in the engine.
   - `configuration` compares as a **set**: a configuration is a set of active leaves, and parallel regions make any list order an artefact.
   - `enabled` compares as a **set**: it derives from a scan whose order the spec does not fix.
   - `context` compares key by key, reporting each key that differs rather than the whole map.
4. Report divergences for the whole case, never only the first, matching the runner's decision to run the whole script.
5. Name the step index in every divergence, so a ten-step script's failure says where. An `expect` block describes the *final* state, so a final-state divergence carries the last step's index; a step that could not run at all carries its own, and is reported **before** any expectation. A case that failed because its ack named nothing pending did not fail because its configuration differs, and leading with the configuration sends the author to the wrong half of the file.
6. Compare context values through the engine's own value equality and rendering, so a `Dec` with a different scale reports as the difference it is rather than as equal or as a string mismatch. This is the comparison most likely to be written wrongly, and exact arithmetic is the reason it matters.
7. Carry the *rule* each field was compared under in the divergence itself. "Why did `effects` fail when `configuration` with the same-looking difference did not" is the first question the asymmetry produces, and a report that can answer it without the reader consulting the source is the whole reason the asymmetry is safe to have.
8. Produce a structured result, not a formatted string. Rendering belongs to the CLI; the core returns data, which is what lets `--json` and the human output agree by construction.

**Tests:**

- `crates/fsm-core/tests/case_expectations.rs`: a fully matching expectation produces no divergences.
- A single wrong configuration leaf produces exactly one divergence naming `configuration`, with both values.
- A configuration listed in a different order than observed produces **no** divergence, proving the set rule.
- Effects in a different order **do** produce a divergence, proving the order rule — the two together are what pin the deliberate asymmetry.
- One differing context key produces one divergence naming that key, not the whole map.
- A `Dec` expected as `10.0` against an observed `10.00` reports a divergence rather than comparing equal, and the message shows both scales.
- An absent `expect` field produces no divergence whatever the observation holds.
- A case with three divergences reports all three, each carrying its step index.
- `terminal` expected true against a non-terminal run reports a divergence naming the field.
- An expectation over an `enabled` set is order-insensitive.
- The result is structured data with no pre-rendered message, asserted by constructing the divergence list directly.

- **Done when:** `cargo test -p fsm-core --test case_expectations` passes every case above, the set-versus-order asymmetry is pinned by the paired configuration and effects tests, decimal scale differences are reported rather than swallowed, every divergence carries its field and step index, the result is data rather than prose, and `cargo test`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo fmt --check` succeed.
