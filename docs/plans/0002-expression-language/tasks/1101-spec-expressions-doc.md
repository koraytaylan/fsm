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
4. Cross-check the section against the committed expression fixtures and reconcile any divergence in the fixtures or the implementation, not the prose, unless the prose is demonstrably wrong (in which case the change is called out in the commit message).

- **Done when:** `docs/SPEC.md` exists with the skeleton and a §Expressions covering grammar, typing, builtins, evaluation, partial evaluation, and the error catalogue, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
