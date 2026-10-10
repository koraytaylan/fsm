---
id: 0021
title: "Executor Contract Preflight"
status: done
---
# Scope — Plan 0021

> A valid machine and a valid handler table do not yet prove that the two can run together.

## Why this plan

`fsm execute --check` parses an operator's handler table and reports its resolved commands and dead letters; it does not compare that table with a machine, and `fsm://executor` exposes enough metadata for a client to perform that comparison by hand without enforcing it before execution.

The gap matters before the first external action: a later emit can omit a handler placeholder, an outcome event can be unknown or internal, and a static outcome payload can violate the event's field contract after a command has already succeeded; the executor then has evidence of an external operation without the workflow advance the author expected.

The existing compiler deliberately permits effect arguments beyond the declared field list and checks a declared field's type only when that field is supplied, so this plan cannot substitute a comparison of declarations for an inspection of actual emit sites.

## In scope

- **0091 — The compatibility contract.** A deterministic, bounded report over compiled machines and an operator-owned handler table; every syntactic emit site, required argument and inferred type; static outcome payloads and stamps; invocation closure and explicit limits around dynamic signals; deliberate manual effects and deliberate outcomes without an advance; and a shared execution admission check before any new child starts.
- **0092 — The authoring surfaces.** An offline and read-only store CLI check, an MCP draft check available before machine creation, precise unavailable-contract reporting, cross-surface acceptance fixtures, and guidance that teaches the author to repair a specific incompatibility before starting work.
- Recheck admission against actual pending arguments, the emitting definition, the current outcome definition, and the loaded table identity, including after restart, migration, and a change of operator configuration.
- Preserve existing pure-core semantics, journal bytes, hashes, request-id derivation, outcome mapping, and the distinction between a compatible contract and a workflow whose progress is provable.

## Out of scope

Proving guard satisfiability, event reachability, successful remote operations, compensation correctness, or guaranteed workflow termination; execution remains at least once, and external services remain beyond the statechart's proof boundary.

Copying stdout or MCP results into events, adding a handler input type language, letting an MCP client submit executable handler configuration, or changing the machine format to satisfy executor policy.

Autonomous scheduling belongs to plan 0020; process containment and shutdown belong to plan 0022; this plan supplies admission to their shared execution path and does not duplicate those drivers or lifecycle mechanisms.

## Completion standard

A client can check an unregistered draft against the active executor, receive the same structural result as the CLI, repair an invalid contract, and execute it through either supported driver; an incompatible pending workflow cannot start even its first external handler, while explicitly manual effects and intentionally absent outcome events remain supported and visible; unavailable evidence is never reported as a pass.

All six task inventories are complete through `09f81669`; the frozen integration
review in STATUS records the executed portable and provisioned native gates,
historical failure dispositions and remaining release environments owned by
plan 0023.
