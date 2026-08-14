# Scope — Plan 0003

> A pure `step()`: hierarchical selection, LCA exit/entry pipelines, history — every decision explainable, every claim oracle-checked.

## Why this plan

This plan lands the engine itself: tree-shaped machine definitions that validate structurally and compile against the expression pipeline, and a pure transition function implementing child-first selection, LCA-based exit/entry action pipelines, shallow/deep history, invariants, and an effects outbox — with an explain trace for every applied and rejected event. Because these semantics ossify the moment the first journal is written (plan 0004), they are pinned three ways before that happens: ordering goldens derived from SPEC prose, an exhaustive small-tree enumeration run differentially against a deliberately naive second interpreter, and seeded history properties.

## In scope

- **0012 — Spec Model.** The `fsm.machine/1` JSON format parsed into a typed model (recursive state tree, flat transition array, entry/exit blocks, history pseudo-children), full structural validation with `def/*` codes and size limits, and content-addressed machine identity over canonical bytes.
- **0013 — Static Checks.** Expression binding with exact assignment typing into a compiled machine, then static analysis: exact enterable-set reachability, a leaf-by-event completeness matrix, shadowing and duplicate-guard errors, ancestor-shadowing warnings, and a const-folded always-fails-at-creation check.
- **0014 — Engine.** The tree machinery (parent/depth tables, proper LCA, exit/entry sets, history descent), transition selection along the ancestor chain, the atomic action pipeline with history capture and invariants, and per-step traces plus the canonical state hash.
- **0015 — Assembly.** Pure simulation over event sequences, the chain-aware enabled-events report, and the oracle-differential proof suite (naive interpreter, exhaustive enumeration, ordering goldens, history properties).
- **0016 — Docs.** The normative §Semantics lands in `docs/SPEC.md`.

## Out of scope

Journaling, persistence, and instance identity assignment are plan 0004 (the engine stays pure; effect ids and sequence numbers are composed by the shell). CLI and MCP surfaces are plans 0005–0006. Parallel regions and declarative deadlines are reserved spec keys rejected as "not yet supported", by design.
