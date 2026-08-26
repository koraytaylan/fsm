# Architecture — Plan 0010

> The concrete deltas, by symbol.

## Implementer orientation

Read this before your first task. The workflow is identical for every task in this plan:

1. Read your task file top to bottom, then only the parts of this document your workstream covers. Everything is decided here — if you find yourself making a design choice, you have missed a sentence; re-read before improvising.
2. Fixtures first, always: commit the vectors/goldens/spec fragments your task names before writing implementation code.
3. Your task's **Tests:** block is the complete acceptance inventory — implement every listed case; add more if you find a gap, never fewer.
4. Stay inside your task's `touches` list. Needing another file is a signal you misread the design.
5. Run the gates locally before every commit: `cargo test && cargo clippy --workspace -- -D warnings && cargo fmt`.
6. Write the obvious version. Determinism and reviewability beat cleverness everywhere here.
7. When a golden fails, fix the code to match the fixture — never the fixture to match the code — unless the fixture demonstrably contradicts this document or `docs/SPEC.md`.
8. **This plan bumps two formats** (`fsm.state/2` → `fsm.state/3`, store `VERSION` 8 → 9). Read `4904`'s migration rules before touching any persistence code, and never write a format discriminator a reader cannot interpret without guessing.

## 0000 — Orientation: the five facts that shape this plan

- **The core cannot create an instance.** `fsm-core` performs no I/O and holds no allocator for ids that outlive a call; `crates/fsm-core/src/lib.rs` forbids naming `std::fs`, `std::net`, `std::time`. Invocation therefore *must* be an outbox: the pure engine records that a child should exist, and the shell makes it exist. This is not a limitation to route around — it is the same shape `emit`/`effect_ack` already has, and copying it is why this plan needs no new concepts in the store.
- **Instance ids are already derived, not allocated.** SPEC: "deterministic identifiers — ids derive from content and the injected clock." A child id can therefore be a pure function of `(parent_instance_id, slot)`, which removes id allocation from the critical path entirely and makes `invoke_child` idempotent by construction: re-running it computes the same child id and the store answers `duplicate: true`.
- **`$done.*` is a solved problem.** Plan 0009 built the generated-event vocabulary, the `$` reservation, the bounded internal queue, and the rule that an unhandled generated event is discarded. `$done.invoke.<slot>` is a third member of a family, not a new mechanism. **Do not build a parallel path for it.**
- **Every state-dependent outcome is journaled, and retries are exact.** SPEC §Idempotency keys on `(request_id, request_fp)`. Both new operations derive their keys from journaled content, so a crashed enactment replays rather than re-applies, exactly as the executor's acks do.
- **Duplicate replay is reconstructed *per record kind*, and it fails silently.** `store/idempotency.rs::replay_duplicate` first serves a retry from an in-memory `last_responses` cache, and only when that is empty — after a restart, or from a second process — does it rebuild the response from the journal through a chain of **kind-specific** `if`/`matches!` branches. It is not an exhaustive `match`, so a record kind with no branch compiles fine and simply falls through. Every task in this plan that adds a record kind therefore adds its own branch **and** tests the cold path by reopening the store, because a same-process retry would pass with no branch at all and prove nothing about the case the whole design exists to survive.

The consequence: composition adds **three record kinds, one block action, one state declaration, one generated event, and one format version** — and no new control flow anywhere.

## 0048 — Invocation in the core

### Declaring one

A state may carry `invoke`, an array of at most `MAX_INVOKES_PER_STATE = 4` slots (task `4801`):

```json
{
  "name": "await_review",
  "invoke": [{
    "id": "review",
    "machine": "9f2c…64 hex…",
    "with": { "amount": "ctx.total", "opened_at": "ctx.opened_at" },
    "returns": { "decision": "outcome", "reviewed_at": "closed_at" }
  }]
}
```

