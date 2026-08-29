# fsm — normative specification

This document is the source of truth for `fsm`. Implementers MUST treat the
keywords MUST / NEVER as binding. Golden fixtures derive from this prose, never
from observed implementation behavior — a golden that disagrees with SPEC.md is
a bug in the implementation or in the golden, never a reason to edit SPEC.md
silently.

## Format versions

| Version | Role |
|---|---|
| `fsm.machine/1` | Machine definition documents |
| `fsm.journal/1` | Journal record envelopes |
| `fsm.state/3` | Instance state identity hash payloads |
| `fsm.state/2` | Instance state identity before composition; still verified where a record declares it |
| `fsm.state-root/3` | Complete logical store roots |
| `fsm.snapshot/5` | Disposable snapshot caches |
| `fsm.base/1` | Authoritative base state a sealed store opens from |
| `fsm.base-dedup/1` | Request-fingerprint root a seal commits over the keys its base carries |
| `fsm.archive/1` | Manifest of a detached archive's sealed segments |
| `expr/1` | Expression grammar |

## Machine definitions

Format `fsm.machine/1`. Top-level keys: `format`, `name`, optional
`description`, optional `enums`, `context`, `events`, optional `effects`,
exactly one configuration form, optional `deadlines`, optional `on_unhandled`
(`reject` default | `ignore`), `transitions` (flat array, document order),
optional `invariants`:

- A single-region definition has `states` and `initial` and omits `regions`.
- A parallel definition has `regions` and omits `states` and `initial`.
  `regions` is a document-ordered array of two through eight
  `{name, states, initial}` objects. State and history names remain unique
  machine-wide, including across regions.

Numerics are strings everywhere (`req/number_token` on a raw JSON number).
Machine identity hashes the entire canonical definition *including*
`description`.

Context variables: `{name, ty, init}`. Types: `int`, `str`, `bool`,
`timestamp`, `duration`, `{decimal: "N"}`, `{enum: "Name"}`. Events and
effects declare `fields`. An event may declare `internal: true`: it is an
ordinary typed event that only the machine may raise — the external send
path refuses it with `req/event_internal`, as it refuses every `$`-prefixed
generated name — and it may still be the `on` of any transition. States are recursive trees; a child with
`history: "deep"|"shallow"` is a history pseudostate. It MUST be owned by a
compound parent and MUST be childless, nonterminal, not final, and have no
`initial` of its own. A leaf with `final: true` ends its parent compound's
inner workflow when entered — the macrostep raises `$done.state.<parent>` —
without ending the machine or region; `terminal` does that. A `final` state
MUST be a leaf under a compound, MUST NOT also be `terminal`, MUST NOT be its
compound's `initial`, and MUST NOT be the `from` of any transition or
deadline; it is otherwise an ordinary leaf with blocks, a target, and a
history binding. A transition's `on` may name `$done.state.<compound>` for a
compound that owns a `final` child (`def/unknown_event` otherwise, with a
hint listing the names this machine generates); such events are never
declared, carry no fields — `evt` binds to an empty object — and are never
sendable (`req/event_internal`). Likewise `on: "$done.region.<region>"` for
a declared region: the macrostep raises it when that region's active leaf
becomes terminal, and only a transition in another region can handle it,
because a completed region is inert. A generated event is raised only when some transition names it in `on`; one nobody handles is never raised, so a definition that never names it sees nothing of it — not in the trace, not in the microstep count. There is no `$done.machine`. Blocks use `do` (sets), `emit`, and `raise`: a `raise` is
`{event, with}`, where `event` names a declared event (never a generated
`$done` name) and `with` maps every one of its declared fields — no more, no
fewer — to an `expr/1` source typed exactly like a context assignment, scale
included (`def/assign_type`). Its payload is evaluated in the block's snapshot
like `do` and `emit`, and the event is delivered to this instance inside the
same macrostep; only a block that commits raises. Transitions use `from`, optional
`on` (absent = an eventless transition, keyed internally under the reserved
`$always` sentinel and run by the macrostep whenever its guard holds; an
explicit `null` is `def/shape`), optional `if` (guard), optional `to` (absent =
internal), optional `do`/`emit`. An eventless transition's guard and block see
no `evt`. Admission charges an omitted `if` on an eventless transition one
implicit-`true` tick under the `$always` key exactly as it does for an event.

Deadlines are document-ordered timed transitions
`{name, from, after, to, optional do, optional emit}`. `name` is unique among
deadlines. `after` is an `expr/1` expression over `ctx` only and MUST have type
`duration`; `to` is required. A deadline source and target MUST be in the same
region. Deadlines are scheduled from caller-supplied time when their source is
entered; the core NEVER reads a clock.

### Structural rules (`def/*`)

| Code | Rule |
|---|---|
| `def/unknown_key` | unknown key at a given JSON-Pointer path |
| `def/shape` | wrong JSON type, missing required field, or malformed history pseudostate shape |
| `def/dup_name` | state and history names share one global namespace |
| `def/one_initial` | every compound declares exactly one `initial` |
| `def/initial_not_child` | `initial` names a direct real child |
| `def/initial_is_history` | `initial` must not name a history pseudostate |
| `def/unknown_state` | `from`/`to`/`initial` resolve |
| `def/unknown_event` | `on` resolves |
| `def/unknown_effect` | emit names resolve |
| `def/unknown_enum` | enum references resolve |
| `def/terminal_not_leaf` | terminal states are leaves |
| `def/terminal_has_transitions` | no transition has a terminal `from` |
| `def/initial_terminal` | every creation entry chain leaf is nonterminal |
| `def/multiple_history` | at most one history per compound |
| `def/from_history` | history is never a transition source |
| `def/history_target_from_inside` | history is only targeted from outside its owner |
| `def/reserved_ident` | `$`-prefixed identifiers rejected |
| `def/cross_region` | an event or deadline transition targets another region |
| `def/deadline_type` | a deadline `after` expression is not a duration |
| `def/duplicate_deadline` | deadline names are unique |
| `def/assign_type` | set target type equals RHS exactly, scale included |
| `def/dup_set` | duplicate set targets in one block |
| `def/shadowed` | guardless/`true` transition precedes later same `(from,on)` |
| `def/duplicate_guard` | structurally identical guards in one group, the eventless group included |
| `def/eventless_evt` | an eventless transition's guard or block references `evt` |
| `def/eventless_from_terminal` | an eventless transition's `from` is terminal |
| `def/eventless_shadowed` | guardless/`true` eventless transition precedes a later eventless transition from the same state |
| `def/eventless_internal_noop` | warning: an eventless transition with no `to`, `do`, `emit`, or `raise` |
| `def/eventless_cycle` | a cycle in the eventless transition graph that the machine provably cannot leave: every state on it has a guardless (or literal-`true`) eventless transition and every eventless transition its scan could select stays on the cycle; an internal eventless transition is a self-edge |
| `def/eventless_cycle_guarded` | warning: any other cycle in the eventless transition graph — a guard the engine cannot decide at admission must break it, and `MAX_MICROSTEPS` stops it at run time |
| `def/eventless_depth` | warning: the longest acyclic eventless cascade times the region count reaches half of `MAX_MICROSTEPS` |
| `def/final_not_leaf` | a `final` state has children |
| `def/final_at_root` | a `final` state has no parent compound — `terminal` is the spelling there |
| `def/final_and_terminal` | `final` and `terminal` on one state |
| `def/final_has_transitions` | a transition or deadline has a `final` state as its `from` |
| `def/final_is_initial` | a compound's `initial` names its `final` child |
| `def/invoke_machine_ref` | an `invoke` slot's `machine` is not a 64-lowercase-hex `machine_id` digest |
| `def/invoke_dup_slot` | two `invoke` slots share an `id`, across the whole machine |
| `def/invoke_on_terminal` | an `invoke` on a `terminal` or `final` state, whose result nothing could consume |
| `def/invoke_evt` | an `invoke` `with` expression reads `evt`; an invocation starts on state entry and sees `ctx` only |
| `def/limit_invokes` | more than 4 `invoke` slots on one state (`MAX_INVOKES_PER_STATE`) |
| `def/limit_signals` | more than 4 `signal` entries in one block (`MAX_SIGNALS_PER_BLOCK`) |
| `def/invoke_result_unhandled` | warning: nothing handles a slot's `$done.invoke.<slot>` |
| `def/invoke_only_exit` | warning: a state whose only exit is an invoked child returning |
| `def/invoke_unknown_machine` | an `invoke` names a machine this store does not hold (checked where the catalogue is, not in the pure core) |
| `def/invoke_unknown_ctx` | a `with` key or `returns` value names a context variable the child does not declare |
| `def/invoke_type` | a `with` expression's type does not match the child's declaration exactly, scale included |
| `def/invoke_cycle` | the invocation graph closes a cycle |
| `def/invoke_depth` | the invocation graph is more than 4 machines deep (`MAX_INVOKE_DEPTH`) |
| `def/supersedes_machine_ref` | `supersedes.machine` is not a 64-lowercase-hex digest |
| `def/supersedes_self` | a definition supersedes itself, which its own hash makes unsatisfiable |
| `def/supersedes_unknown_machine` | `supersedes` names a machine this store does not hold (checked at admission) |
| `def/supersedes_unknown_state` | a `states` mapping names a state one of the two definitions does not have |
| `def/supersedes_target_not_leaf` | a `states` mapping targets a state that is not a leaf |
| `def/supersedes_target_terminal` | a `states` mapping targets a terminal state |
| `def/supersedes_region` | a `states` mapping crosses parallel regions incoherently |
| `def/supersedes_ctx_unknown` | a `context` mapping names a variable the new definition does not declare |
| `def/supersedes_ctx_type` | a `context` expression's type does not match the new declaration |
| `def/supersedes_slot` | a `states` mapping moves an instance onto a state whose invoke slots it cannot carry |
| `def/unreachable_state` | warning: state never enterable |
| `def/ancestor_shadowed` | warning: ancestor handler globally dead |
| `def/create_always_fails` | creation fails on declared inits |
| `def/limit_states` | ≤ 256 state nodes |
| `def/limit_depth` | nesting depth ≤ 12 |
| `def/limit_history` | ≤ 32 history pseudostates |
| `def/limit_regions` | ≤ 8 parallel regions |
| `def/limit_deadlines` | ≤ 128 deadlines |
| `def/limit_events` | ≤ 128 events |
| `def/limit_enums` | ≤ 32 enums |
| `def/limit_variants` | ≤ 64 variants each |
| `def/limit_transitions` | ≤ 2048 transitions |
| `def/limit_cell` | ≤ 32 transitions per (state, event) |
| `def/limit_ctx` | ≤ 64 context variables |
| `def/limit_fields` | ≤ 32 fields per event/effect |
| `def/limit_sets` | ≤ 32 sets per block |
| `def/limit_emits` | ≤ 8 emits per block |
| `def/limit_raises` | ≤ 8 raises per block |
| `def/limit_invariants` | ≤ 64 invariants |
| `def/limit_eval` | ≤ 4096 worst-case evaluation ticks: compiled AST nodes + 1 per distinct event with an omitted `if` |
| `def/limit_bytes` | definition ≤ 256 KiB |

