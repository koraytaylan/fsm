# Architecture — Plan 0011

> The concrete deltas, by symbol.

## Implementer orientation

Read this before your first task. The workflow is identical for every task in this plan:

1. Read your task file top to bottom, then only the parts of this document your workstream covers. Everything is decided here — if you find yourself making a design choice, you have missed a sentence.
2. Fixtures first: commit the definition pairs and goldens your task names before writing implementation code.
3. Your task's **Tests:** block is the complete acceptance inventory — implement every listed case; add more if you find a gap, never fewer.
4. Stay inside your task's `touches` list.
5. Run the gates locally before every commit: `cargo test && cargo clippy --workspace -- -D warnings && cargo fmt`.
6. Write the obvious version. Determinism and reviewability beat cleverness.
7. When a golden fails, fix the code to match the fixture — never the fixture to match the code — unless the fixture demonstrably contradicts this document or `docs/SPEC.md`.
8. **Every rule in this plan is a refusal or a ruling.** Migration is where a workflow engine is most tempted to guess. If a case is not covered by a rule below, the answer is to refuse it with a named code, never to do something reasonable-looking.

## 0000 — Orientation: the four facts that shape this plan

- **Records are never rewritten, and fold re-applies through the pure engine.** An instance whose records span two definitions must therefore be foldable by a reader who processes those records in order, switching definitions at the migration record. That is the whole persistence design, and §0055 implements exactly it.
- **`machine_id` is the definition.** `StoredMachine` holds the compiled machine keyed by hash, and `Store::resolve_machine` looks it up. Both the old and the new machine are in the store — the old one cannot be removed, because pre-migration records replay against it. Migration adds a *second* definition to an instance's life, never a replacement in the catalogue.
- **`InstanceState` holds five collections besides its status, configuration, and context.** `history` binds state names, `deadlines` binds deadline names, `pending` holds effect ids whose names re-derive by replaying against the *emitting* definition, and — after plan 0010 — `invocations` binds slot ids while `signals` holds undelivered signals addressed to *other* instances. Each needs a carry-over ruling, and §0054 gives each one. Count them from `crates/fsm-core/src/machine.rs` before starting, not from this sentence: a collection with no ruling is a collection whose migration behaviour is undefined.
- **Idempotency keys on `(request_id, request_fp)`.** A migration is state-dependent and must be journaled, so a retry after a lost response replays rather than re-migrating. The fingerprint covers `(instance_id, to_machine_id)`.

## 0053 — Declaration and admission

### `supersedes`

A machine definition may carry one optional top-level `supersedes` block (task `5301`):

```json
"supersedes": {
  "machine": "3a7f…64 hex…",
  "states": { "awaiting_review": "awaiting_review", "in_review": "under_review" },
  "context": { "reviewer": "ctx.assignee", "opened_at": "ctx.opened_at" }
}
```

- **It is part of the canonical definition and therefore part of `machine_id`.** This is the plan's central decision. Two authors who write the same corrected machine with different mappings produce two different machines, which is correct — the mapping *is* part of what the new definition means. A reader holding the new hash holds the mapping, and a migration can never be reinterpreted after the fact.
- `machine` is a 64-lowercase-hex `machine_id`, validated the way plan 0010 validates an invoke reference (`def/supersedes_machine_ref`).
- `states` maps **old state names to new state names**. An old state absent from the map is not an error at admission — it is a state from which migration is refused at run time, which is the honest reading of "I did not say what this becomes".
- `context` maps **new context variables to expressions over the old instance's context**, written as `ctx.<old_var>` with the old machine's declared types in scope. A new variable absent from the map takes its declared `init`. An old variable nobody references is dropped.
- Only one `supersedes` per definition: a chain of three definitions migrates in two hops, each journaled, and that is deliberate — a transitive closure computed by the engine would be a mapping nobody wrote.

### Admission

`def/supersedes_machine_ref` and `def/supersedes_self` (a machine naming its own hash, which is unsatisfiable since the hash includes the block) are decidable from the definition alone and live in `crates/fsm-core/src/spec/validate/reactive.rs` (task `5301`).

The rest need both definitions and therefore run in `Store::define_machine_on` (task `5302`), through a new `crates/fsm-core/src/migrate/validate.rs` the store calls with both compiled machines:

| Code | Rule |
|---|---|
| `def/supersedes_unknown_machine` | the store does not hold the superseded `machine_id` |
| `def/supersedes_unknown_state` | a `states` key is not a state of the old machine, or a value is not a state of the new one |
| `def/supersedes_target_not_leaf` | a `states` value names a compound or a history pseudostate; migration lands on a **leaf**, because that is what an active configuration holds |
| `def/supersedes_target_terminal` | a `states` value names a terminal or `final` state, which would complete an instance by migrating it |
| `def/supersedes_region` | the old and new machines disagree on shape — one sequential and one parallel, or a differing region-name set. Region topology is not mappable and this plan does not pretend otherwise |
| `def/supersedes_ctx_unknown` | a `context` key is not a context variable of the new machine, or an expression names a variable the old machine does not declare |
| `def/supersedes_ctx_type` | a `context` expression's type differs from the new variable's declared type, scale included |
| `def/supersedes_slot` | a plan-0010 invoke slot present in the old machine is absent from the new one and no mapping covers it |