- `id` is the **slot** name, unique machine-wide across all invoke slots, and `$`-free like every other identifier. It names the invocation for the whole of its life: the child id derives from it, the generated event is `$done.invoke.<id>`, and the audit trail reads by it.
- `machine` **MUST be a 64-hex `machine_id`, never a name.** This is the plan's sharpest ruling and it is worth the ergonomic cost. A content-addressed reference means the parent's own `machine_id` transitively pins the exact child definition, so "what does this workflow do" has one answer forever; a name would let the same parent mean different things on different days, which is precisely the property this engine exists to deny. `def/invoke_machine_ref` rejects anything that is not 64 lowercase hex; `def/invoke_unknown_machine` is a **store-level** admission check, because the core cannot see a catalogue — `Store::define_machine` refuses a definition whose invoked machine it does not already hold.
- `with` projects parent expressions into the child's declared context variables. Each key must be a context variable the child declares, each RHS is an `expr/1` over the parent's `ctx` only (no `evt` — an invocation is triggered by state entry, not by an event), and the types must match exactly including decimal scale (`def/assign_type`). A child context variable not named in `with` keeps its declared `init`.
- `returns` projects the **child's** context out into the payload of the generated done event: each key is a field name on `$done.invoke.<id>`, each value names a context variable the child declares. Types are the child's declared types. The parent's handling transition reads them as `evt.decision`, exactly like any other event payload — the parent's own context is never written behind its back.

### Validation

`spec/validate/reactive.rs` gains the invoke rules (task `4801`), joining plan 0009's tenants in that file:

| Code | Rule |
|---|---|
| `def/invoke_machine_ref` | `machine` is not a 64-lowercase-hex `machine_id` |
| `def/invoke_dup_slot` | two invoke slots share an `id` |
| `def/invoke_on_terminal` | an invoke on a terminal or `final` state — nothing could ever consume its result |
| `def/invoke_unknown_ctx` | a `with` key or a `returns` value names a context variable the child does not declare |
| `def/invoke_type` | a `with` RHS or a `returns` projection type-mismatches the child's declared type |
| `def/invoke_evt` | a `with` expression references `evt` |
| `def/limit_invokes` | more than `MAX_INVOKES_PER_STATE` slots on one state |
| `def/invoke_cycle` | the invocation graph has a cycle |
| `def/invoke_depth` | the invocation graph is deeper than `MAX_INVOKE_DEPTH = 4` |

The last four are only decidable with the child definitions in hand, so `def/invoke_unknown_ctx`, `def/invoke_type`, `def/invoke_cycle`, and `def/invoke_depth` are **store-level admission checks** in `store/lifecycle.rs::define_machine_on` (task `4901`), reported through the same `Finding` vocabulary. The graph is statically knowable precisely *because* `machine` is a hash — this is the payoff of that ruling, and the cycle check is a plain depth-first walk over an immutable, content-addressed DAG.

### The outbox and the state format

`InstanceState` gains **two** fields, and **both land in task `4802`**:

```rust
pub invocations: BTreeMap<String /*slot*/, Invocation>,
pub signals: BTreeMap<String /*signal id*/, PendingSignal>,

pub struct Invocation { pub child_machine_id: String, pub status: InvokeStatus, pub child_instance_id: String }
pub enum InvokeStatus { Pending, Running, Returned }
```

**A format version is defined exactly once.** `signals` is not populated until workstream 0050, but it is added to the struct and to the hash payload here, as an always-empty map, because `fsm.state/3` must mean one thing for its whole life. Landing `invocations` under v3 in `4802` and then adding `signals` to v3 in `5001` would give the same version string two payloads: every v3 record written in between — including anything `4904`'s migration stamped — would carry a hash no later build could reproduce. Task `5001` therefore *populates* a field that already exists and does **not** touch the format.

Entering a state with `invoke` slots inserts each as `Pending` with the child id already derived (below). `invocations_pending` in a view is the slots at `Pending`. Exiting the state removes the slot entirely — and if it was `Running`, the exit also produces a cancel directive (§0049).