These numeric limits match `crates/fsm-core/src/limits.rs`. The aggregate
expression limit is deliberately whole-definition and conservative. One
create, step, deadline poll, or enabled-event scan can visit each compiled
expression slot at most once, and lazy operators can only visit fewer nodes. A
step can additionally evaluate at most one omitted guard's implicit `true`:
that transition immediately wins the one global selection, so later candidates
are not evaluated. An enabled-event scan performs that selection separately for
every declared event and can therefore evaluate one omitted guard for each
affected event. The scan reports sendable events only — an event declared
`internal` and a generated `$done.*` name are absent from it, not listed as
disabled — and it never predicts the reactions a send would cause; `simulate`
answers that. Admission charges the sum of every compiled AST's node count,
plus one tick per distinct event that has an omitted guard. A definition
accepted by the current compiler MUST NOT exhaust a fresh standard 4096-tick
budget.

## Semantics

`step(machine, tree, state, event, payload, now_ms, budget)` is a pure
function. `now_ms` is caller-supplied data used only to schedule deadlines on
states entered by this step.

1. **State and status gate.** The tagged configuration, lifecycle/terminal
   relationship, history bindings, and active deadline-name set MUST form a
   coherent state for this machine; otherwise reject with
   `run/configuration_invalid`. For a coherent state, `Completed` →
   `run/instance_completed` and `Cancelled` → `run/instance_cancelled`.
2. **Validate event.** Declared name, exact field set, typed string values, no raw JSON numbers.
3. **Candidate scan.** For a single-region machine, walk `chain(leaf)`
   innermost-first. For a parallel machine, concatenate each active region's
   leaf-to-root chain in region document order, skipping a region whose active
   leaf is terminal. At each state, take document-ordered transitions for this
   event. Empty candidates across the complete scan is `run/unhandled`
   (`ignore` yields `Ignored`).