Refusing at *definition* time rather than at migration time is deliberate: an operator learns their mapping is wrong when they write it, not when they try to move a live instance with it.

## 0054 — The pure migration

New module `crates/fsm-core/src/migrate/` (tasks `5401`–`5403`), pure like everything else in the crate:

```rust
pub fn migrate(from: &CompiledMachine, to: &CompiledMachine, t_to: &Tree,
               st: &InstanceState, now_ms: i64, budget: &mut Budget) -> Result<Migrated, Rejection>

pub struct Migrated { pub state: InstanceState, pub report: MigrationReport }
```

Order of operations, which is normative (task `5401`):

1. **Gate.** The instance must be `Running`. A `Completed` or `Cancelled` instance is refused with `req/migrate_settled` — there is nothing to save, and migrating a finished workflow would rewrite what it did.
2. **Map the configuration.** Every active leaf — the one leaf for a sequential instance, every region's leaf for a parallel one — is looked up in `states`. A leaf with no entry refuses the whole migration with `req/migrate_unmapped`, naming the leaf. Partial migration is never performed.
3. **Project the context.** Evaluate each `context` expression against the **old** instance's context under the supplied budget. Unmapped new variables take their declared `init`. An evaluation error refuses with `run/action_error` naming `migration`, reusing the existing block-naming vocabulary rather than inventing one.
4. **Carry over the five collections**, per §0054's rulings below.
5. **Evaluate the new machine's invariants** on the migrated context and configuration. An enforce failure refuses the migration atomically with `run/invariant`; monitor failures are reported in the `MigrationReport` and do not block. Migrating an instance into a state its own definition calls invalid is exactly the thing this step exists to prevent.
6. **Run the reaction phase to quiescence**, then return. A migrated instance has landed on a leaf of a machine whose semantics it has never been subject to, and if that leaf has an eventless exit or a `final` child, leaving the instance parked there would put it in a state its own definition says it should already have left. Plan 0009 made `create` and `poll_deadline` macrosteps for exactly this reason, and migration is the third case: the mapped configuration is a *trigger*, and the reaction runs before anything is sealed. `instance_migrated` therefore carries a `microsteps` array under the same absent-when-empty rule every other record uses, and `5502`'s replay verifies it in both directions. A rejection anywhere in the reaction rejects the whole migration atomically, exactly as a rejection in step five does.
7. **Return.** The status stays `Running` unless the reaction reached a terminal leaf, in which case it is `Completed` — a migration that lands on a state the new machine immediately finishes is a legitimate outcome, and hiding it would make the instance's status disagree with its configuration. `seq` is the store's business, not the core's.

### Carry-over rulings (task `5402`, `crates/fsm-core/src/migrate/carryover.rs`)

Each of the four is a decision, and each is written here so nobody has to invent it twice:

- **History** is remapped through the same `states` map, key and value alike: a binding `owner → child` becomes `states[owner] → states[child]` when both are mapped. A binding whose owner or child is unmapped is **dropped**, not refused — a history binding is an optimisation for a state the instance is not in, and losing it degrades behaviour rather than corrupting it. The report lists every dropped binding, so the loss is visible rather than silent.
- **Deadlines are recomputed, never carried.** The old schedules are named by the old machine's deadline names and were computed from the old machine's `after` expressions. Carrying the due times would keep a promise the new definition never made. Instead every schedule is dropped and the new machine's deadlines for the mapped configuration are scheduled from the migration's `now_ms`, by exactly the rule state entry uses. State the consequence plainly in SPEC: **migration restarts the clock on every timer**, and an operator who cannot accept that should let the instance finish on its old definition.
- **Pending effects are retained verbatim.** An effect id is `{instance}/{seq}/{k}` and its name re-derives by replaying the *emitting* record against the machine that emitted it — which is still in the catalogue. Nothing about migration invalidates work already handed to the outside world, and dropping a pending effect would strand a handler that is mid-run. An ack after migration therefore still resolves.
- **Invocation slots** (plan 0010) are carried when the slot id exists in the new machine with the **same** `child_machine_id`, and refuse the migration with `req/migrate_slot` otherwise. Unlike history, a running child is not droppable: something exists and is executing. A `Returned` slot whose id is gone is dropped with a report entry, since its result has already been delivered.
- **Pending signals** (plan 0010) are **retained verbatim**, for the same reason pending effects are. A signal names a target instance id and an event the *target's* machine declares; the migrating instance's own definition has no bearing on whether it can be delivered, so neither `states` nor `context` can invalidate one. Dropping a signal would silently lose a message the sender's journal says it produced, and refusing the migration over one would block an upgrade for a reason unrelated to the definition. This is the fifth ruling and it exists because a collection with no ruling has undefined behaviour — not because the answer was hard.

