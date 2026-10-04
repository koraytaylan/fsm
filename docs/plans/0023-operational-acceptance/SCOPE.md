---
id: 0023
title: "Operational Acceptance"
status: planned
---
# Scope — Plan 0023

> Readiness is a set of observed outcomes on an identified build.

## Why this plan

The review at `67ad5e3` rated the project 8/10 for its intended single-node
scope: a strong deterministic engine and recovery suite, with remaining gaps
in autonomous embedded execution, machine–handler compatibility checks, and
shutdown/restart safety; it also called for live-model acceptance and sustained
operational evidence before a higher rating.

Plans 0020–0022 implement those changes, but passing their focused tests does
not establish that an installed binary works through a real client or survives
a long-running workload on every supported operating system. The current
`acceptance/suite/` already provides an independent Python standard-library
client and fifteen installed-binary scenarios; `docs/RELEASE.md` also requires
manual live-model and Desktop checks. This plan extends those assets and makes
their evidence attributable to a candidate, rather than claiming they are
absent or replacing them with an engine-derived oracle.

The score is a review judgment, not a product API or a mathematical guarantee;
completion means all five review concerns have concrete passing evidence and
no unresolved finding contradicts them, so another reviewer can assess the
same intended scope without relying on the original score.

## In scope

- **0095 — Installed-binary evidence.** A versioned report with revision,
  binary digest, environment, scenario inventory, actual checks, skips and
  failures; independent end-to-end scenarios for the three implementation
  plans, including process and MCP handlers over stdio and HTTP.
- **0096 — Operational duration and portability.** A repeatable mixed workload
  with bounded fault injection, responsiveness and resource measurements,
  short CI coverage, and sustained native Linux, macOS and Windows runs.
- **0097 — Real users and release decisions.** A reproducible but uncoached
  live-model protocol, retained host compatibility checks, and a candidate
  evidence gate which refuses missing, stale, skipped or contradictory proof.

## Out of scope

Implementing the production changes owned by plans 0020–0022; raising a score
by adding unrelated engine features; HA, multiple writers or distributed
workers; claims of exactly-once external effects; model benchmarking across
providers; storing credentials or private user workflows in release artifacts;
an unattended agent sending messages to third parties.

## Prerequisites and completion

Register this plan only after plans 0020, 0021 and 0022 have integrated with
their public contracts and platform support decisions settled; these are
cross-plan prerequisites, not dangling task IDs in the local dependency DAG.
The lifecycle feasibility gate in 0022 cannot be bypassed by skipping a native
test or calling an unresolved run safe. Existing release gates remain required.

The seven tasks are planned work, not evidence that any of their proposed
commands or artifacts exists today. Automated proof, sustained proof and the
manual live-model task must all pass for the same candidate executable and
contract set before the plan may be marked complete.
