---
id: 0018
title: "Machine Cases"
status: planned
---
# Scope — Plan 0018

> The engine's own behaviour is pinned by four test layers. The machines it runs are pinned by nothing.

## Why this plan

Everything in this repository exists so a model can author a definition and have the engine guarantee the semantics. The guarantee is real and it is narrow: it covers what the *engine* does with a definition, never whether the definition says what its author meant. Plan 0011 then made definitions editable — an instance can move onto a corrected machine under a declared mapping — and that raised the stakes without raising the assurance. A model that revises a machine today has no way to state what the old one did, and therefore no way to discover that the new one no longer does it.

The tools that exist all *describe* a machine and none of them *expects* anything of it:

- `fsm validate` proves the definition is well-formed. A well-formed machine that approves the wrong requests is well-formed.
- `fsm simulate` runs a sequence and prints what happened. It is a probe, not an expectation — nothing is committed, so nothing can fail later.
- `machine analyze` and the completeness matrix report reachability and coverage over the definition's shape, which is a different question from behaviour.
- `machine diagram` renders it.

So the gap is precise: **there is no way to commit what a machine should do and have a change falsify it.** The whole engine is built on the principle that a claim you cannot check is a claim you do not have, and the machines are the one layer where that principle is not applied.

This plan closes it with the smallest thing that works: a case file beside a machine, a runner that executes it deterministically, and a command that compares. No new service, no new state, nothing persisted.

## What makes this more than a convenience

Two properties fall out of the engine's own design and are why this belongs here rather than in a user's shell script:

- **It is exactly reproducible, by construction.** The core is pure and takes time as a parameter. A case run reads no clock, touches no store, and allocates no identifier from anything but its inputs, so a case that passes on one platform passes on every platform and a failure is always the machine's fault. A user's script driving the CLI against a temporary data directory has none of that.
- **It pairs with migration.** A definition that declares `supersedes` is claiming to be a corrected version of a specific earlier machine. That claim is checkable: run the earlier machine's cases against the new definition, map the configurations through the mapping the new definition already declares, and report which outcomes moved. A migration is then a **reviewed diff** rather than a hope, in exactly the register plan 0011's `--dry-run` established for instances.

The second property is the reason the plan is worth its cost. The first is the reason it can be trusted.

## In scope

- **0084 — The format and the runner.** The `fsm.cases/1` file format with closed key sets and its own limits; a pure scripted runner in `fsm-core` that generalizes `simulate`'s loop to the three things a workflow actually does — send an event, poll a deadline, acknowledge an effect; and the expectation matcher whose failure output names the field that moved rather than printing two states side by side.
- **0085 — The surface.** `fsm machine test`, and regeneration through the repository's existing fixture-regeneration idiom so a deliberate behaviour change is a reviewable diff instead of a hand-edited file.
- **0086 — Migration pairing and docs.** Running a superseded machine's cases against the definition that supersedes it, reported as a per-case delta; and the authoring documentation.

## Out of scope

An MCP tool. `tools/list` measures 36 256 bytes against a ceiling of 38 000 that the budget test argues must never rise, leaving room for roughly one tool — and spending the last of it is a decision to make deliberately, once the format has met real machines, rather than by arrival order. Cases are a CLI and library capability in this plan.

Anything persisted. A case run writes nothing to a store, claims no `request_id`, and produces no record. It is a pure function of a definition and a case file, and that is what makes it free to run.

Property-based or generative case authoring, coverage-directed case synthesis, and mutation scoring of a case set. Each is a plausible later plan and each needs this one first.

Cases over a *running* instance, or assertions about a store's contents. That is what `journal replay` and `explain` already do, from the other end.

Making a superseding definition's case delta a **gate**. A corrected machine usually changes behaviour on purpose; a rule that forbids it would be wrong, and one that could be overridden would be ignored. The delta is a report an author reads, not a test that fails.