`fsm.state/3` therefore hashes `{format, machine_id, instance_id, seq, status, configuration, ctx, history, deadlines, pending, invocations, signals}`, with `invocations` canonically sorted by slot and `signals` by signal id. Both keys are always present, empty maps included — an *omit-when-empty* rule would be a second thing about the format to remember and buys nothing here, unlike plan 0009's `microsteps`, whose absence had to preserve pre-existing record bytes. This is a genuine format bump and `4904` owns its migration.

**The child instance id is derived, never allocated:** `child_instance_id = "inst-" || hex(sha256("fsm:child:1" || parent_instance_id || 0x00 || slot))[..24]`, using the existing `fsm_core::sha256` and the domain-separation convention every other hash in this workspace follows. Two consequences fall out and both are load-bearing: `invoke_child` is idempotent without consulting anything, and a reader can compute a child's id from its parent's id and the definition alone.

### The generated event

`$done.invoke.<slot>` (task `4803`) joins plan 0009's family. `on: "$done.invoke.<slot>"` resolves when the machine declares that slot; anything else is `def/unknown_event` with the generated-name list in the hint. It is not externally sendable (`req/event_internal` already covers every `$`-prefixed name). Unlike its two siblings it **carries a payload** — the `returns` projection — typed by the child's declarations, and a handling transition's block reads it through the ordinary `evt` binding.

The event is enqueued by the **store**, not the core: it arrives with `invocation_return` (§0049), which hands it to plan 0009's macrostep machinery as the trigger event exactly as `poll_deadline` hands over a due deadline. The core's only job is to know the name resolves and to type its payload.

## 0049 — Store enactment

### `invoke_child`

`crates/fsm-store/src/store/instance/invoke.rs` (task `4901`), signature mirroring the existing mutators:

```rust
pub fn invoke_child(&mut self, parent_id: &str, slot: &str, request_id: &str) -> Result<Value, ErrorObj>
pub fn invoke_child_on(&mut self, clock: &mut dyn Clock, parent_id: &str, slot: &str, request_id: &str) -> Result<Value, ErrorObj>
```

One record, `instance_invoked`, body `{parent_instance_id, slot, child_instance_id, child_machine_id, overrides, request_id, state_hash, child_state_hash, state_format}`:

- `overrides` is the evaluated `with` projection, canonical and typed — evaluated by the **core** when the slot went `Pending` and carried in `InstanceState`, not re-evaluated here, so the values are the ones the entry pipeline computed and not whatever the context holds now.
- The child instance is created by the same `create` path any instance uses, from the child machine's declared inits with `overrides` applied, at the record's `ts`. **A failed child creation fails the whole operation** and journals nothing, mirroring SPEC's rule that `run/create_failed` is unjournaled — the slot stays `Pending` and the caller may correct and retry.
- The parent's slot moves `Pending → Running` and both state hashes are committed, so a fold can check the pair.
- Legal only when the slot is `Pending`; against a `Running` or `Returned` slot it is `req/field_unknown` in the manner `ack_effect` already rejects a settled effect, journaling a `request_rejected` that claims the key so a retry replays the same benign refusal.

**Fold derives the child from this one record.** There is no separate `instance_created` for a child: the child's existence, machine, initial configuration, and context are a pure function of the record body and its `ts`, so replay reconstructs it. One record, one fsync, one atomic outcome — which is why this plan needs no group-commit concept.

### `invocation_return`

`crates/fsm-store/src/store/instance/invoke.rs`, same file (task `4902`):

```rust
pub fn invocation_return_on(&mut self, clock: &mut dyn Clock, parent_id: &str, slot: &str, request_id: &str) -> Result<Value, ErrorObj>
```

