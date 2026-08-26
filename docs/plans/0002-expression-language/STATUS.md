# Plan 0002 — Expression Language — ✅ Complete

The roll-up row in [../STATUS.md](../STATUS.md) must stay in sync with this file. Task-level truth lives in [tasks/](tasks/) frontmatter; Makina's integration coordinator updates both layers.

- **Status:** ✅ Complete.

- **Goal:** land the guard/action/invariant expression language — lexer, parser, typechecker, evaluator, builtins, and three-valued partial evaluation — as a total, step-budgeted, exactly-typed pipeline with span-precise self-correcting errors.
- **Root cause:** every machine's semantics live in model-authored expressions, so evaluation must be provably terminating and exactly typed, and every rejection must teach its own fix — none of which a general-purpose scripting language provides.
- **Approach:** a deliberately tiny versioned grammar (no `/`, `%`, `let`, loops, or user functions) lands stage by stage, each stage pinned by fixtures authored before the implementation, ending with the normative SPEC.md §Expressions from which all goldens derive.
- **Progress:** 7/7 tasks done; 0 blocked; 0 dropped.
- **Integration:** `complete`; run — (the single Makina run, `01M018Z4PKM8RQSARHTNJ824TX`, marked the plan failed and every later task skipped after task 0101 tripped the reviewer cap on a footprint false positive — four test-output files sat outside its `touches`. The plan was therefore landed directly on `develop`, one commit per task in dependency order, and sealed here after the fact.); base `develop` @ `2d2f8ce57b53bf773ab80b2200e25ab40a8f4afd`; validation base —; mode `direct-to-develop`; final integration `d36b954` (the `v0.1.0` tag commit).
- **Exceptions:** — (coordinator-owned blocked/dropped reasons are recorded here).
- **Outcome:** Guards, actions, and invariants parse, typecheck, and evaluate as a total, step-budgeted, exactly-typed expression language with three-valued partial evaluation and span-precise errors.

_Last updated: 2026-08-14, against `develop` @ `2d2f8ce`._