### Preview (task `5403`)

`pub fn preview(from, to, t_to, st) -> MigrationPreview` runs every step including the reaction phase, without producing a state, and returns what an operator needs before committing: the mapped configuration, the projected context with before/after values, every dropped history binding, every deadline that will be rescheduled with its old and new due times, every retained effect, and the refusal — with its code — if there would be one. It is pure and takes no `request_id`, so it is safe on a read-only store and is the honest answer to "what will this do".

A cohort preview — `preview_all(from, to, store_states)` — groups instances by their refusal code, so an operator sees "412 migrate cleanly, 8 are in `awaiting_countersign` which your map does not cover" instead of discovering the eight one at a time.

## 0055 — Store, replay, and surface

### The operation (task `5501`)

`crates/fsm-store/src/store/instance/migrate.rs`:

```rust
pub fn migrate_instance_on(&mut self, clock: &mut dyn Clock, instance_id: &str,
                           to_machine: &str, request_id: &str) -> Result<Value, ErrorObj>
```

One record, `instance_migrated`, body `{instance_id, from_machine_id, to_machine_id, configuration_before, configuration_after, dropped_history, rescheduled_deadlines, request_id, state_hash, state_format}`.

- The target machine's `supersedes.machine` **must** equal the instance's current `machine_id`, or the operation is `req/migrate_not_superseded`. There is no path that migrates an instance onto a machine that did not declare it was superseding this one.
- The report fields in the body are not decoration: they are what makes the migration auditable after the fact, and replay recomputes and checks them like any other journaled claim.
- Idempotency fingerprint covers `(instance_id, to_machine_id)`. A retry replays; the same instance migrated to a *different* machine under the same key is refused, not replayed.
- A refusal from the pure `migrate` is journaled as a `request_rejected` claiming the key, exactly as a rejected send is, so the audit trail shows the attempt.

### Replay (task `5502`)

`crates/fsm-core/src/replay/apply.rs` and `crates/fsm-store/src/store/reconstruct.rs` track the **current machine per instance** rather than resolving it once from `instance_created`. On `instance_migrated`, the instance's machine changes and every subsequent record for it replays against the new definition. This is the only structural change replay needs, and it is small precisely because the record carries both hashes.

Fold verifies the migration as a claim: re-run the pure `migrate` with the record's `ts`, and check the resulting `state_hash`, `configuration_after`, `dropped_history`, and `rescheduled_deadlines` against the journaled values. A mismatch is the existing `StateHashMismatch` posture — refuse, no repair.

### Surface (tasks `5503`, `5504`)

- `fsm instance migrate <id> --to <machine> [--dry-run]` and the MCP tool `instance_migrate`, which joins `MUTATING_TOOLS`. `dry_run: true` runs `preview` and writes nothing, mirroring `machine_create`'s dry run, and works on a read-only server.
- `fsm migrate --from <machine> --to <machine> [--dry-run] [--limit N]` (task `5504`) is the cohort command. It previews the whole cohort first, prints the grouped summary, and then migrates instance by instance, **each with its own derived `request_id`** of the form `migrate-{instance_id}-{to_machine_id}` so an interrupted run resumes by replaying what it already did. There is no bulk atomicity and the command says so: a crash halfway leaves half the cohort migrated, and re-running finishes it.
- `instance_get` gains `machine_history: [{machine_id, from_seq}]`, so a reader can see that an instance has changed definitions without paging its journal.

## 0056 — Proof and docs

**Properties and chaos (task `5601`).** A property suite over generated definition pairs — take an enumerated small machine, apply a random structural edit (rename a state, add a state, change a guard, add a deadline), synthesise the identity mapping, and assert: migration preserves `Running` status; the migrated configuration is coherent under the new machine's tree; a `step` immediately after migration behaves identically to the same `step` on an instance created fresh in the mapped state with the projected context; and a full journal fold reproduces every `state_hash`. Plus a chaos leg over interrupted cohort migrations, asserting exactly one `instance_migrated` per instance and a resumable half-done cohort.

**Docs (task `5602`).** SPEC gains a `## Evolution` section covering the `supersedes` block and its inclusion in `machine_id`, the admission table, the seven-step migration order, all five carry-over rulings — with the deadline restart called out as a MUST-know consequence — the record kind, and the replay rule that an instance's records span definitions. `docs/EMBEDDING.md` gains the operator's runbook: preview the cohort, read the grouped refusals, fix the map or accept the exclusions, then migrate. `README.md` gains one guarantee row — *definitions evolve explicitly: an instance changes machine because a record says so, under a mapping the new definition declares* — and the honest non-claim that migration restarts timers and that cohorts are not atomic.