Legal only when the slot is `Running` **and** the child's status is `Completed` or `Cancelled`. One record, `invocation_returned`, body `{parent_instance_id, slot, child_instance_id, outcome ("completed" | "cancelled"), payload, request_id, state_hash, state_format}`:

- `payload` is the `returns` projection read out of the child's **final** context. For a `cancelled` child the projection is skipped and the payload is `{}`; a transition that must distinguish reads a declared field, so the parent's definition decides what cancellation means rather than the engine deciding for it. `outcome` is carried in the record for the audit trail and is **not** in the event payload — adding an engine-chosen field to a typed payload would break the child's declared shape.
- The record delivers `$done.invoke.<slot>` into the parent as the trigger of a plain macrostep (plan 0009), so the parent may cascade and the whole reaction seals in this one record, `microsteps` array included.
- The slot moves `Running → Returned`. It is removed when the parent exits the invoking state, not here — a parent may sit in the state and read the result through its transition.
- If the parent has **no** transition on `$done.invoke.<slot>`, the event is discarded per plan 0009's rule and the record still commits with the slot `Returned`. That is a modelling smell, and `machine_analyze` reports it (task `5103`), but it is not an error.

### Cancel cascade and orphans (task `4903`)

- **Exiting the invoking state while a child is `Running` cancels the child.** The core emits the exit as it always did; the resulting state has the slot gone, and the store — in the same operation that applied the parent's transition — journals the child's `instance_cancelled` with `reason: "parent-exit:<parent_id>/<slot>"`. This is the one place this plan writes two records for one request, and it is safe for a specific reason: the second record is a *cancellation*, which is idempotent and state-independent, so a crash between the two leaves the child running-but-unreferenced, which task `4903`'s reconciliation sweep detects and finishes. Say this in SPEC rather than pretending the window does not exist.
- **Cancelling a parent cancels every `Running` child, depth-first**, by the same mechanism and with the same reason string, bounded by `MAX_INVOKE_DEPTH`.
- **A child may be cancelled directly.** Its parent then sees `outcome: "cancelled"` on return. Nothing about a child forbids the operations any instance has.
- **Orphan reconciliation** is a store-open-time sweep, not a background thread: a `Running` child whose parent slot is gone, or whose parent is `Completed`/`Cancelled`, is reported by `fsm doctor` and cancelled by an explicit `fsm repair --cancel-orphans`. Never automatically at open — an open must not write.

### Format migration (task `4904`)

`fsm.state/3` and store `VERSION` 9. The existing machinery already does exactly this and the rules are unchanged: records carry `state_format` so a reader interprets old bytes without guessing; interior records are never rewritten; opening a `VERSION` 1–8 directory folds the complete journal using each record's discriminator and stamps 9 on success; a failed fold refuses and leaves `VERSION` alone. An instance whose journal predates this plan folds to an `invocations` map that is **empty**, and an empty map is hashed as `fsm.state/2` did — no, it is not: the format string differs, so the hash differs by construction. The rule is therefore: **records written before this plan keep `state_format: "fsm.state/2"` and verify under the v2 hash; new records carry `"fsm.state/3"`.** Per-record discriminators are what make this a non-event, and this plan adds no new mechanism to support it.

## 0050 — Cross-instance signals

`signal` is a block action beside `do`, `emit`, and `raise` (task `5001`):

```json
"signal": [{ "to": "ctx.counterparty_instance", "event": "batch_ready", "with": { "batch": "ctx.batch_id" } }]
```