4. **Guard evaluation.** Guards see the pre-transition `(ctx, evt)` only. A guard evaluation error is `run/guard_error` (never treat-as-false). The first true guard wins; later candidates are `not_considered`. All false → `run/not_enabled`.
5. **Target / dom.** Absent `to` is internal (no exit/entry). A history target resolves through `history_descent` (owner used for dom). External self-transition uses `dom = parent(from)` and exits/re-enters. Otherwise `dom = properLCA(source, target)`. A parallel transition changes only its source region; every other active leaf is retained byte-for-byte.
6. **Block pipeline.** Exit blocks inner→outer, then the transition or selected deadline block (only an event transition block sees `evt`), then entry blocks outer→inner. Each block is snapshot-internal: all RHS evaluate against the ctx left by the previous block, then apply atomically. Staging idiom: `transition` sets `ctx.x = evt.y`; an entry block consumes `ctx.x`. Emits collect under one global `k`. Any evaluation error is `run/action_error` naming `exit(state)` / `transition` / `deadline(name)` / `entry(state)`; computed-but-discarded values of completed blocks stay in the trace.
7. **History capture.** For each exited compound that owns a history pseudostate, bind from the **pre-transition** configuration (deep = pre leaf; shallow = owner's direct child on the pre chain). Unbound history later descends the owner's initial chain. Restore re-runs entry blocks. Bindings are retained after completion/cancel. History may only be targeted from outside its owner.
8. **Invariants.** All evaluated on the final ctx and the final active configuration (every entered-but-not-exited state name, `in(state)`). Enforce failure or eval error → `run/invariant`. Monitor failures collect into `monitor_flags` and never block.
9. **Deadlines.** Remove schedules whose source is exited. For every newly
   entered state, in entry order and then deadline document order, evaluate
   its deadlines' `after` expressions against the final context and set
   `due_ms = now_ms + after`. Negative durations or checked-add overflow reject
   atomically as `run/action_error` with cause `run/overflow`. A state that
   remains active retains its existing schedule. Re-entering it replaces the
   schedule. When a parallel region reaches a terminal leaf, remove every
   schedule sourced from that region's active chain; completed regions are
   inert to both events and deadline polls.
10. **Status.** A single-region instance is `Completed` when its new leaf is
    terminal. A parallel instance is `Completed` only when every region's leaf
    is terminal. Completion clears all deadline schedules. Explicit
    cancellation does the same. Rejection discards ctx, every region
    configuration, history, deadline schedules, and effects.

**Creation.** `create(machine, tree, overrides, now_ms)` validates overrides
like event fields and starts from declared inits. A single-region machine
enters its root initial descent outer→inner. A parallel machine enters every
region's initial descent in region document order. Entry blocks share and
thread one context, and effects share one global `k`. Creation then evaluates
invariants and schedules deadlines for all entered states from `now_ms` by the
same rule as a step. History starts empty. Failure is `run/create_failed`. The
shell NEVER journals a failed create and consumes no id or seq. Pure
simulation MUST return that creation rejection as an error and MUST NOT invent
an active configuration or partial report.

**Deadline poll.** `poll_deadline(machine, tree, state, now_ms, budget)` is
pure. After the same complete state-integrity check as `step`, completed and
cancelled instances reject with the corresponding status code before selecting
a schedule. Otherwise it applies at most one due
deadline: minimum `(due_ms, deadline document index)` among active sources in
nonterminal regions with `due_ms <= now_ms`. No due deadline yields `NotDue`
and changes nothing.
A due deadline runs the ordinary external
transition pipeline with no `evt` binding, schedules newly entered sources
from the poll's `now_ms`, and produces `Applied` or `Rejected`. Polling is
explicit: no read, event send, background thread, or passage of wall-clock time
advances an instance. Callers repeat polls to drain multiple due deadlines.

The durable shell journals `NotDue` as `deadline_not_due` and claims its
`request_id`. A retry of the same poll MUST replay that original observation,
even if time has since advanced; a caller uses a new `request_id` for a new
observation. `expect_seq` and the injected clock remain excluded from the
request fingerprint.

**Atomicity.** `create`, `step`, and `poll_deadline` are pure. A caller mutates
logical instance state only for `Applied`; the durable shell separately
journals every state-dependent request outcome, including `NotDue` and
rejections, to make retries exact.

**Active configuration.** The public and persisted configuration is tagged:
`Sequential { leaf }` or `Parallel { leaves }`, where `leaves` maps every
region name to exactly one real leaf. A parallel map with a missing, extra, or
non-leaf entry is invalid state, never repaired by guessing. The same holds for
a status that disagrees with terminality, a malformed history binding, or a
missing/unexpected deadline schedule. `step` and `poll_deadline` validate that
complete state before the lifecycle gate and reject incoherence as
`run/configuration_invalid`. `fsm.state/3`
hashes canonical `{format, machine_id, instance_id, seq, status,
configuration, ctx, history, deadlines, pending, invocations, signals}` under
`fsm:state:3`; `deadlines` maps deadline name to its signed-millisecond due
time, `pending` is sorted before hashing, and `invocations` and `signals` are
ordered by key and always present, empty maps included. `fsm.state/2` is the
same payload without those last two keys, under `fsm:state:2`, and remains the
identity of every record that declares it.

### Macrosteps

A create, event send, or deadline poll runs one **macrostep**: the trigger
microstep the rules above describe, then the machine's reactions to
quiescence, sealed as one outcome and one record. After the trigger, and after
every reaction, the engine repeats one selection until nothing selects:

1. **Eventless first.** Scan the active configuration exactly as for an event
   (rules 3–4) over the transitions with no `on`, reading `ctx` alone. Every
   guard false is quiescence for this scan; a guard that fails to evaluate
   rejects the macrostep.
2. **Then the queue front.** Otherwise take the oldest internal event — raised
   by a block, or generated when a `final` leaf or a region's terminal leaf was
   entered — and scan for its handler with the raised payload bound as `evt`.
   No handler is a **discard**: recorded in the trace as `internal_unhandled`,
   never a rejection, and never subject to `on_unhandled`, which governs the
   trigger microstep only.
3. **Quiescence.** No eventless candidate and an empty queue end the macrostep.

Raised events queue in block order behind everything already waiting
(breadth-first); a microstep's own raises precede the done events it
generated, `$done.state.*` before `$done.region.*`, in entry order and region
document order respectively. A generated event is raised only when some
transition names it in `on`. Effects number continuously across the microsteps
of one macrostep.

Three exceptions to "each microstep is a step": the invariants (rule 8) are
evaluated **once**, at quiescence, on the final context and configuration;
`evt` is bound only in the microstep whose trigger supplied it, so an
eventless transition's guard and block see none; and one `now_ms` serves the
whole macrostep, each microstep's schedules settling by rule 9 as the next
reaction is selected and the last after the invariants.

The macrostep is **atomic**. Any failure in any microstep — a guard or action
error, the invariants, a schedule, or the ceiling — rejects the whole
macrostep, the instance keeps the state it had, and the rejection's trace
keeps every microstep that ran. Applied reactions and discards together MUST
NOT exceed `MAX_MICROSTEPS` (64): the 65th is refused as `run/microstep_limit`,
naming the last microstep. A definition whose eventless transitions provably
never quiesce is refused at admission (`def/eventless_cycle`); a guarded cycle
is admitted with a warning and stopped by the ceiling at run time.

The internal queue is a value of the macrostep alone and is **never persisted**:
it MUST NOT appear in `fsm.state/3` or any record, and it is empty at every
sealed state, so an instance resumed from its records has nothing to resume. The record carries the reactions as `microsteps`
(§Record kinds), absent when there were none, and the decision trace carries
each microstep's candidates and pipeline.

### `run/*` catalogue

| Code | Trigger | Hint policy |
|---|---|---|
| `run/configuration_invalid` | active configuration, lifecycle/terminal status, history, or deadline schedule set is incoherent for the machine | reconstruct the state from a trusted create/step/poll result |
| `run/unhandled` | no candidate on the chain | add a transition or send a handled event — this is a definition gap, not a payload miss |
| `run/not_enabled` | candidates exist but every guard is false | fix the payload or add a child override |
| `run/guard_error` | guard evaluation failed | source state, index, span |
| `run/action_error` | a transition/deadline pipeline action failed; a grandfathered ownerless history target reports cause `def/shape` | name the block or replace the grandfathered definition |
| `run/invariant` | enforce invariant failed | list every failing invariant |
| `run/microstep_limit` | the macrostep's reactions reached `MAX_MICROSTEPS`; nothing applied | name the last microstep's source state and transition, and say which guard to make false |
| `run/instance_completed` | event or deadline poll against a completed instance | — |
| `run/instance_cancelled` | event or deadline poll against a cancelled instance | — |
| `run/create_failed` | creation failed; **unjournaled** | wrap the inner error |
| `run/overflow` | checked arithmetic in an action/guard | operand strings |
| `req/event_unknown` | undeclared event | — |
| `req/elicit_nested` | a second elicitation while one is outstanding | name the outstanding question |
| `req/elicit_failed` | the client answered an elicitation with an error, or cancelled it | say that nothing was journaled and the `request_id` is unclaimed |
| `req/elicit_timeout` | no answer to an elicitation within the limit, or the client left | offer `instance_send` as the direct path |
| `req/elicit_unsupported` | an ask with no client able to answer it | name `instance_send` as the direct path |
| `req/event_internal` | an event declared `internal`, or a `$`-prefixed generated name, sent from outside | name where the machine raises it and list the sendable events |
| `req/invoke_slot_state` | `invoke_child` against a slot that is not `pending` | name the slot's current status and the slots the instance has |
| `req/cancelled` | the client withdrew the request; the call stopped at its next coarse boundary | say that a single engine step is not interruptible |
| `req/signal_target` | a `signal` addressed to its own sender | name `raise` as the construct for an event to this instance |
| `req/migrate_settled` | migrating an instance that is completed or cancelled | say which status it holds; a settled instance has nothing to migrate |
| `req/migrate_unmapped` | the instance's active state has no entry in the mapping | name the state and the mapping's keys |
| `req/migrate_not_superseded` | the target definition does not supersede the instance's current one | name both machine ids |
| `req/migrate_slot` | the instance holds an invocation slot the migration cannot carry | name the slot and its status |
| `run/invoke_create_failed` | creating an invoked child failed; nothing is journaled and the slot stays `pending` | carry the child's own rejection as the cause |
| `req/field_missing` | declared field absent | — |
| `req/field_unknown` | extra field | — |
| `req/field_type` | value does not match declared type | — |
| `req/field_scale` | decimal has too many fraction digits | — |
| `req/number_token` | raw JSON number | quote it |


## Composition

A machine MAY declare `invoke` on a state: a list of at most
`MAX_INVOKES_PER_STATE` slots, each `{id, machine, with?, returns?}`. Entering
the state creates one pending slot per entry; exiting it removes them, and any
slot whose child was running is cancelled (§Cascade).

`machine` MUST be a 64-hex `machine_id` digest, never a name. A name is a
mutable pointer: the definition it resolves to can change between the
invocation and its replay, and a store whose history depends on what a name
means today is not replayable. Admission refuses an unknown digest with
`def/invoke_unknown_machine`.

A child's instance id is **derived**, never allocated:

```
child_instance_id(parent, slot)
  = "inst-" + hex(sha256("fsm:child:1" | 0x0A | parent | 0x00 | slot))[..24]
```

The domain string is `fsm:child:1`, the separator after it is one `0x0A`
byte, and the separator between the parent id and the slot is one `0x00`
byte. The digest is truncated to 24 hex characters. Two writers that invoke
the same slot therefore agree on the child's id without coordinating, and a
reader can check any child id against the record that created it.

`with` maps a **child** context field to an `expr/1` expression evaluated in
the parent's scope when the slot is created; a field the child does not
declare is `def/invoke_unknown_ctx` and a type that does not match the child's
declaration is `def/invoke_type`. `returns` maps a **parent** event field to a
child context field, read out of the child's final context when the
invocation returns. The generated event `$done.invoke.<slot>` carries exactly
the `returns` projection, at the child's declared types; its declarations come
from the child machine, which is why admission needs the catalogue.

An invocation graph MUST NOT exceed `MAX_INVOKE_DEPTH` machines
(`def/invoke_depth`) and MUST NOT contain a cycle (`def/invoke_cycle`). A
cycle is unreachable by construction — a machine would have to contain the
digest of a definition that contains its own digest — and the rule is stated
and enforced anyway, as defence in depth.

### Invocation operations

Two journaled operations enact a slot, because a state change caused by
something outside the instance MUST be a record somebody can point at:

`invoke_child(parent, slot, request_id)` is legal only against a `pending`
slot (`req/invoke_slot_state`). It creates the child by running the child
machine's creation with the slot's evaluated `with` as overrides, and writes
one `instance_invoked` record naming both instances and both state hashes.
The slot moves `pending → running`. A child whose creation is rejected is
`run/invoke_create_failed`: **nothing** is journaled and the slot stays
`pending`. There is no `instance_created` record for a child — the fold
derives the child from the `instance_invoked` record by running the same
creation — so a reader looking for one MUST accept either kind.

`invocation_return(parent, slot, request_id)` is legal only against a
`running` slot whose child is `completed` or `cancelled`
(`req/invoke_slot_state` names the child's status). It writes one
`invocation_returned` record carrying the `returns` projection of the child's
final context, and delivers `$done.invoke.<slot>` into the parent as the
trigger of an ordinary macrostep, so the parent's whole reaction seals in that
record's `microsteps`. A cancelled child returns `outcome: "cancelled"` with
an **empty** payload; a parent that must distinguish models a declared field
for it, because `outcome` MUST NOT be injected into a payload whose shape the
child's declarations promised. A parent with no transition on the event
discards it (§Macrosteps) and the record still commits. The slot moves
`running → returned` and stays until its state is exited.

A generated `$done.invoke.<slot>` event follows the handler-only rule
(§Macrosteps): it is raised only when some transition names it in `on`.

### Cascade

Leaving an invoking state cancels every running child of the slots it
removed, with `reason: "parent-exit:<parent>/<slot>"`, in the same operation.
Cancelling an instance cancels every running descendant depth-first, bounded
by `MAX_INVOKE_DEPTH`, with `reason: "parent-cancel:<instance>"`. A child that
already settled is skipped, never re-cancelled.

This is the one place the store writes two records for one request, and the
window between them is documented rather than denied: a crash there leaves the
child `running` and unreferenced. That is safe because the second record is a
cancellation — idempotent and state-independent — so nothing is corrupt, only
unreferenced. A store open MUST NOT repair it: `fsm doctor` reports every
running child whose parent slot is gone or whose parent has settled, and `fsm
repair --cancel-orphans` settles each with one `instance_cancelled` record
carrying `reason: "orphan"`. A group commit would close the window at the
price of the one-fsync-per-record durability claim, which is the worse trade.

### Signals

A block MAY declare `signal`: at most `MAX_SIGNALS_PER_BLOCK` entries, each
`{to, event, with?}`, evaluated under the same snapshot semantics as `do`,
`emit`, and `raise`, and enqueued only from a block that commits. Signal ids
are `{instance_id}/{seq}/{k}` with `k` running in its own sequence,
independent of the effect `k`.

`to` is an `expr/1` expression of type `str` naming **exactly one** instance.
A query-targeted delivery MUST NOT be added: the set a query matches grows
over time, so replaying the record would deliver to a different set and the
store would stop being a function of its journal.

`event` and `with` are **not** typed at admission. The target machine is a
run-time value, so its declarations are unknown when the sender is admitted;
the check belongs to delivery, where the target's own machine validates the
event name and payload. This is the only construct in this specification whose
payload typing is a delivery-time check.

`signal_deliver(sender, signal_id, request_id)` applies the event to the
target as an ordinary macrostep and journals one `signal_delivered` record
naming both instances. `outcome` is one of `applied`, `ignored`,
`target_missing`, `target_settled`, or `rejected:<the target's code>`. Every
one of those clears the sender's pending entry: a signal is fire-and-forget by
design, and a sender that needs an answer models the target signalling back.
Delivery is NOT a transition of the sender, and MUST NOT advance its
configuration. A signal addressed to its own sender is `req/signal_target` and
journals nothing; `raise` is the construct for an event to this instance.

## Evolution

`machine_id` is a content hash, so editing a definition mints a different
machine and every in-flight instance stays bound to the old one. A machine
MAY therefore declare one optional top-level `supersedes` block —
`{machine, states?, context?}` — naming by 64-lowercase-hex digest the
definition it replaces, mapping old state names to new ones, and mapping new
context variables to `expr/1` expressions.

The block is part of the canonical definition and therefore **MUST** be
inside `machine_id`. Two definitions differing only in their mapping are
different machines. This is the property the whole feature rests on: a
reader holding the new hash holds the mapping too, so a migration can never
be reinterpreted after the fact. It also means adding `supersedes` to a
definition produces a *new* machine and never changes an existing one.

At most one block per definition. A three-definition chain migrates in two
journaled hops; a transitive closure computed by the engine would be a
mapping nobody wrote.

### Admission

Two rules are decidable from the definition alone:
`def/supersedes_machine_ref` (not a bare lowercase digest — a name is a
mutable pointer, and a store whose migrations depend on what a name means
today is not replayable) and `def/supersedes_self`, which its own hash makes
unsatisfiable. The rest need both definitions and run at `define_machine`,
so an author learns their mapping is wrong when they write it:

| Code | Trigger |
|---|---|
| `def/supersedes_unknown_machine` | this store holds no such machine; the definition is refused rather than accepted and failed later |
| `def/supersedes_unknown_state` | a mapping key or value names a state its definition does not have |
| `def/supersedes_target_not_leaf` | a value names a compound or a history pseudostate; an active configuration only ever holds leaves |
| `def/supersedes_target_terminal` | a value names a terminal or `final` state; completing a workflow by migrating it hides the completion from its own history |
| `def/supersedes_region` | the two machines disagree on shape or on their region-name set; region topology is not mappable |
| `def/supersedes_ctx_unknown` | a mapping key names a variable the new definition does not declare, or an expression reads one the old definition does not |
| `def/supersedes_ctx_type` | an expression's type differs from the new declaration, decimal scale included |
| `def/supersedes_slot` | the old machine has an invoke slot the new one does not |

A `context` expression is typed with the **old** machine's context in scope
and the **new** machine's variable as the target: it reads what the instance
holds today and writes what it will hold tomorrow.

### The migration

Migrating one instance runs seven steps, in this order:

1. **Gate.** A `Completed` or `Cancelled` instance is `req/migrate_settled`.
2. **Map the configuration.** Every active leaf — the one for a sequential
   instance, each region's for a parallel one — must have a mapping entry, or
   the whole migration is `req/migrate_unmapped` naming the leaf and its
   region. Partial migration is never performed and no leaf is ever guessed.
   The target machine MUST supersede the machine the instance is on, or the
   migration is `req/migrate_not_superseded`; there is no override.
3. **Project the context.** Each mapping expression is evaluated against the
   **old** context. A new variable nobody mapped takes its declared `init`;
   an old variable nobody references is dropped. An evaluation failure is
   `run/action_error` with the block named `migration`.
4. **Carry over** (§Carry-over).
5. **Invariants.** The new definition's invariants are evaluated on the
   migrated state, before any reaction: an enforce failure is `run/invariant`
   and monitor failures are reported without blocking.
6. **React to quiescence.** A migrated instance runs its reaction phase
   exactly as a freshly created one does, so a mapped leaf with an eventless
   exit does not park in a state its own machine says it should have left.
   `instance_migrated` therefore carries a `microsteps` array under the same
   absent-when-empty rule every other record uses.
7. **Return.** Status stays `Running` unless the reaction reached a terminal
   leaf.

Every refusal is atomic: the instance is untouched and no partial state
escapes.

### Carry-over

| What | Ruling |
|---|---|
| history | remapped when both ends are mapped, **dropped** otherwise and listed in the report — a binding concerns a state the instance is not in, so losing one degrades a future re-entry rather than corrupting the present |
| deadlines | **recomputed, never carried** |
| pending effects | retained verbatim; an effect id names the record that emitted it, and that record's machine is still in the catalogue |
| invocation slots | carried when the new definition declares the same slot with the same child machine; otherwise `req/migrate_slot` for a `Running` slot, and a `Returned` slot is dropped with a report entry |
| pending signals | retained verbatim; a signal's event belongs to the *target's* machine, so neither mapping bears on it |

Two of those an operator **MUST** know before migrating anything:

- **Migration reschedules every deadline from the migration instant.** Every
  existing schedule is dropped and the new machine's are computed for the
  mapped configuration from the migration's own `now_ms`. A deadline that was
  about to fire starts over. Carrying an old due time would keep a promise
  the new definition never made.
- **A `Running` invocation slot with no counterpart refuses the whole
  migration** (`req/migrate_slot`). A running child is a live instance doing
  work and cannot be dropped the way a history binding can.

### Replay

An instance's records legitimately span two definitions. A fold tracks the
**current machine per instance** and switches it on an `instance_migrated`
record; every subsequent record for that instance replays against the new
definition, and every earlier one against the old. The record's
`from_machine_id` MUST equal the machine the fold holds for that instance, or
the fold fails: a record claiming to migrate from a machine the instance was
not on is corruption, not a reinterpretation.

A superseded machine is **never** removed from the catalogue. Records written
before a migration replay against it, and a pending effect's name re-derives
from the machine that emitted it.

A migration is journaled at the record's own `ts`, which is the `now_ms` the
pure function received, so the deadline rescheduling a record describes is
reproducible without a clock. Replay re-runs the migration and checks
`state_hash`, `configuration_after`, `dropped_history`,
`rescheduled_deadlines`, and `microsteps` in both directions.

A cohort migration is **not atomic**: it is N idempotent operations, each
keyed on `migrate-{instance_id}-{to_machine_id}`, both halves derived from
journaled content. A crash halfway leaves half the cohort migrated, and
re-running finishes it.

## Journal

### Idempotency

`request_id` is an idempotency key over the *content* of a request, not a label
on a slot. Every record that claims a key also stores `request_fp`, a
`fsm:request-fp:1` digest of the operation and its arguments — for a send, the
instance, event, and payload as received. Resending a key:

- with the same fingerprint replays the original outcome, marked `duplicate`;
- with a different fingerprint is `req/request_id_conflict`, never a replay.

Without that check a driver deriving ids from (task, event) rather than
per-attempt would receive the *first* request's success for a second, different
request, and diverge from the instance silently. `req/request_id_conflict` is
not retryable: the remedy is a new key, and the old key still replays its own
outcome. `expect_seq` is excluded from the fingerprint — it is a concurrency
precondition, so refreshing it across a retry must not look like new content.

Keys claimed by records written before store format 7 carry no fingerprint and
remain replay-only; the format is migrated on open without rewriting records.

### Payload size

Event payloads, effect-ack `result`s, and annotation notes are journalled
verbatim and never rewritten, so their cost is permanent and is paid again on
every fold, snapshot, and verify. Anything larger than `MAX_PAYLOAD_BYTES`
(64 KiB of canonical bytes) is refused with `req/payload_too_large` — journal a
digest or an identifier and keep the blob in its own store. The check runs
before the request is applied and does not depend on instance state, so like
`req/seq_mismatch` it is unjournaled and does not consume `request_id`: correct
the payload and resend under the same key.

Store-side event stamping measures the final candidate payload after every
absent requested timestamp field has been filled from one reserved timestamp.
If that candidate is oversized, the caller's payload, journal, logical state,
request id, and built-in injected clock MUST remain unchanged. On acceptance,
when the request is journalled, the missing fields and journal record use that
reserved timestamp and the built-in clock commits it exactly once.

`MAX_PAYLOAD_BYTES` is deliberately absent from the genesis `limits` block,
which is hash-verified on fold; adding a key there would make every store
written by an earlier build unreadable rather than migratable.

A request-outcome record exists **iff** the outcome depended on instance state and is not retry-stable. The unique admitted state-dependent-but-retry-stable case is `expect_seq` mismatch (`req/seq_mismatch`): it is unjournaled and does not consume `request_id`. Dedup lookup MUST precede the `expect_seq` check — otherwise a lost-response retry with a stale seq would be rejected, the client would "fix" the seq under a new request_id, and the event would apply twice. `run/create_failed` is the one unjournaled `run/*` outcome (no prior instance exists). `state_checkpoint` is a maintenance record rather than a request outcome; it changes no logical state and consumes no `request_id`.

Envelope (one canonical LF-terminated line, domain `fsm:record:1` over the envelope minus `hash`):

`{"body":…,"hash":"<64 hex>","kind":"…","prev":"<64 hex>","seq":…,"ts":…}`

Genesis is `seq` 0, `prev` sixty-four `0`s, body `{format: "fsm.journal/1", created_ts, limits}`.
New stores bind every definition ceiling in the table above, including
`max_regions`, `max_deadlines`, and `max_eval_ticks`. For migration, readers
also accept the exact historical limits object that predates those three keys;
partial or otherwise modified tables are invalid. Existing genesis records and
their hashes are never rewritten.

During a complete fold of a hash-verified journal whose sequence-zero genesis
carries the exact historical limits object, `machine_defined` records are
compiled without the aggregate `def/limit_eval` ceiling. For a sequential,
deadline-free definition, that compiler MUST also preserve the legacy
history-shape admission bug: a history node may be top-level, may own children,
or may carry `terminal` or `initial`, and an event transition may target an
ownerless history node. Other parse, type, and structural checks remain
enforced. This is a journal-level replay compatibility rule: the format records
no authenticated per-definition introduction version, so a complete
historical-genesis fold cannot distinguish definitions written before and after
migration. It is not definition admission. Every new definition write and every
`fold_from` tail uses the current compiler and MUST satisfy `def/limit_eval` and
the current history shape; a current genesis never enables the compatibility
rule. Current-valid parallel or deadline definitions appended after migration
remain replayable in a complete historical-genesis fold, but receive no
malformed-history exception. A snapshot that needs the historical compiler is
operational only when its state is bound to that exact historical-genesis
journal.

Replay MUST also accept the active pseudostates and deep or shallow bindings
that the legacy stepper could emit from child-bearing or nested history nodes.
For a malformed history node carrying `initial`, historical execution retains
the old global-name lookup, including a jump to a state that is not its child;
current definitions cannot express this shape. A cyclic malformed lookup has no
sealed Applied outcome and MUST terminate safely rather than hang replay or a
new operation.
Selecting a top-level ownerless history target could only panic in the old
stepper and therefore has no sealed Applied outcome; a new operation on such a
grandfathered machine rejects as `run/action_error` with cause `def/shape`
instead of panicking. When verifying an `event_rejected` record in a historical
journal, a reader MUST also accept the historical enabled-event diagnostic that
did not charge omitted guards; current diagnostics charge their implicit `true`
exactly like runtime selection. This diagnostic compatibility applies only to
sealed rejection details, never to a new enabled-event scan. Likewise, a sealed
`event_rejected` whose details carry `cause: internal/budget` can only have
been written when one step's budget was 4096 ticks and the compiler of that
day did not charge omitted guards: when the macrostep budget reproduces no
rejection, a reader MUST re-run that record under the historical single-step
budget and accept an exact match. This applies to sealed rejections alone,
never to a new operation; no sealed deadline rejection can carry that cause,
because a deadline poll visits no event guard.

### Record kinds

| Kind | Body fields |
|---|---|
| `genesis` | `format`, `created_ts`, `limits` |
| `machine_defined` | `machine_id`, `def` |
| `instance_created` | `instance_id`, `machine_id`, `request_id`, `state_hash`, `state_format`, `configuration`, `overrides`, optional `microsteps` |
| `event_applied` | `instance_id`, `event`, `payload`, `request_id`, `state_hash`, `state_format`, `exited`, `entered`, `source_state`, optional `microsteps` |
| `event_rejected` | `instance_id`, `event`, `payload`, `request_id`, `state_hash`, `state_format`, `code`, `message`, `hint`, `details`, optional `span` |
| `event_ignored` | `instance_id`, `event`, `payload`, `request_id`, `state_hash`, `state_format` |
| `deadline_applied` | `instance_id`, `deadline`, `deadline_idx`, `due_ms`, `request_id`, `state_hash`, `state_format`, `exited`, `entered`, `source_state`, optional `microsteps` |
| `deadline_rejected` | `instance_id`, `deadline`, `deadline_idx`, `due_ms`, `request_id`, `state_hash`, `state_format`, `code`, `message`, `hint`, `details`, optional `span` |
| `deadline_not_due` | `instance_id`, `request_id`, `state_hash`, `state_format`, either all or none of `next_deadline`, `next_deadline_idx`, `next_due_ms` |
| `effect_acked` | `instance_id`, `effect_id`, `request_id`, `outcome` (`ok` or `failed`), `state_hash`, `state_format`, optional `result` |
| `request_rejected` | `request_id`, `instance_id`, `code`, `message`, `hint`, `details`, `operation`, `state_hash`, `state_format`; `effect_id` required when `operation` is `ack` |
| `instance_cancelled` | `instance_id`, `request_id`, `reason`, `state_hash`, `state_format` |
| `instance_invoked` | `parent_instance_id`, `slot`, `child_instance_id`, `child_machine_id`, `overrides`, `request_id`, `state_hash`, `child_state_hash`, `state_format` |
| `invocation_returned` | `parent_instance_id`, `slot`, `child_instance_id`, `outcome` (`completed` or `cancelled`), `payload`, `request_id`, `state_hash`, `state_format`, optional `microsteps` |
| `signal_delivered` | `sender_instance_id`, `signal_id`, `target_instance_id`, `event`, `payload`, `outcome`, `request_id`, `sender_state_hash`, `state_format`, optional `target_state_hash`, optional `microsteps` |
| `instance_migrated` | `instance_id`, `from_machine_id`, `to_machine_id`, `configuration_before`, `configuration_after`, `dropped_history`, `rescheduled_deadlines`, `request_id`, `state_hash`, `state_format`, optional `microsteps` |
| `annotated` | `instance_id`, `request_id`, `note` |
| `effect_attempted` | `instance_id`, `effect_id`, `attempt` (1-based, strictly `last + 1`), `outcome` (always `failed`), `request_id`, `state_hash`, `state_format`, optional `result`. Leaves the effect pending and changes no logical state: a retry counter kept in memory is lost by exactly the restart it exists to survive, so the attempt count is derived from these records. A *successful* attempt is an ordinary `effect_acked` and writes none of these |
| `state_checkpoint` | `state_root`, `state_root_format` |
| `journal_sealed` | `sealed_through_seq`, `sealed_last_hash`, `base_state_root`, `state_root_format`, `base_dedup_fp_root`, `base_dedup_format`, `archive_id`, `records_sealed`. Marks a sealed and detached prefix. It claims no `request_id` and changes **no** logical state: the loader reads it before folding, and the fold applies it as a marker exactly as it applies `state_checkpoint`. It is appended at `sealed_through_seq + 1`, so `sealed_last_hash` MUST equal `sha256:` followed by the record's own `prev` — the body asserts a join the chain already made, and a record where the two disagree is corrupt. `state_root_format` names the format of `base_state_root`, which is the root of the state the base file materializes at `sealed_through_seq`, **after** the dropped dedup entries were removed; it is NEVER equal to the `state_root` a record on the same sequence would carry, and a reader MUST NOT assert them equal |

`microsteps` is the macrostep's reaction list: `[{index, trigger, event?,
source_state, transition_idx, exited, entered}]` with `index` starting at 1
(index 0 is the trigger, described by the record's own `exited`, `entered`,
`source_state`, and `deadline_idx` fields, whose meaning does not change),
`trigger` either `eventless` or `internal`, and `event` present exactly when
the trigger is `internal`. The key MUST be absent, never empty, when a
macrostep had no reaction microsteps: that absence is what keeps every
non-reactive record's canonical bytes, hash, and chain identical to the bytes
an earlier build wrote. `state_hash` commits the state after the whole
macrostep; the internal queue is never part
of it. Replay verifies the claim in both directions (§Verification).

Every deadline-poll record timestamp is the exact `now_ms` passed to the pure
poll, so replay NEVER consults a clock. This includes `deadline_not_due`: the
record proves the negative observation and makes retries exact.

Records written by store `VERSION` 9 that carry `state_hash` also carry
`state_format: "fsm.state/3"`; records written by `VERSION` 8 carry
`"fsm.state/2"` and verify under it forever; records from earlier versions omit it and verify
under `fsm.state/1`. Likewise, every new record carrying `state_root` carries
`state_root_format: "fsm.state-root/3"`; an absent field denotes the historical
`fsm.state-root/2`. These per-record discriminators let migration verify old
bytes without rewriting or guessing.

Verification: the stored line MUST equal its canonical re-serialization; seq
is consecutive; `prev` matches the prior hash; `hash` is recomputed; fold
re-applies through `step`/`create`/`poll_deadline` — as macrosteps, under the
macrostep budget — using the record timestamp and checks journaled
`state_hash` / `exited` / `entered` / `source_state`, and `microsteps` in
both directions: a journaled array MUST match the re-derived reactions entry
for entry, and an absent key requires that replay derived none. No
historical-compiler exception applies to reactions: the reactive shapes are
opt-in syntax, so a definition written before them cannot acquire one on
recompilation.
Duplicate `request_id` values are a fold error. `effect_acked` and
`instance_cancelled` commit the post-operation instance `state_hash`. A record
carrying `state_root` commits the complete logical store state after that
record at its `seq`; the root excludes the record hash to avoid a cycle, and
replay MUST recompute it.

Every persistence input read as one unit is bounded by
`JsonLimits::DEFAULT.max_bytes`
(16 MiB). A `VERSION` file larger than that ceiling is a fatal `io/read` on
open. Journal segment files may be larger because they are streamed, but each
record's canonical envelope, excluding its terminating LF, MUST be at most the
ceiling. The exact ceiling is accepted. An append that would exceed it returns
`io/write` before segment rotation or any write and MUST NOT change journal
bytes, sequence, hash, logical state, or idempotency state; a Store request may
be corrected and retried under the same `request_id`. An oversized journal
record encountered while opening is authoritative input and is therefore a
fatal `io/read`, never a torn-tail repair candidate.

On-disk store `VERSION` is `9`. Opening a `VERSION` `1` through `8` directory,
or a journal with no `VERSION` marker, MUST attempt a best-effort migration:
ignore snapshot caches entirely, fold the complete journal using each record's
format discriminator, and on success stamp `VERSION` `9`. Interior journal
records MUST NOT be rewritten. If classify is not `Ok` (including a migratable
marker whose journal is missing) or fold fails, refuse with that health and
leave `VERSION` unchanged — a migratable directory is never re-created over. A
successful `repair --truncate-torn-tail` on a migratable store folds the
complete retained journal and likewise stamps `VERSION` `9`. Any other
`VERSION` value is `store/version_mismatch`, refused and never silently
reinterpreted.

`Store::open_read_only` and CLI inspection MUST NOT create directories, take
the advisory writer lock, stamp or migrate `VERSION`, or write snapshots.
`Store::open_read_only` returns one self-consistent journal prefix even if a
live writer appends after that prefix is read. It omits an unterminated line
only at the end of the lexically final segment as an in-progress append;
strict `load_records`, writer open, classification, and verification report
that same line as `TornTail`, and an unterminated non-final line is interior
corruption. Mutating methods on a read-only Store refuse with `io/write`.

### Recovery

| Health | Posture |
|---|---|
| `Ok` | open |
| `TornTail` | refuse; remedy `fsm repair --truncate-torn-tail` (quarantine tail bytes, then truncate) |
| `ChainBroken` | refuse; interior; no repair; blast radius `records ≥ N unverifiable` |
| `StateHashMismatch` | refuse; no repair |
| `NonCanonical` | refuse; no repair |
| `LockIo` | refuse; actual lock acquisition or contention fault |
| `StoreIo` | refuse as `io/read`; repair the filesystem or input fault |

The MCP tools `journal_verify` and `store_doctor` report these names and, where
this table prescribes one, its remedy command verbatim; `journal_replay` checks
the complementary property, that replaying the journal reproduces the outcomes
it recorded. The postures above are normative — those tools report them and do
not restate them.

#### Durability across platforms

Every append fsyncs the segment **file** before returning, on every platform.
What differs is the enclosing directory entry: after creating or renaming a file
(segment rotation, snapshot installation, the request-id allocation file) the
store also fsyncs the containing directory, and that step is Unix-only. Windows
exposes no portable equivalent — opening a directory as a file fails outright,
and flushing a directory handle requires `FILE_FLAG_BACKUP_SEMANTICS`, which the
standard library does not offer.

The consequence on Windows is bounded: a crash in the window between a rename
and the directory metadata reaching disk can leave the entry missing even though
the file's bytes were flushed. It cannot corrupt a record, because record
durability does not depend on it. Every such case lands in the table above and
is classified on the next open rather than trusted, so the outcome is a recovery
step, not silent loss.

Interior history is never rewritten. Snapshots (`fsm.snapshot/5`) are
disposable caches, never authoritative, never part of the chain. Each snapshot
carries a self-checked `state_root`: `sha256:` plus the hex encoding of domain
`fsm:state-root:3` over canonical `{seq,machines,instances,dedup}` using the
same values and per-instance state hashes as the snapshot; `last_hash`
is excluded to avoid a cycle. The fast path is permitted only when the journal
record at the snapshot sequence has the same hash as the snapshot's
`last_hash`, carries the same `state_root`, and declares
`state_root_format: "fsm.state-root/3"` in its hash-chained body. Because that
root binds each dedup request id to its claiming sequence but not
the request fingerprint bytes, the fast path MUST also compare every snapshot
fingerprint with `request_fp` in the hash-verified claiming record at that
sequence, including exact absence for migrated legacy claims. Explicit
snapshots append a `state_checkpoint`; the automatic 10,000-record snapshot
commits the root in that existing boundary record. A clean-shutdown cache
without a journal-bound root is accepted only after folding the complete
journal prefix and proving exact state equality, so it is not a fast path.
Mutable sidecar files are never trust anchors. `fsm.snapshot/1` through
`fsm.snapshot/3` caches are skipped, never reinterpreted. Snapshot caches over
the 16 MiB persistence-unit ceiling are likewise skipped on read and the
authoritative journal is folded. A writer MUST detect an oversized canonical
snapshot before creating, pruning, or installing any cache file and returns
`io/write`; automatic best-effort snapshotting may proceed with no cache.

## Expressions

Grammar version `expr/1`. Keywords are reserved: `if then else and or not true
false ctx evt`. Mode and unit words (`half_even`, `ms`, …) are ordinary
identifiers; position, not reservation, disambiguates them. Identifiers are
`[a-z_][a-z0-9_]{0,63}`. Type identifiers are `[A-Z][A-Za-z0-9_]{0,63}`. There
are no comments. Source over 4,096 bytes is `expr/too_long`. `/` and `%` are
not tokens; `a / b` fails at the lexer with a hint naming `div(a, b, scale, mode)`.

```ebnf
expr        = if_expr ;
if_expr     = "if" , or_expr , "then" , if_expr , "else" , if_expr | or_expr ;
or_expr     = and_expr , { "or" , and_expr } ;
and_expr    = not_expr , { "and" , not_expr } ;
not_expr    = "not" , not_expr | cmp_expr ;
cmp_expr    = add_expr , [ cmp_op , add_expr ] ;          (* non-associative *)
cmp_op      = "==" | "!=" | "<=" | "<" | ">=" | ">" ;
add_expr    = mul_expr , { ( "+" | "-" ) , mul_expr } ;
mul_expr    = unary_expr , { "*" , unary_expr } ;
unary_expr  = "-" , unary_expr | primary ;
primary     = int_lit | dec_lit | str_lit | "true" | "false"
            | ( "ctx" | "evt" ) , "." , ident
            | type_ident , "." , ident
            | ident , "(" , [ arg , { "," , arg } ] , ")"
            | "(" , expr , ")" ;
