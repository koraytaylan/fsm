---
id: cases-docs
title: "Cases Docs"
workstream: "0086"
kind: task
depends_on:
  - supersedes-case-delta
gated: false
touches:
  - docs/EXAMPLES.md
  - docs/EMBEDDING.md
  - examples/expense_approval.cases.json
  - examples/order_lifecycle.cases.json
  - examples/parallel_review_deadline.cases.json
  - crates/fsm-cli/tests/cases_doc.rs
status: done
merged_as: ""
---
# Cases Docs

A case file is something a model writes, so the documentation has to be the thing it learns the format from — which means a worked example that runs, not a schema table.

**Steps:**

1. Add `examples/expense_approval.cases.json` beside the committed example machine: a small set of cases in the repository's neutral vocabulary, including a context override and a partial `expect` block that asserts one field. **All three script steps cannot fit in one file.** `expense_approval` declares no effect and no deadline, and no committed example machine declares *both* — `order_lifecycle` emits and has no deadline, `parallel_review_deadline` has deadlines and emits nothing. Inventing a machine that exists only to hold all three would teach the format at the cost of the example being real, so the steps are split across three files, one per machine, each testing the machine it sits beside: `expense_approval.cases.json` for `send`, `order_lifecycle.cases.json` for `ack`, and `parallel_review_deadline.cases.json` for `poll`. `EXAMPLES.md` says why, and a test asserts all three steps appear across the set.
2. Add a worked section to `docs/EXAMPLES.md` showing the case file, the command, the passing output, and one deliberately failing case with its divergence output. The failure is the half that teaches the format — a reader who has only seen success does not know what a divergence looks like.
3. Add the format and the library entry point to `docs/EMBEDDING.md`: the closed key sets, the three script steps, the optional `expect` fields, and the three ceilings with their values.
4. Document the two comparison rules explicitly, because they are asymmetric on purpose and a reader will assume otherwise: effects compare in emission order, configuration and enabled compare as sets, and context compares key by key.
5. Give the `supersedes` delta its own short section. It is the reason to keep case files rather than write them once, and it is the part a reader will not discover on their own.
6. Document regeneration together with its refusal: `FSM_REGEN_FIXTURES=1`, and the rule that it will not run against an uncommitted file, with the reason — a regeneration nobody reviews produces a file that agrees with the code by construction.
7. State plainly that a case run opens no store, claims no `request_id`, and writes nothing, so a reader knows it is free to run in a loop.
8. Create `crates/fsm-cli/tests/cases_doc.rs` asserting the documentation against the code, in the shape `executor_doc.rs` established.

**Tests:**

- `crates/fsm-cli/tests/cases_doc.rs`: the committed example case file parses and **passes** against the committed example machine, so the documentation's example is executable rather than aspirational.
- Every closed key set documented in `EMBEDDING.md` matches the parser's accepted keys, asserted against the constants so a new key cannot ship undocumented.
- The three documented ceilings equal the constants the parser enforces.
- The documented commands are commands the binary accepts, asserted by running each documented invocation.
- The `EXAMPLES.md` failure transcript matches the output the binary actually produces for that case, line for line, generated at test time. It is written as prose plus a `text` fence rather than a `$ fsm` transcript: `examples.rs` executes every documented `$ fsm` line and asserts that a documented non-zero exit carries an exact `hint:`, which is true of an `ErrorObj` and **not** of `machine test`, whose non-zero exit is a *result*. Two commands here also name placeholder files that are not in the repository. `cases_doc.rs` checks these transcripts against real binary output, which is the stronger check anyway.
- `EMBEDDING.md` states the order-versus-set asymmetry, the regeneration refusal and its reason, and the no-store property — one assertion each, against pinned phrases.
- `cargo test -p fsm-cli --test examples` passes with the new example files included.
- `cargo test -p fsm-cli --test policy` passes over the new prose and example.

- **Done when:** `cargo test -p fsm-cli --test cases_doc --test examples --test policy` passes, the documented example case file runs and passes against the committed machine, the failing transcript matches real output byte for byte, every key set and ceiling is asserted against the constants rather than as literals, and `cargo test`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo fmt --check` succeed.