- `to` is an `expr/1` expression of type `str` evaluated at emit time, yielding **exactly one** instance id. There is no broadcast, no tag fan-out, no query. The reason is replayability: the set of instances matching a query grows over time, so replaying the record would deliver to a different set and the store would stop being a function of its journal. The engine already refuses broadcast across parallel regions; this is the same rule one level up.
- `event` names an event **the target's machine declares**, and `with` types against the target's declaration. The target machine is not statically known — `to` is a run-time value — so this is checked at **delivery** (`req/event_unknown` / `req/field_type` against the target), not at admission. State that plainly: a signal is the one construct in this engine whose payload typing is a run-time check, and the alternative (declaring the target machine statically) was rejected because a signal's whole purpose is to reach an instance the sender learned about at run time.
- Signals land in `signals_pending` on the sender, part of `fsm.state/3` alongside `invocations`, with the same `Pending` semantics as an effect.
- `MAX_SIGNALS_PER_BLOCK = 4`, `def/limit_signals`.

`signal_deliver(sender_id, signal_id, request_id)` (task `5002`) journals one `signal_delivered` record carrying **both** instance ids, the event, the payload, the outcome, and both state hashes, and applies the event to the target as an ordinary macrostep. A delivery whose target does not exist, is settled, or refuses the event is journaled as delivered-with-outcome rather than lost: the sender's audit trail must show that it tried. The sender's own state is unchanged by delivery except that the signal leaves `signals_pending` — **a signal is fire-and-forget by design**, and a sender that needs an answer models it as the target signalling back.

## 0051 — Surface

**Executor (task `5101`).** `fsm-execute` gains two directives and one rule, so composition runs unattended with no handler-table entry: `InvokeChild` for every `Pending` invocation, and `InvocationReturn` for every `Running` slot whose child has settled. Derived keys, in the crate's existing style: `exec-inv-{parent}/{slot}` and `exec-ret-{parent}/{slot}`. `signal_deliver` gets `exec-sig-{sender}/{signal_id}`. The watcher reads all three from `InstanceState`'s public fields; the scheduler decides from journal-derived facts as it already does; nothing here needs a subprocess, so these directives bypass the runner entirely and go straight to the pipeline. New codes `exec/invoke` and `exec/signal` join the crate's `ALL_CODES`.

**CLI and MCP (task `5102`).** `fsm instance invoke <parent> <slot>`, `fsm instance return <parent> <slot>`, `fsm instance signal <sender> <signal-id>`, and the three matching MCP tools `invocation_start`, `invocation_return`, `signal_deliver` — mutating, so they join `MUTATING_TOOLS` and the read-only refusal set. They exist so a bare MCP session can compose machines without an executor running; the executor is the default path, not the only one.

**Legibility (task `5103`).** `instance_get` gains `parent: {instance_id, slot} | null` and `children: [{slot, child_instance_id, status}]`, and `instance_list` gains a `parent` filter and a `roots_only` switch — a store where every list is flat is a store nobody can navigate. `machine_diagram` gains an `invoke` overlay drawing a slot as a labelled box on the invoking state, and `machine_analyze` reports slots with no handling transition on their `$done.invoke.*` event.

## 0052 — Proof and docs

**Chaos (task `5201`).** `crates/fsm-cli/tests/composition_chaos.rs`, following `executor_chaos.rs`'s seeded-restart precedent, kills at each named point: before `invoke_child`, between `invoke_child` and the child's first event, before `invocation_return`, between a parent-exit and its cascade cancel, and mid-signal-delivery. The invariants: exactly one `instance_invoked` per slot; exactly one `invocation_returned` per slot; no child without a derivable parent record; no `Running` slot whose child does not exist; the journal verifies clean; and after the parent-exit window, the orphan sweep finds and reports the child. Depth-2 and depth-3 trees, plus a machine that invokes two slots from one state.

**Docs (task `5202`).** SPEC gains a `## Composition` section: the invoke declaration, the derived child id with its domain string, both operations and their legality rules, the cascade and its one two-record window, the signal single-target ruling, `fsm.state/3`, and the new record kinds. `docs/EMBEDDING.md` gains the operator's view of the executor's three new directives. `README.md` gains one guarantee row — *composition is explicit: a child exists because a record says so, and its id is derivable from its parent's* — and one non-claim: composition is single-store and single-writer like everything else.