arg         = expr | ident ;                               (* bare ident = Word *)
```

A second comparison operator in one `cmp_expr` is `expr/chained_cmp` with hint
exactly `use `and` to combine comparisons`. Integer literals that do not fit
`i64` are `expr/int_range`. Decimals with more than 38 digits or more than 12
fraction digits are `expr/dec_range`. More than 512 AST nodes is `expr/too_long`.
Nesting beyond depth 32 is `expr/too_deep`. Other mismatches are `expr/parse`
with the expected-token set in the hint.

### Types

`Bool`, `Int`, `Dec(scale ≤ 12)`, `Str`, machine-declared enums, `Ts`, `Dur`.
Rendered as `bool`, `int`, `decimal(N)`, `str`, `enum Name`, `timestamp`,
`duration`.

| Construct | Rule |
|---|---|
| `IntLit` | `Int` |
| `DecLit` | `Dec(s)` where `s` is the fraction-digit count |
| `+ -` | `Int×Int→Int` · `Dec(s1)×Dec(s2)→Dec(max(s1,s2))` · `Ts+Dur→Ts`, `Dur+Ts→Ts`, `Ts−Ts→Dur`, `Ts−Dur→Ts`, `Dur±Dur→Dur` · everything else `expr/type_mismatch`; `Dec` with `Int` → `expr/mixed_class` (hint: write `0.00`-style literals or `dec(x, s)`) |
| `*` | `Int×Int→Int` · `Dec(s1)×Dec(s2)→Dec(s1+s2)`, statically `expr/scale_cap` when `s1+s2 > 12` · `Dec(s)×Int→Dec(s)` and `Int×Dec(s)→Dec(s)` (exact) · `Dur×Int→Dur`, `Int×Dur→Dur` |
| unary `-` | `Int`, `Dec`, `Dur` only |
| `cmp` | both sides same class; `Dec` compares by value across scales; full order on `Int`, `Dec`, `Ts`, `Dur`; `Str`, `Enum`, `Bool` allow `==`/`!=` only, an ordering operator → `expr/cmp_unordered` |
| `and or not` | `Bool` operands |
| `if c then a else b` | `c: Bool`; branches unify in the same class; two `Dec` branches widen exactly to `Dec(max scale)` |
| `CtxRef`/`EvtRef` | declared name, else `expr/unknown_var`/`expr/unknown_field` with a Levenshtein suggestion (distance ≤ 2) plus the legal list; `EvtRef` in an invariant is `expr/evt_in_invariant`; in an entry/exit block is `expr/evt_in_block` |
| `EnumLit T.v` | `T` declared (`expr/unknown_enum`), `v` a variant (`expr/unknown_variant`); result `Enum(T)` |
| `Call` | signatures below; unknown name → `expr/unknown_builtin` listing the eight legal names |

### Builtins

Scale arguments MUST be integer literals `0..=12`. Mode and unit arguments MUST
be literal words. Otherwise the result *type* would depend on a runtime value
(`expr/scale_not_literal` / `expr/mode_invalid`). Wrong arity is `expr/arity`.

| Signature | Typing | Evaluation |
|---|---|---|
| `min(a, b)`, `max(a, b)` | both `Int` → `Int`; both `Dec` → `Dec(max scale)`; both `Ts` or both `Dur` | value comparison (Dec via `Dec::cmp`) |
| `abs(x)` | type-preserving on `Int`/`Dec`/`Dur` | checked (`abs(i64::MIN)` → `run/overflow`) |
| `dec(x, S)` | `Int → Dec(S)`; `Dec(s0) → Dec(S)` requires `s0 ≤ S` else `expr/scale_narrow` (hint: use `round`) | exact widen, total |
| `round(x, S, M)` | `Dec(s0) → Dec(S)`, `M` mandatory; warns `expr/round_widens` when `S ≥ s0` | `Dec::round` |
| `div(a, b, S, M)` | `a`, `b` each `Int` or `Dec` → `Dec(S)` | `Dec::div`; `b = 0` → `run/div_zero` |
| `dur(n, U)` | `n: Int`, `U ∈ ms s min h d` → `Dur` | checked multiply to milliseconds |
| `in(S)` | `S` a literal word naming a declared (non-history) state → `Bool`; only legal in an invariant, else `expr/state_out_of_scope`; `S` not a declared state is `expr/unknown_state` with a Levenshtein suggestion plus the legal list | `true` iff `S` is the active leaf or a compound ancestor of it in the final configuration, in any region |

`M ∈ {down, up, floor, ceiling, half_up, half_down, half_even}`.

### Evaluation

Evaluation is total, deterministic, and strict left-to-right. `and`/`or`
short-circuit; `if` evaluates only the taken branch. One `Budget` is shared
across every expression evaluation of a single create, step, or deadline poll
— each a whole macrostep, budgeted at 4096 × (64 + 2) ticks (Appendix B) —
or of a single enabled-event scan, budgeted at 4096; each AST-node visit,
including an omitted guard's implicit `true`, decrements it;
exhaustion is `internal/budget` (an engine-invariant breach, never a user
error). Compilation limits the definition's worst-case evaluation cost — all
compiled AST nodes plus one tick per distinct event with an omitted `if` — to
4096, so a fresh standard budget cannot exhaust on a definition accepted by
the current compiler during an enabled-event scan, and the macrostep budget,
sized for the trigger, `MAX_MICROSTEPS` reactions, and the closing scan at
that cost each, cannot exhaust during create, step, or deadline poll. A
caller-supplied smaller or already-consumed budget may still exhaust. All
`Int`/`Ts`/`Dur` arithmetic uses checked operations, including
`-(i64::MIN)` → `run/overflow`. Decimal arithmetic delegates to the decimal
module (`Overflow` → `run/overflow`).

### Partial evaluation

`partial_eval_bool` answers “could this guard pass?” when the next event payload
is unknown. Callers supply a `Scope` with declared enums and event-field types.
Lazy `if` reduces a concrete-true or concrete-false condition to the selected
branch before payload dependence is decided, so an unreachable `evt.*` branch
does not make a context-concrete guard `Unknown`. Remaining `EvtRef` is Kleene
`Unknown`. `and`/`or`/`not` follow the Kleene tables (`False and _ = False`,
`True or _ = True`, `not Unknown = Unknown`). Comparisons and arithmetic
containing an `Unknown` operand are `Unknown`. Fully-`ctx` subtrees evaluate
concretely. A concrete sub-evaluation error — including budget exhaustion —
yields `Unknown`. This is deliberately conservative: an erroring guard is
neither definitely enabled nor definitely disabled; the authoritative loud
failure (`run/guard_error`) happens at send time.

### Expression error catalogue

| Code | Trigger | Hint policy |
|---|---|---|
| `expr/too_long` | source > 4096 bytes, or > 512 AST nodes | split or shorten |
| `expr/too_deep` | nesting beyond depth 32 | flatten |
| `expr/lex` | unexpected byte, bad number/string form, `/` or `%` | `/` names `div(a, b, scale, mode)` |
| `expr/parse` | grammar mismatch or trailing tokens | expected-token set |
| `expr/chained_cmp` | second comparison in one `cmp_expr` | exactly `use `and` to combine comparisons` |
| `expr/int_range` | integer literal does not fit `i64` | use a smaller integer |
| `expr/dec_range` | more than 38 digits or 12 fraction digits | shrink the literal |
| `expr/type_mismatch` | operand class does not match the construct | name the expected type |
| `expr/mixed_class` | `Dec` mixed with `Int` on `+`/`-`/cmp | `0.00`-style literal or `dec(x, s)` |
| `expr/scale_cap` | `Dec×Dec` scale sum > 12 | round an operand first |
| `expr/unknown_var` | unknown `ctx` name | Levenshtein ≤ 2 plus legal list |
| `expr/unknown_field` | unknown `evt` name | Levenshtein ≤ 2 plus legal list |
| `expr/unknown_enum` | unknown enum type | suggestion plus legal list |
| `expr/unknown_variant` | unknown variant | suggestion plus legal list |
| `expr/unknown_builtin` | unknown call name | the eight legal names |
| `expr/cmp_unordered` | `<`/`>` on `Str`/`Enum`/`Bool` | use `==` or `!=` |
| `expr/evt_in_invariant` | `evt` in an invariant | invariants read `ctx` only |
| `expr/evt_in_block` | `evt` in an entry/exit block | blocks read/write `ctx` only |
| `expr/state_out_of_scope` | `in(state)` outside an invariant | guards, blocks, and actions cannot reference the active state |
| `expr/unknown_state` | `in(state)` names an undeclared or non-literal state | Levenshtein ≤ 2 plus legal list |
| `expr/scale_narrow` | `dec` would drop scale | use `round` |
| `expr/scale_not_literal` | scale is not an integer literal `0..=12` | types cannot depend on runtime values |
| `expr/mode_invalid` | bad or non-literal mode/unit | list the legal words |
| `expr/arity` | wrong argument count | expected N / found M |
| `expr/round_widens` | warning: `round` target scale ≥ operand | use `dec` |
| `run/overflow` | checked arithmetic overflow | operand canonical strings in `details` |
| `run/div_zero` | `div` by zero | name the divisor |
| `internal/budget` | shared operation budget exhausted | engine invariant |

## Appendix A — Error codes

Every stable code in `fsm_core::error::ALL_CODES`:

> These are the engine's codes. The effect executor has its own namespace,
> `exec/*`, which is deliberately not part of this appendix: nothing under it
> is a statement about statechart semantics. It is listed in
> [EMBEDDING.md](EMBEDDING.md#executor-error-codes).

- `def/ancestor_shadowed` — ancestor handler globally dead
- `def/assign_type` — set target type ≠ RHS
- `def/create_always_fails` — creation fails on declared inits
- `def/cross_region` — transition crosses a parallel-region boundary
- `def/deadline_type` — deadline after expression is not a duration
- `def/dup_name` — duplicate state or history name
- `def/dup_set` — duplicate set targets in one block
- `def/duplicate_deadline` — duplicate deadline name
- `def/duplicate_guard` — identical guards in one (from, on) group
- `def/eventless_cycle` — an eventless cycle no guard can stop
- `def/eventless_cycle_guarded` — warning: an eventless cycle only a guard can break
- `def/eventless_depth` — warning: an eventless cascade approaches the macrostep ceiling
- `def/eventless_evt` — an eventless transition references evt
- `def/eventless_from_terminal` — an eventless transition leaves a terminal state
- `def/eventless_internal_noop` — warning: an eventless transition that can only burn a microstep
- `def/eventless_shadowed` — a guardless eventless transition hides later eventless siblings
- `def/final_and_terminal` — final and terminal on one state
- `def/final_at_root` — a final state with no parent compound
- `def/final_has_transitions` — a transition or deadline from a final state
- `def/final_is_initial` — a compound that starts in its final child
- `def/final_not_leaf` — a final state with children
- `def/invoke_cycle` — the invocation graph closes a cycle
- `def/invoke_depth` — the invocation graph is deeper than four machines
- `def/invoke_dup_slot` — two invoke slots share an id
- `def/invoke_evt` — an invoke `with` expression reads evt
- `def/invoke_machine_ref` — an invoke names its machine other than by 64-hex digest
- `def/invoke_on_terminal` — an invoke on a terminal or final state
- `def/from_history` — history used as a transition source
- `def/history_target_from_inside` — history targeted from inside its owner
- `def/initial_is_history` — initial names a history node
- `def/initial_not_child` — initial is not a direct child
- `def/initial_terminal` — creation chain lands on a terminal
- `def/invoke_type` — a with projection type-mismatches the child's declaration
- `def/invoke_unknown_ctx` — a projection names a context variable the child does not declare
- `def/invoke_unknown_machine` — an invoke names a machine the store does not hold
- `def/limit_bytes` — definition exceeds 256 KiB
- `def/limit_cell` — more than 32 transitions per (state, event)
- `def/limit_ctx` — more than 64 context variables
- `def/limit_deadlines` — more than 128 deadlines
- `def/limit_depth` — nesting depth exceeds 12
- `def/limit_emits` — more than 8 emits per block
- `def/limit_enums` — more than 32 enums
- `def/limit_eval` — definition exceeds 4096 worst-case evaluation ticks
- `def/limit_events` — more than 128 events
- `def/limit_fields` — more than 32 fields
- `def/limit_history` — more than 32 history nodes
- `def/limit_invariants` — more than 64 invariants
- `def/limit_invokes` — more than 4 invoke slots on one state
- `def/limit_raises` — more than 8 raises per block
- `def/limit_regions` — more than 8 regions
- `def/limit_sets` — more than 32 sets per block
- `def/limit_states` — more than 256 states
- `def/limit_signals` — more than 4 signals in one block
- `def/invoke_only_exit` — a state that leaves only when an invoked child returns
- `def/invoke_result_unhandled` — nothing handles a slot's generated result event
- `def/limit_transitions` — more than 2048 transitions
- `def/limit_variants` — more than 64 variants
- `def/multiple_history` — more than one history per compound
- `def/one_initial` — compound missing exactly one initial
- `def/reserved_ident` — `$`-prefixed identifier
- `def/shadowed` — guardless transition hides later siblings
- `def/shape` — wrong JSON type, missing field, or malformed history pseudostate shape
- `def/supersedes_ctx_type` — a context expression type-mismatches the new declaration
- `def/supersedes_ctx_unknown` — a context mapping names a variable the new definition does not declare
- `def/supersedes_machine_ref` — a supersedes block names its machine other than by 64-hex digest
- `def/supersedes_region` — a state mapping crosses parallel regions incoherently
- `def/supersedes_self` — a definition supersedes itself
- `def/supersedes_slot` — a state mapping cannot carry the instance's invocation slots
- `def/supersedes_target_not_leaf` — a state mapping targets a state that is not a leaf
- `def/supersedes_target_terminal` — a state mapping targets a terminal state
- `def/supersedes_unknown_machine` — a supersedes block names a machine this store does not hold
- `def/supersedes_unknown_state` — a state mapping names a state that does not exist
- `def/terminal_has_transitions` — transition from a terminal
- `def/terminal_not_leaf` — terminal is not a leaf
- `def/unknown_effect` — emit names an unknown effect
- `def/unknown_enum` — unknown enum type
- `def/unknown_event` — unknown event name
- `def/unknown_key` — unknown key at a JSON-Pointer path
- `def/unknown_state` — unknown state name
- `def/unreachable_state` — state is not enterable
- `expr/arity` — wrong builtin arity
- `expr/chained_cmp` — two comparisons in one cmp_expr
- `expr/cmp_unordered` — ordering compare on unordered type
- `expr/dec_range` — decimal literal out of range
- `expr/evt_in_block` — evt in an entry, exit, or deadline block
- `expr/evt_in_invariant` — evt in an invariant
- `expr/int_range` — integer literal out of i64
- `expr/lex` — lexer error
- `expr/mixed_class` — Dec mixed with Int
- `expr/mode_invalid` — bad rounding mode or unit
- `expr/parse` — grammar mismatch
- `expr/round_widens` — round target scale ≥ operand
- `expr/scale_cap` — Dec×Dec scale sum > 12
- `expr/scale_narrow` — dec would drop scale
- `expr/scale_not_literal` — scale is not a literal
- `expr/state_out_of_scope` — in(state) outside an invariant
- `expr/too_deep` — nesting beyond 32
- `expr/too_long` — source or AST too large
- `expr/type_mismatch` — operand class mismatch
- `expr/unknown_builtin` — unknown call
- `expr/unknown_enum` — unknown enum in expression
- `expr/unknown_field` — unknown evt field
- `expr/unknown_state` — in(state) names an undeclared state
- `expr/unknown_var` — unknown ctx name
- `expr/unknown_variant` — unknown enum variant
- `internal/budget` — evaluation budget exhausted
- `internal/unimplemented` — stub
- `io/read` — read failed
- `io/write` — write failed
- `req/args_invalid` — tool/CLI arguments invalid
- `req/cancelled` — the client cancelled the request
- `req/elicit_failed` — an elicitation the client refused or cancelled
- `req/elicit_nested` — an elicitation while one is outstanding
- `req/elicit_timeout` — an elicitation nobody answered in time
- `req/elicit_unsupported` — an ask nobody can answer
- `req/event_internal` — an internal or generated event sent from outside
- `req/event_unknown` — undeclared event
- `req/field_missing` — declared field absent
- `req/field_scale` — too many fraction digits
- `req/field_type` — value does not match type
- `req/field_unknown` — extra field
- `req/instance_not_found` — unknown instance
- `req/invoke_slot_state` — an invocation slot is not pending
- `req/signal_target` — a signal addressed to its own sender
- `req/machine_ambiguous` — bare name matches several versions
- `req/machine_exists` — define refused because the spec exists
- `req/machine_not_found` — unknown machine
- `req/migrate_not_superseded` — the target definition does not supersede the instance's current one
- `req/migrate_settled` — migrating an instance that has settled
- `req/migrate_slot` — the instance holds an invocation slot the migration cannot carry
- `req/migrate_unmapped` — the instance's active state has no mapping entry
- `req/number_token` — raw JSON number where a string is required
- `req/payload_too_large` — journalled payload exceeds 64 KiB
- `req/request_id_conflict` — request_id reused for different content
- `req/seq_mismatch` — stale expect_seq
- `run/action_error` — block evaluation failed
- `run/configuration_invalid` — configuration, lifecycle, history, or deadline
  schedules are incoherent for the machine
- `run/create_failed` — creation failed
- `run/div_zero` — division by zero
- `run/guard_error` — guard evaluation failed
- `run/instance_cancelled` — event or deadline poll against a cancelled instance
- `run/instance_completed` — event or deadline poll against a completed instance
- `run/invoke_create_failed` — creating an invoked child failed; nothing was journaled
- `run/invariant` — enforce invariant failed
- `run/microstep_limit` — a macrostep did not quiesce within 64 reactions
- `run/not_enabled` — all guards false
- `run/overflow` — checked arithmetic overflow
- `run/unhandled` — no candidate on the chain
- `store/chain_broken` — interior hash/seq break
- `store/degraded` — a store-backed call on a server that could not open its store
- `store/lock` — lock I/O
- `store/non_canonical` — non-canonical journal line
- `store/state_hash_mismatch` — fold disagreed
- `store/torn_tail` — truncated final record
- `store/version_mismatch` — data directory VERSION is not 9 and cannot be migrated

## Appendix B — Limits

| Limit | Value |
|---|---|
| definition size | 256 KiB (`MAX_DEF_BYTES`) |
| journalled payload | 64 KiB (`MAX_PAYLOAD_BYTES`) |
| persistence read unit | 16 MiB (`JsonLimits::DEFAULT.max_bytes`); one VERSION file, journal record excluding LF, or snapshot cache |
| nesting depth | 12 (`MAX_NESTING`) |
| eval budget | 4096 ticks per microstep (`MAX_EVAL_TICKS`); a create, event, or deadline poll runs a macrostep of at most the trigger, 64 reactions, and one closing quiescence scan, so its budget is 4096 × 66 = 270336 ticks (`MACROSTEP_EVAL_TICKS`); an enabled-event scan keeps the 4096-tick budget |
| definition eval cost | ≤ 4096 compiled AST nodes plus one per distinct event with an omitted `if` (`MAX_EVAL_TICKS`) |
| reactions per macrostep | 64 (`MAX_MICROSTEPS`): applied eventless transitions, applied internal-event transitions, and discarded internal events all count; the 65th is refused as `run/microstep_limit`. Deliberately not in the genesis `limits` block, which is hash-verified on fold |
| states | 256 |
| events | 128 |
| transitions | 2048 |
| context variables | 64 |
| history nodes | 32 |
| parallel regions | 8 (`MAX_REGIONS`) |
| deadlines | 128 (`MAX_DEADLINES`) |
| invariants | 64 |
| enums | 32 (`MAX_ENUMS`) |
| variants per enum | 64 (`MAX_VARIANTS`) |
| transitions per (state, event) | 32 (`MAX_TRANSITIONS_PER_CELL`) |
| fields per event or effect | 32 (`MAX_FIELDS`) |
| sets per block | 32 (`MAX_SETS_PER_BLOCK`) |
| emits per block | 8 (`MAX_EMITS_PER_BLOCK`) |
| raises per block | 8 (`MAX_RAISES_PER_BLOCK`); deliberately not in the genesis `limits` block, which is hash-verified on fold |
| invoke slots per state | 4 (`MAX_INVOKES_PER_STATE`); deliberately not in the genesis `limits` block, which is hash-verified on fold |
| invocation depth | 4 machines (`MAX_INVOKE_DEPTH`); deliberately not in the genesis `limits` block |
| signals per block | 4 (`MAX_SIGNALS_PER_BLOCK`); deliberately not in the genesis `limits` block |

These match `crates/fsm-core/src/limits.rs`.

## Appendix C — Format versions

| Tag | Role |
|---|---|
| `fsm.machine/1` | Machine definition documents |
| `fsm.journal/1` | Journal record envelopes |
| `fsm.snapshot/5` | Disposable snapshot caches optionally accelerated by a hash-chained `state_root` |
| `fsm.snapshot/1` through `fsm.snapshot/3` | Skipped, never reinterpreted; the journal is folded instead |
| `fsm.state/3` | Current instance state identity hash payload |
| `fsm.state/2` | Instance state identity before composition; verified where a record declares it |
| `fsm.state/1` | Historical single-leaf state identity hash payload |
| `fsm.state-root/3` | Current complete logical store root payload |
| `fsm.state-root/2` | Historical single-leaf logical store root payload |
| `fsm.base/1` | Authoritative base state a sealed store opens from. Required, never a cache: a missing snapshot degrades to a fold, a missing base refuses the open |
| `fsm.base-dedup/1` | Payload of the request-fingerprint root a seal commits over the dedup entries its base carries |
| `fsm.archive/1` | Manifest of a detached archive: per-segment plain SHA-256 digests and the sealed chain endpoints |
| `expr/1` | Expression grammar |

On-disk store `VERSION` is `9`. A `VERSION` `1` through `8` directory, or a journal with no `VERSION` marker, is best-effort migrated on open (or by a successful repair) by folding the complete journal with snapshot caches ignored, then stamping `VERSION` `8`; records, machine ids, and snapshot caches are never rewritten or reinterpreted. Any other `VERSION` is `store/version_mismatch`, refused and never reinterpreted.

Because records are never rewritten, a migrated store keeps whatever its records already carried: a `request_id` claimed before `VERSION` `7` has no `request_fp`, so it can be replayed but not conflict-checked. Records written after the migration are fully checked.

Hash domains are versioned independently of these tags: `fsm:machine:1`,
`fsm:record:1`, `fsm:state:2`, `fsm:state-root:3`, `fsm:snapshot:4`,
`fsm:request-fp:1`, `fsm:base-dedup:1`, and `fsm:archive:1`. The last two are
**additive**: `fsm:state-root:3` deliberately excludes request fingerprints
because the record body that claimed each key already authenticates its
fingerprint through the chain, and sealing is exactly the operation that
removes that record from the live chain — so a seal commits the carried
fingerprints under a domain of their own rather than under a fourth version of
the state root, and no historical root moves. Replay retains `fsm:state:1` and `fsm:state-root:2` only to
verify historical journal bytes; snapshot domains 1 through 3 are never
reinterpreted.
