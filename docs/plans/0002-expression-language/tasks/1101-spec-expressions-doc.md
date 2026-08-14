---
id: spec-expressions-doc
title: "Spec Expressions Doc"
workstream: "0011"
kind: chore
depends_on:
  - builtins
  - three-valued-partial-eval
gated: false
touches:
  - docs/SPEC.md
status: planned
merged_as: ""
---
# Spec Expressions Doc

`docs/SPEC.md` is the normative document the whole project builds against — golden fixtures derive from its prose, never from observed implementation behavior — and this task creates it with its skeleton and the complete §Expressions.

**Steps:**

1. Create `docs/SPEC.md` with the document skeleton: title, normative-language note, the format-version registry (`fsm.machine/1`, `fsm.journal/1`, `fsm.state/1`, grammar `expr/1`), and placeholder sections for machine definitions, semantics, journal, and the error-code appendix, each marked with the plan that lands it.
2. Write the complete normative `## Expressions` section: the EBNF exactly as implemented, keyword and contextual-word rules, the full typing tables, builtin signatures with the literal-scale/mode rules, evaluation order (strict left-to-right, short-circuit, lazy `if`, shared step budget), three-valued partial evaluation with the conservative-error rule, and the `expr/*` plus expression-raised `run/*` error catalogue (code, trigger, hint policy).
3. State the standing golden rule in the document: a golden that disagrees with SPEC.md is a bug in the implementation or the golden, never a silent SPEC edit.

**Tests:**

- This is a documentation chore — no unit tests exist for prose; acceptance is the following checklist, each item falsifiable:
- The EBNF block in §Expressions is character-identical to the grammar block in this plan's ARCHITECTURE workstream 0007 (verified by diffing the two fenced blocks).
- Error-catalogue completeness by mechanical grep, recorded in the commit message: every distinct code string matching `expr/` or `run/` under `crates/fsm-core/src/expr/` appears in the catalogue, and the catalogue lists no code absent from the source (the automated error-code coverage test that permanently enforces this lands in plan 0006).
- Two committed `parse.jsonl` fixtures — one precedence success and the `expr/chained_cmp` error — are hand-traced against the doc's EBNF and hint wording, and the trace is summarized in the commit message.
- The typing tables cover every operator/class row implemented in `typeck.rs` (row-by-row review against the match arms; any row present in code but absent from the doc is a blocker).
- The builtin table lists all seven builtins with the literal-scale/mode rule and each one's error codes; the partial-evaluation subsection states the conservative-error rule including budget exhaustion.
- The golden-rule sentence appears verbatim; each placeholder section names its landing plan; the format-version registry lists exactly the four versions.

- **Done when:** `docs/SPEC.md` exists with the skeleton and a §Expressions covering grammar, typing, builtins, evaluation, partial evaluation, and the error catalogue, with the checklist above satisfied and its grep/trace evidence in the commit message, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
