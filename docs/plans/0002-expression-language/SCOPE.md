# Scope — Plan 0002

> A total, terminating, exactly-typed expression language — small enough to prove, rich enough for guarded workflows.

## Why this plan

Guards, actions, and invariants are the semantic payload of every machine, and they are authored by a model, not a human. That forces two properties at once: evaluation must be total, deterministic, and exactly typed (checked integers, fixed-point decimals with static scale discipline, no floats, a hard step budget), and every rejection must carry a span-precise, mechanically generated hint so the author can self-correct in one attempt. The language is deliberately tiny — no division operator, no `%`, no `let`, no loops, no user functions — because every excluded construct is a class of bugs and a page of audit surface that never needs to exist.

## In scope

- **0006 — Lexing.** Token set with byte-offset spans over a 4 KiB source cap; keywords reserved, mode/unit words contextual.
- **0007 — Parsing.** The versioned EBNF grammar: lazy `if/then/else`, short-circuit `and`/`or`, non-chaining comparisons with a fix-it hint, `+ - *` only, builtin calls, depth/size limits, expected-token-set errors.
- **0008 — Typing.** The full typing table: `Bool`, `Int`, `Dec(scale ≤ 12)`, `Str`, machine-declared enums, `Ts`, `Dur`; mixed-class arithmetic forbidden with fix-its; scope flags for invariant and entry/exit contexts; Levenshtein suggestions for unknown identifiers.
- **0009 — Evaluation.** Strict left-to-right checked evaluation under a shared step budget, with a full sub-expression trace (values, skipped subtrees, error operands) — plus the seven builtins `min max abs dec round div dur`.
- **0010 — Partial Evaluation.** Three-valued (Kleene) evaluation with event fields unknown, the engine behind the enabled-events report.
- **0011 — Docs.** `docs/SPEC.md` is created with its skeleton and the complete normative §Expressions; goldens derive from that prose, never from observed behavior.

## Out of scope

Statechart semantics, machine compilation, and the binding of expressions to declared context/events are plan 0003. Persistence, CLI, and MCP surfaces are plans 0004–0006. Excluded language constructs (`/`, `%`, string operations beyond equality, `let`, loops, recursion, user functions) are design decisions recorded in SPEC.md, not future work.
