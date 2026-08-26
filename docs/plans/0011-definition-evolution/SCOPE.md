---
id: 0011
title: "Definition Evolution"
status: planned
---
# Scope — Plan 0011

> Content addressing makes a definition immutable. Nothing yet makes a *workflow* survive that.

## Why this plan

`machine_id` is a hash of the canonical definition, and that is right: it is what makes "what does this workflow do" have exactly one answer forever, and what lets plan 0010 pin a child machine by reference. It has a consequence nobody has yet paid for.

**Editing a machine mints a different machine.** Every instance already running stays bound to the old `machine_id` for the rest of its life. There is no operation — none in the CLI, none in the MCP surface, none in the store — that moves a running instance onto a corrected definition. The consequences are not hypothetical:

- A guard with an inverted comparison strands every in-flight instance in the state it was wrong about. The fix is a new machine that no existing instance can reach.
- A workflow that runs for weeks cannot absorb a correction made in week one without being cancelled and re-created, which discards its context, its history bindings, and its journal continuity — the three things this engine exists to protect.
- Plan 0009 adds eventless transitions and plan 0010 adds invocation. Both are exactly the kind of improvement an author will want to apply to a definition that already has live instances, and both make the absence of migration more expensive.

The design constraint is that migration must never become a hole in the audit posture. Everything this engine promises — replay determinism, tamper evidence, exact idempotency, one answer per question — has to survive an instance changing definitions mid-life. That rules out the obvious shortcuts. Migration is **not** a store rewrite; interior records are never touched. It is **not** implicit; no instance ever changes definition because a newer machine appeared. It is **not** a guess; a mapping that does not cover an instance's current state refuses rather than improvising.

So migration is one more journaled, idempotent, replayable operation, and the mapping it applies is **declared in the new definition itself**. That last point is the plan's key decision: `supersedes` is part of the new machine's canonical bytes, so the new machine's own identity includes how it interprets the old one's states. Two authors who write the same fix with different mappings produce different machines, which is correct, and a reader who has the new `machine_id` has the mapping too.

## In scope

- **0053 — Declaration and admission.** The `supersedes` block on a machine definition — the superseded `machine_id`, a state mapping, and a context projection — and the admission checks that need both definitions in hand: mapping totality, target-state existence, context typing, and the refusal of a mapping that would land an instance somewhere incoherent.
- **0054 — The pure migration.** `migrate(from, to, state, now_ms) -> Result<InstanceState, Rejection>` in `fsm-core`: state-mapping, context projection, the reaction phase a migrated instance runs like a freshly created one, and the carry-over rulings for the five collections an instance holds besides its status, configuration, and context — history bindings, deadline schedules, pending effects, pending signals, and invocation slots. Plus the preview that answers "what would this do" without doing it.
- **0055 — Store, replay, and surface.** The `instance_migrate` operation and its `instance_migrated` record; the fold and replay changes that let one instance's records span two definitions; the CLI and MCP tools including a dry run; and the bulk command that migrates a cohort one journaled instance at a time.
- **0056 — Proof and docs.** A property suite over generated definition pairs, a chaos leg over interrupted bulk migrations, and the SPEC section that makes every ruling normative.

## Out of scope

Automatic or implicit migration of any kind — an instance moves because an operator asked, with a `request_id`, and never because the store noticed something newer. Bulk atomicity: a cohort migration is N independent journaled operations, and a crash halfway leaves half the cohort migrated, which is reported rather than hidden. Downgrade: `supersedes` points backwards in time only, and migrating an instance onto an *older* definition is refused, because the mapping that would make it safe does not exist. Editing history: no record is ever rewritten, and an instance's pre-migration records keep replaying against the definition that produced them. Cross-store migration, and migration of a child instance independently of its parent's slot declaration — a parent and child migrate as separate decisions, and plan 0010's slot hash pins what a parent expects.
