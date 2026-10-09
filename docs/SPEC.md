# fsm — normative specification

Public executor ticks MUST continue bounded owned transport observation when journal scanning fails, without collecting outcomes for settlement, opening a writer, or releasing retained execution state; this does not supply missing timeout or native-host reconciliation.

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
| `fsm.state-root/5` | Current complete logical store roots, including execution admission and ownership |
| `fsm.state-root/3` | Historical complete store roots; verified where historical records declare them |
| `fsm.snapshot/7` | Current disposable snapshot caches, including execution state |
| `fsm.snapshot/5` | Historical disposable snapshot caches |
| `fsm.base/3` | Current authoritative sealed base, including execution state |
| `fsm.base/1` | Historical authoritative sealed base; opens with quarantined execution admission |
| `fsm.base-dedup/1` | Request-fingerprint root a seal commits over the keys its base carries |
| `fsm.base-index/1` | Record-derived index root over tags, parent links and sequences |
| `fsm.base-execution-claims/1` | Separate sealed-base commitment to original execution-claim hashes |
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
| `genesis` | `format`, `created_ts`, `limits`, and `execution_admission: "enabled"` for new VERSION 12 stores |
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
| `execution_claimed` | `run_id`, `instance_id`, `effect_id`, `attempt`, `handler_fingerprint`, `retry`, `domain`, `request_id`, `request_fp`; exclusive pending-effect ownership |
| `execution_stopped` | `run_id`, `instance_id`, `effect_id`, `handler_fingerprint`, `closure`, `outcome`, `request_id`, `request_fp`; verified closure retains ownership |
| `execution_settled` | `run_id`, `instance_id`, `effect_id`, `disposition`, `request_id`, `request_fp`, `state_hash`, `state_format`, plus matching ack/attempt outcome fields; atomic single consumption |
| `execution_enabled` | `previous_head`, `quiescence`, `request_id`, `request_fp`; verified legacy admission |
| `state_checkpoint` | `state_root`, `state_root_format` |
| `journal_sealed` | `sealed_through_seq`, `sealed_last_hash`, `base_state_root`, `state_root_format`, `base_dedup_fp_root`, `base_dedup_format`, `base_index_root`, `base_index_format`, `archive_id`, `records_sealed`. Marks a sealed and detached prefix. It claims no `request_id` and changes **no** logical state: the loader reads it before folding, and the fold applies it as a marker exactly as it applies `state_checkpoint`. It is appended at `sealed_through_seq + 1`, so `sealed_last_hash` MUST equal `sha256:` followed by the record's own `prev` — the body asserts a join the chain already made, and a record where the two disagree is corrupt. `state_root_format` names the format of `base_state_root`, which is the root of the state the base file materializes at `sealed_through_seq`, **after** the dropped dedup entries were removed; it is NEVER equal to the `state_root` a record on the same sequence would carry, and a reader MUST NOT assert them equal |

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

On-disk store `VERSION` is `12`. Opening a `VERSION` `1` through `11` directory,
or a journal with no `VERSION` marker, MUST attempt a best-effort migration:
ignore snapshot caches entirely, fold the complete journal using each record's
format discriminator, and on success stamp `VERSION` `12`. Interior journal
records MUST NOT be rewritten. If classify is not `Ok` (including a migratable
marker whose journal is missing) or fold fails, refuse with that health and
leave `VERSION` unchanged — a migratable directory is never re-created over. A
successful `repair --truncate-torn-tail` on a migratable store folds the
complete retained journal and likewise stamps `VERSION` `12`. Any unsupported
`VERSION` value is `store/version_mismatch`, refused and never silently
reinterpreted.

Historical genesis without execution admission MUST remain quarantined after
migration, as specified in the claim-era persistence contract below; stamping
the current version does not establish legacy executor quiescence.

`Store::open_read_only` and CLI inspection MUST NOT create directories, take
the advisory writer lock, stamp or migrate `VERSION`, or write snapshots.
`Store::open_read_only` returns one self-consistent journal prefix even if a
live writer appends after that prefix is read. It omits an unterminated line
only at the end of the lexically final segment as an in-progress append;
strict `load_records`, writer open, classification, and verification report
that same line as `TornTail`, and an unterminated non-final line is interior
corruption. Mutating methods on a read-only Store refuse with `io/write`.

Mutating store, initialization, migration and repair paths MUST hold the
exclusive advisory writer lock throughout their persistence writes. Releasing
the owning guard MUST attempt an explicit unlock before closing its file
handle: transient duplicated references, including those created during
process spawning before exec, do not constitute another writer lease. An
unlock error remains best-effort during destruction; closing the handle is
the fallback, and subsequent writers still MUST acquire the lock normally.

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
| `BaseMissing` | refuse as `store/base_missing`; the journal starts above sequence zero and nothing explains why. No repair reconstructs the missing records; restore the journal, or restore the `BASE` the seal that removed them wrote |
| `BaseMismatch` | refuse as `store/base_mismatch`; interior; **no repair** — the records the base replaced are in the archive, not in this directory. Restore the `BASE` this store was sealed with |

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

- `case/limit_bytes` — a case file exceeds the document ceiling
- `case/limit_cases` — a case file declares more cases than the ceiling allows
- `case/limit_steps` — one case's script exceeds the step ceiling
- `case/shape` — wrong JSON type, missing required field, or a script step that is not exactly one of send, poll, ack
- `case/unknown_key` — unknown key at a given JSON-Pointer path in a case file
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
- `req/instance_exists` — a `create` named an instance id that already exists. Creating NEVER replaces an instance: a retry of the original creation MUST reuse its original `request_id`, which replays the outcome
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
- `store/archive_refused` — a proposed journal seal cannot be taken. It is a **size** limit, not a rule against sealing a store with work in flight: either the idempotency keys the cut must carry do not fit a base state file, or the cut is above the lowest sequence a live derivation still depends on. The hint names what clears it
- `store/base_missing` — the journal starts above sequence zero and no base state file explains why. Records were removed from the data directory without a seal saying so; this is NEVER reported for a sealed store
- `store/base_mismatch` — the base state a sealed store opens from does not match the seal record that commits it, or does not match its own declared roots. There is no repair: the records the base replaced are in the archive, not in this data directory
- `store/chain_broken` — interior hash/seq break
- `store/degraded` — a store-backed call on a server that could not open its store
- `store/execution_contract` — the immutable handler contract or policy differs
- `store/execution_disposition` — the proved-stopped result cannot take the requested disposition
- `store/execution_evidence` — native evidence is untrusted, mismatched, malformed or unsupported
- `store/execution_exhausted` — the store-local run counter cannot allocate another identity
- `store/execution_limit` — execution metadata, result, entry or aggregate byte bounds are exceeded
- `store/execution_owned` — an unresolved or stopped run still owns the effect
- `store/execution_quarantined` — execution admission needs trusted legacy environment closure
- `store/execution_retry` — the durable retry class, count or deadline refuses another launch
- `store/execution_stale` — the run or immutable identity does not match current ownership
- `store/lock` — lock I/O
- `store/non_canonical` — non-canonical journal line
- `store/state_hash_mismatch` — fold disagreed
- `store/sealed_replay_unavailable` — a claimed `request_id` whose claiming record the store has sealed into its archive. The request was applied and is NOT applied again: the store refuses rather than reproduce a thinner outcome or, worse, treat the key as unclaimed
- `store/torn_tail` — truncated final record
- `store/version_mismatch` — data directory VERSION is unsupported and cannot be migrated

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
| `fsm.snapshot/7` | Current disposable snapshot caches including bounded execution ownership and acknowledgement handoffs |
| `fsm.snapshot/6` | Historical disposable claim-era caches; skipped, never reinterpreted |
| `fsm.snapshot/5` | Historical disposable caches; skipped by the claim-era reader |
| `fsm.snapshot/1` through `fsm.snapshot/3` | Skipped, never reinterpreted; the journal is folded instead |
| `fsm.state/3` | Current instance state identity hash payload |
| `fsm.state/2` | Instance state identity before composition; verified where a record declares it |
| `fsm.state/1` | Historical single-leaf state identity hash payload |
| `fsm.state-root/5` | Current complete logical store root payload including execution ownership and acknowledgement handoffs |
| `fsm.state-root/4` | Historical claim-era root payload, verified under its original domain |
| `fsm.state-root/3` | Historical complete logical store root payload, verified under its original domain |
| `fsm.state-root/2` | Historical single-leaf logical store root payload |
| `fsm.base/3` | Current authoritative sealed base, including ownership, original unresolved claim hashes and acknowledgement handoffs; required, never a cache |
| `fsm.base/2` | Historical authoritative claim-era base; validated under its original root/4 domain |
| `fsm.base/1` | Historical authoritative sealed base; retained under its original root format. A missing base refuses the open |
| `fsm.base-dedup/1` | Payload of the request-fingerprint root a seal commits over the dedup entries its base carries |
| `fsm.base-index/1` | Payload of the root a seal commits over the record-derived indexes its base carries: per-instance tags, parent slot, creation sequence and last sequence, and each machine's first definition sequence |
| `fsm.base-execution-claims/1` | Payload of the original unresolved claim-record hash root committed by a claim-era seal |
| `fsm.archive/1` | Manifest of a detached archive: per-segment plain SHA-256 digests and the sealed chain endpoints |
| `expr/1` | Expression grammar |

The following sealed-store rules describe the formats introduced with
historical store `VERSION` `10`, including base/1 and state-root/3. Their
historical bytes and hash domains remain authoritative when reading those
formats. Current writers use VERSION 12 and the post-ack base/3 and
state-root/5 contract below; current migration folds supported VERSION 1–11
prefixes and stamps 12. The historical `9`-to-`10` step converted nothing:
a pre-`10` store had no seal record and no base state file.

**A `VERSION` `10` store is not readable by 0.2.x, sealed or not.** The version stamp moves on first write regardless of whether anything was ever archived, so an unsealed 0.3.0 store is refused by an older build exactly as a sealed one is.

### Sealed stores

An operator MAY seal a prefix of the journal into a detached archive. Sealing NEVER rewrites, reorders, or removes a record from the live chain: it appends a `journal_sealed` record, relocates the sealed segments unchanged, and leaves a `fsm.base/1` state file behind.

* The cut MUST be the last record of a segment. A segment the cut fell inside could only be archived by splitting it, which would rewrite published bytes.
* The base state file is **required**, never a cache. A missing or stale snapshot degrades to a fold; a missing base refuses the open with `store/base_missing`, and one that disagrees with the seal that commits it refuses with `store/base_mismatch`. Neither is repairable from the data directory alone.
* A verification that did not read the sealed bytes MUST NOT report what a complete walk reports. A sealed store whose archive was not presented reports its own verdict, distinct from both success and failure.
* A cut MUST NOT archive a record a live derivation still needs. A pending effect's emitting record, its instance's creation record, and every one of its attempt records are all read back by the executor, so a cut at or above the lowest of them is refused with `store/archive_refused`.
* A seal carries, for every instance and machine its base holds, the facts a reader derives from records rather than from state: an instance's tags, the parent instance and slot that invoked it, the sequence it was created at and the highest sequence at or below the cut that touched it, and each machine's first definition sequence. These are committed under `base_index_root`. Without them a live instance created below the cut reports untagged, parentless, and dated zero — and none of those reads as a gap.
* A seal carries every idempotency key claimed above the cut, and every key whose claiming record names an instance that is live in the base state. It drops the rest, each of which is independently unreplayable: an operation against a settled instance is refused by its terminal status, a `create` naming an existing instance is refused with `req/instance_exists`, and a machine definition is idempotent by content hash.

Because records are never rewritten, a migrated store keeps whatever its records already carried: a `request_id` claimed before `VERSION` `7` has no `request_fp`, so it can be replayed but not conflict-checked. Records written after the migration are fully checked.

Hash domains are versioned independently of these tags: `fsm:machine:1`,
`fsm:record:1`, `fsm:state:2`, `fsm:state-root:3`, `fsm:snapshot:4`,
`fsm:request-fp:1`, `fsm:base-dedup:1`, `fsm:base-index:1`, and `fsm:archive:1`.
The base domains are
**additive**: `fsm:state-root:3` deliberately excludes request fingerprints
because the record body that claimed each key already authenticates its
fingerprint through the chain, and sealing is exactly the operation that
removes that record from the live chain — so a seal commits the carried
fingerprints under a domain of their own rather than under a fourth version of
the state root, and no historical root moves. `fsm:base-index:1` exists for the
same reason and by the same rule: the tags, parent links, and sequences a seal
carries are read off records rather than from state, `fsm:state-root:3` covers
none of them, and folding them in would move every historical root. Replay retains `fsm:state:1` and `fsm:state-root:2` only to
verify historical journal bytes; snapshot domains 1 through 3 are never
reinterpreted.

## Structural executor effect reports

The pure executor effect-analysis API accepts a compiled root, a catalogue
keyed by child machine digest, a parsed operator table and explicit limits.
It MUST perform no I/O, read no clock, and change no core or persisted data.
Its closed `fsm.executor-check/1` envelope contains exactly `format`, `status`,
`machine_id`, `contract_id`, `definitions`, `scope`, `findings`, `effects` and
`progress`. Status is `invalid` for known contradictions, otherwise `unknown`
for missing evidence, otherwise `compatible` within the reported scope.
Machine identity is null when compilation failed; contract identity is null
when no authoritative table is available.

`scope` contains exactly `effects_checked`, `outcomes_checked`, and
`dynamic_signals`. Effect analysis MUST inspect entry/exit blocks at every
nested state and region, all transitions and all deadlines, without pruning
guarded or apparently unreachable emits. It MUST use compiled expression
slots for argument types, not effect declarations or expression text. Static
invocations MUST be followed once per identity; unavailable or mismatched
catalogue entries are unknown. Signals MUST be marked as runtime boundaries,
not invented external handlers. Compatible MUST NOT imply termination or
successful external work. Configured outcomes whose validation is not in the
reported scope MUST remain unknown, not implicitly compatible.

Each effect site contains exactly `machine_id`, `path`, `effect`, `arguments`,
`disposition`, `required_args`, and `outcomes`. Arguments map names to inferred
type strings or null. Disposition is `automatic`, `manual`, or `missing`;
required arguments are sorted names. Outcomes map `on_ok` and `on_failed` to
`no-outcome` or the respective compatibility observation. Manual sites have
no automatic outcomes. Findings contain exactly `code`, `severity`,
`machine_id`, `path`, `effect`, `outcome`, `message`, `hint`, and `cause`;
absent effect, outcome and cause are null. Severity is `error`, `unknown`, or
`info`. Definitions and progress observations are unique sorted strings;
sites sort by definition, path and effect; findings sort by definition, path,
effect, outcome and code. Source paths MUST retain document indices.

Public `contract_id` is SHA-256 of the bytes
`fsm:executor-contract:1` followed by one NUL byte and canonical sanitized
table metadata. Metadata contains `handlers`, `manual_effects`, `max_inflight`
and `max_inflight_per_instance`; each handler contains `effect`, `kind`,
`required_args`, `timeout_ms`, `on_ok`, `on_failed` and `retry`, matching the
already-public discovery fields. Private argv, executable paths and fixed
MCP literals MUST NOT enter this identity or any report. This identity MUST
NOT substitute for the private table identity required by execution admission.

The default ceilings are 32 visited definitions, 4,096 emitted sites, 4,096
findings and 1,048,576 canonical report bytes. Caller-supplied limits MAY be
smaller. Each distinct definition, site and finding MUST be charged before
retention; the exact final canonical envelope size counts toward the byte
limit. Exceeding any ceiling MUST return `exec/contract_limit`, never a
truncated compatible result. Executor finding codes remain in the executor's
registry and operator guide, not the engine registry.

## Structural executor outcome checks

The complete pure contract analyzer MUST validate each configured `on_ok`
and `on_failed` of actually emitted automatic effects against that site's
own compiled definition. It MUST reuse core `validate_event` for concrete
payloads and externally sendable names, retaining its typed rejection as
`cause` with exactly `code`, `message`, and `hint`. Event-name rejection uses
`exec/contract_outcome_event`; payload rejection uses
`exec/contract_outcome_payload`. Missing outcomes remain `no-outcome`.
Unused shared handlers MUST NOT constrain the definition.

Stamping MUST preserve supplied fields and fill only absent fields with the
same symbolic canonical signed-i64 millisecond string. The complete family
is compatible with `str`, `int`, `ts`, `dur`, and every accepted decimal
scale: even i64 endpoints multiplied by 10^12 fit the decimal mantissa cap.
It is incompatible with `bool`. An enum that contains no canonical i64
string is incompatible; an enum containing some such strings is unknown,
not compatible with the whole family. Unknown stamp evidence MUST NOT hide
other known payload errors. Unknown stamp names become extra fields under
ordinary event validation. Duplicate stamp names retain first-fill behavior.
Static payload braces MUST remain literal. Analysis MUST read no clock.

Complete outcome checking MUST set `outcomes_checked` true, replace only
scope-related outcome unknowns from effect-only analysis, and aggregate
known invalid before unknown before compatible. Declared outcome events with
no matching transition add `no-transition` progress; guards and reachability
remain runtime-dependent, never structural contradictions. Final findings,
sites and bytes MUST satisfy the same inclusive limits and deterministic
ordering as effect analysis. Compatibility MUST NOT authorize a spawn.

Executor pending-effect reconstruction MUST retain the emitting definition's
machine identity from the verified prefix preceding its emitting record:
creation uses the recorded machine, invocation uses the recorded child machine,
and events and deadlines use the instance's definition in that prefix.
Migration MUST NOT replace that identity with the current receiving definition.
This reconstructed context MUST NOT enter journal bytes, hashes or replay state;
it supplies evidence for separately integrated runtime contract admission and
MUST NOT itself authorize an external start.

An executor pending-contract check MUST revalidate running instance membership,
the historical emitting definition and the concrete reconstructed effect,
then analyze the current receiving definition's complete static invocation
closure against the full loaded handler table. A pending effect's configured
outcomes MUST also be checked against its current receiver even if that
receiver no longer contains its original emit site. Known incompatibility
uses `exec/contract_invalid`; unavailable or stale evidence uses
`exec/contract_unknown` or `exec/contract_definition_unknown`. Refusal MUST
leave the journal, pending work, retry ledgers and request-id inventory unchanged.
This read-only check alone MUST NOT authorize a physical start: the shared
service still needs final writer/generation validation and lifecycle control.

## CLI executor machine checks

`fsm execute --check --handlers <file>` MAY select exactly one of
`--machine-file <file>` and `--machine <name-or-id>`. Selectors MUST require
`--check` and MUST NOT combine with execution/dead-letter options. Both
machine input and handler input MUST NOT consume the same stdin stream.
The file selector MUST compile offline without inspecting the data directory;
unresolved child definitions remain unknown. The stored selector MUST use
an existing strictly read-only store, including while its writer is held.
Neither check MAY create a directory, lock, snapshot, request, definition,
instance or external process.

Machine checks MUST emit the common `fsm.executor-check/1` report directly;
exit codes are 0 compatible, 1 invalid, 2 input/usage/store/analysis failure,
and 3 unknown. Handler parsing failures in machine-check mode MUST preserve
the typed code with sanitized diagnostic text, without private table literals.
Legacy table-only checks retain their content and exit semantics, adding
`scope: "handler-table-only"`; their privileged argv inspection MUST NOT be
included in machine reports. `--list-dead` retains its existing behavior.

## Explicit manual executor effects

The operator-owned `fsm.handlers/1` table MAY contain `manual_effects`, an
array of at most 256 nonempty effect names. Omission MUST mean an empty set.
Duplicate names, nonstring names, and names also present in automatic
`handlers` MUST be rejected with `exec/config`. A table with no automatic
handlers MUST be accepted when it declares at least one manual effect; a table
with neither disposition MUST be rejected. Manual classification MUST NOT
create a process, acknowledge an effect, or send an outcome event. It does not
alter core compilation, machine hashes or journal bytes. It supplies explicit
operator policy for structural executor checking; runtime contract admission
is a separately integrated capability, not a consequence of parsing this field.

## MCP executor discovery

`resources/list` MUST include `fsm://docs/embedding` and `fsm://executor`.
The latter reads an `application/json` document with format `fsm.executor/1`:
`mode` (`writer`, `embedded`, `read-only`, or `degraded`), `executes_effects`
(boolean), `external_executor` (`unknown`; other processes are not inspected), `progress` (`manual`,
`client_requests`, `external`, or `unavailable`), and `handlers`.

For a writable embedded session, `handlers` MUST describe the table loaded
by that session: effect, kind, sorted unique required argument names,
`timeout_ms`, retry policy, and nullable `on_ok` / `on_failed` contracts
(event, static payload, and stamps). Command lines and MCP argument literal
values MUST NOT be exposed by this resource. Required arguments MUST use
the executor's substitution rules, including nested MCP string values.
Outcome payload literals are part of the exposed domain event contract.
Process output and MCP results MUST NOT be implicitly converted to events.

`execution_ownership` MUST be null when the store is unavailable; otherwise
it MUST report `enabled`, `unresolved_runs`, `stopped_runs`, and
`outstanding_handoffs` from the verified observed journal prefix. Counts
MUST reveal no run identifiers, native paths, commands, or results and MUST
NOT be presented as live process observations or proof of closure. A
read-only prefix MAY report these durable counts without taking the writer.

A plain writer reports an empty handler list. A read-only or degraded session
reports a null handler list and unknown external executor status; holding a
read-only store MUST NOT be presented as proof of a running executor.
A read-only session MUST NOT start effect handlers, including when an embedded
startup loses writer contention. Such fallback sessions MUST refresh the
read-only journal prefix before requests. Embedded execution requires client
requests to drive ticks, including recovery; a subscription alone is insufficient.

The executor's recovery window for acknowledged effects MUST be applied after
excluding acknowledgements with no event for their actual outcome, so unrelated acknowledgements cannot
hide an interrupted advance. This changes no record format or state hash.

## Claim-era persistence contract

On-disk store `VERSION` is `11`. The pure fold, persistence codecs and production
store mutators implement the claim-era representations specified here, accepted
under plan 0022 task 9302; native execution integration remains under
development in the downstream tasks.
Format support alone MUST NOT be advertised as contained execution support.

The separately provisioned Linux authority under development uses protected
`/var/lib/fsm-containment/<namespace>/authority-<generation>` directories.
Namespace and generation MUST be canonical domain identities; directories and
their ancestors MUST be root owned, nonsymlink directories without group or
other write permission. An explicitly registered authority generation MUST
be created exclusively and MUST NOT silently replace an existing generation
or recreate missing authority during binding. Its canonical private
`store.json` (`fsm.native-store-registration/1`) binds the canonical store
path and native device/inode identity. This registration does not enable
execution or establish legacy quiescence.

Registration MUST initialize a protected allocation counter bound to the
actual authority directory identity, namespace, generation and current boot.
Preparation MUST refuse missing, copied, rollback or noncanonical counter
state and unknown native domains. It MUST durably create an immutable
allocation intent and advance the counter before creating the empty cgroup;
it MUST then inspect actual device/inode identity and `populated 0` before
publishing its prepared record. Incomplete intents remain unresolved and MUST
NOT be recycled or silently skipped. The initial authority generation allows
at most 4096 lifetime allocations and bounds directory inventory to 32768
entries. Unit/cgroup names include namespace, generation and allocation:
`fsm-containment-<namespace>-<generation>-<allocation>.service`.
Preparation MUST NOT launch user code or enable execution admission.
Before burning an allocation intent, preparation MUST also verify that the
system manager reports `system.slice` loaded and active with control group
`/system.slice`. The query MUST use the fixed root-protected systemctl binary,
clear inherited environment overrides, bound retained output to 4 KiB per
stream and apply a two-second observation deadline. Missing access, excessive
output, failed execution or incomplete I/O MUST refuse preparation before
counter/intent mutation. Cleanup of the query process MUST be bounded; this
capability check is not handler termination or native closure evidence.

The root-only `launch` operation MUST freshly validate the protected claim
binding and approved catalogue under the authority lock before any manager
submission. It MUST exclusively fsync a bounded cold-readable
launch intent only after refusing any preexisting entry grant or pending grant,
regardless of file type. Such refusal MUST leave intent absent and the
prepared domain empty; prearmed entry MUST NOT bypass verified startup.
The bounded cold-readable intent is `launch-<allocation>.json`, containing
exactly `format` (`fsm.native-launch-intent/1`) and `binding`; it MUST be
durable before starting the fixed root-protected systemd-run with the
installed protected gate and only its canonical route. Any existing intent,
including a partial one, MUST refuse
another submission; failed or uncertain submissions MUST retain that intent
and journal ownership. The manager command MUST clear inherited overrides,
use a dynamic unprivileged identity, protected control groups, no delegation,
no capabilities or privilege escalation, no restart, control-group lifetime
and termination, and a runtime ceiling of approved timeout plus the five-second
entry bound. Owned standard streams MUST pass through `--pipe` without spool
files or journald capture. The authority lock MUST remain held after spawning
until a shared two-second deadline verifies active/running manager handoff and
the actual installed enrolled gate. Manager queries MUST respect the remaining
deadline rather than each resetting the startup bound. Before releasing the
lock, launch MUST exclusively fsync `handoff-<allocation>.json`, containing
exactly `format` (`fsm.native-launch-handoff/1`), `binding` and `gate`; `gate`
contains exactly `pid`, `group_id` and `invocation_id` from the verified
manager/proc observations. The largest handoff envelope MUST be charged before
reserving launch intent so accepted submissions remain cold-readable.
An incomplete handoff MUST best-effort revoke entry while retaining the lock
and kill/reap its owned transport within a bounded cleanup interval; failures
retain intent and journal ownership without issuing closure evidence.
The command monitor MUST have a finite deadline and bounded best-effort cleanup;
transport/root exit MUST NOT issue closure evidence or permit settlement.
Broker authentication, manager admission fencing and permanent closure remain
required before the contained runner is accepted.

The private root authority MUST support explicit `provision-broker` for one
nonzero operator UID other than the chown sentinel `u32::MAX`, outside the reserved dynamic-handler UID interval
61184..65519, and `serve` for that provisioned authority. Provisioning MUST
exclusively create protected configuration and epoch counter. Its newly created
broker directory MUST explicitly have mode 0755, independent of the process
mask; existing authority ancestors MUST permit operator traversal or provisioning
MUST refuse without changing their permissions. Registration MUST explicitly
apply mode 0755 to its own newly created namespace/authority directories so a
restrictive mask cannot silently prevent dynamic gate or operator traversal;
preexisting namespace permissions MUST NOT be broadened. Serving MUST NOT
recreate missing authority. Provisioning MUST validate the approved catalogue.
Lifetime guard and server startup MUST validate protected configuration,
authority/boot and leadership identity independently of the current catalogue;
a missing or malformed catalogue MUST NOT prevent original-result recovery,
including after broker restart. New allocation, binding and execution MUST retain
their separate current-catalogue validation and refuse unavailable approval
before allocation or handler entry. A root-owned lifetime broker lock MUST exclude a
second server. Every server startup MUST durably burn a monotonically increasing
epoch before binding a fresh Unix socket, preserve old sockets, and refuse
counter rollback, missing epoch history or partial pending counter material.
The socket MUST be created under a mask excluding all group/other permissions,
then have mode 0600 and the configured operator owner before a root-owned,
read-only, atomically published route exposes its epoch and native identity.
The protected namespace prevents socket replacement by the operator; root and
kernel administrators remain trusted. Socket permissions authenticate operator
access without granting the handler's dynamic UID access to the control route.

A broker connection MUST carry exactly one canonical JSON request, prefixed by
a four-byte big-endian length within 1..8192, acquired within a shared 500 ms
frame deadline. Its closed shape is `format` (`fsm.native-request/1`), `action`
and `payload`. The only actions are `prepare` (null payload), `bind` (existing
claim-binding payload), and `execute`, `close`, `observe`, `recover` (positive canonical
allocation number). No request may supply a namespace, filesystem path, argv,
manager command or grant. Dispatch MUST remain bound to the provisioned
verified authority directory; claim/catalogue/native checks remain mandatory.
A canonical response is a length-prefixed closed object with exactly `format`
(`fsm.native-response/1`), boolean `ok`, and `result` (operation value or bounded
private error string); its body is at most 65536 bytes and writes share a 500 ms
deadline. Error strings MUST NOT become stable library error codes.

The server MUST retain at most eight owned connection threads, refuse excess
connections, and observe joins before releasing their capacity. Executing
connections MUST retain their runner thread and observe client EOF or unexpected
trailing bytes as a cancellation request, using the same verified cleanup path;
EOF MUST NOT prove handler closure. Before dispatch and after execution, the
server MUST revalidate its protected authority/configuration identity. Server
or connection death MUST NOT clear journal claims or fabricate receipts; host
shutdown/recovery and public client/service integration remain separately
required. Other actions may finish despite connection loss, but never launch
handler code. Broker presence alone MUST NOT release the production acceptance
gate.

The provisioned binary's private unprivileged `client NAMESPACE GENERATION`
helper MUST authenticate its current kernel UID against the configured operator
in the protected read-only public route, validate current boot, authority inode,
route shape/epoch and exact socket type, owner, mode and device/inode before
connection, and revalidate that route/socket before sending and after receiving.
It MUST read one bounded canonical request from its owned standard input and
forward only that request to the verified broker; it MUST return one bounded
canonical response frame on standard output, never a handler command or journal
mutation. An unavailable, changed or untrusted route MUST refuse before dispatch.
The helper MUST be supervised by its execution host, which owns its process and
standard streams and imposes startup, request and shutdown deadlines. Potentially
blocking Unix connect or standard I/O MUST remain in that killable helper process;
the host MUST NOT use detached unbounded connection threads or assume a portable
Unix connect timeout. Killing the helper closes its broker connection and requests
existing verified cancellation, without proving closure or releasing a claim.
The opaque `VerifiedClosure::matches_claim` read-only predicate MUST compare
receipt run ID, the complete native domain and original journal-claim hash
against the supplied immutable claim/hash. It MUST NOT mutate the journal or
assert that supplied ownership is current; the store's stopped transition still
MUST recheck current ownership under its writer lease. A matching predicate
alone MUST NOT authorize settlement, retry or capacity release.

The provisional `run::native_client::NativeCompletion` validator MUST accept
only a successful closed broker response with the exact native result envelope,
original immutable claim/hash and canonical derived receipt path for that run.
It MUST preserve the candidate acknowledgement value and accept only existing
nullable failure classes, without reinterpreting the current handler table.
It MUST read opaque protected closure evidence and require `matches_claim`
before returning a checked completion. Missing/mismatched material MUST retain
uncertainty; validation MUST NOT write stopped results, settle ownership or
release capacity. The service MUST still recheck current ownership and persist
its stopped result through the store's writer-protected mutator before settlement.

The supervised helper MUST retain an open nonblocking stdin lifetime channel
after the request frame; EOF or further input MUST terminate the helper even
during blocking connect or output, so host death triggers broker cancellation without
depending on the handler timeout, and that cancellation MUST NOT itself prove
closure or release a claim.
Its lifetime watcher MUST be owned and joined on ordinary return, use bounded
nonblocking reads, and MUST NOT create a detached connection worker.

Checked native completion MUST expose a stopped outcome preserving the exact
candidate as its result: clean process/MCP success maps to `ok`, classified
failures to their existing class, cancellation to `interrupted`, and terminal
MCP protocol failure to `failed`. Candidate/class contradictions or unknown
error/status forms MUST refuse before receipt verification; a null retry class
MUST NOT by itself imply success. This conversion MUST NOT mutate the journal.
For the final attempt of a class explicitly admitted by the immutable claim's
retry policy, the stopped result MUST preserve existing exhaustion semantics:
retain candidate fields while setting `error` to `exec/retries_exhausted` and
adding original `class` and claimed `attempts`. The raw candidate getter MUST
remain unchanged; disallowed classes and terminal unclassified failures MUST
NOT be relabelled exhausted, and no current handler table may supply this policy.
The provisional Linux pipeline `stop_native` method MUST persist only the
checked completion outcome and opaque proof through `Store::stop_execution_on`,
retaining its writer-protected current-claim/hash/domain checks and request-id
replay. It MUST NOT acknowledge, retry, settle ownership or release capacity.
The provisional Linux pipeline `claim_native` adapter MUST refuse unsupported
native architectures and otherwise delegate its complete claim request to the
writer-protected store claim mutator before any binding or launch. It MUST
retain admission, pending-effect, unresolved ownership, immutable contract and
retry/backoff checks and request replay; claiming MUST NOT itself launch code.
The provisional pipeline `settle_stopped` adapter MUST delegate the caller's
explicit disposition and immutable claim to `Store::settle_execution_on`, with
current stopped ownership and idempotent request replay checked under the
writer. It MUST consume the stopped result only through that atomic mutator,
without launching a handler or consulting a changed handler table; declared
outcome-event recovery remains a separate acknowledgement-following step.
The pure execution-state `settlement_for` selector MUST require the exact current
claim and a durable stopped result. An absent effect or stopped interruption
selects interruption; clean success and terminal `failed` select acknowledgement;
a classified failure selects attempt only when its immutable claim policy
includes that class and permits another attempt, otherwise acknowledgement.
Selection MUST NOT mutate ownership or impose a new launch before backoff;
atomic settlement and retry eligibility remain separately mandatory.

The provisional Linux `run::native_client::NativeRequest` host adapter MUST
validate a closed bounded request and canonical namespace/generation, verify the
fixed root-protected nonsymlink non-setuid helper executable, and own its process
and nonblocking stdin/stdout/stderr sockets. It MUST bound pending request bytes
to 8196, retained response bytes to 65540 plus one overflow-detection byte and
diagnostic prefix to 4096, with fixed 64 KiB per-stream work per poll. Its caller
supplies a positive checked deadline; deadline/transport failure MUST request
helper cancellation, retain the process handle and return uncertainty without
releasing any journal claim. Explicit cancellation MUST NOT prove tree closure.
A response MUST be collected only after actual helper reap, successful exit,
complete stdout/stderr EOF and one exact bounded canonical response frame with
no extra bytes; a transport response itself MUST NOT prove matching run closure.
Reap progress MUST remain explicit and retain unresolved handle ownership until
actual reap/EOF observation. Each reap poll MUST attempt both stream drains and
process observation independently, even when another observation fails; a failed
drain MUST NOT prevent observing process retirement or imply stream EOF.
Drop is bounded best-effort only. No detached
connection thread, caller-selected helper path, capture file or inherited
manager/environment override is permitted. Public service claim/closure matching
and lifecycle integration remain separately required.

The provisional Linux `NativeRun` adapter MUST own the original immutable
claim/hash and one shared checked deadline across binding and execution. It
MUST require successful binding before issuing execute, derive the route and
allocation solely from that claim, and return only a `NativeCompletion` verified
against the same claim/hash. Cancellation, bind refusal, helper failure and
missing closure MUST retain uncertainty; helper reap alone MUST NOT authorize
journal settlement or capacity release. It MUST retain explicit helper cleanup
progress and never fall back to direct handler execution.
Protected authority claim verification MUST explicitly require enabled
execution admission as well as current runnable ownership; a retained claim
in a quarantined execution state MUST NOT authorize binding, launch or entry.
Complete-close MAY publish matching closure for a protected bound allocation
that never launched only while retaining the authority lock, with exact absence
of launch intent, handoff and manager stop/retirement records, durable entry
revocation, empty manager unit/job inventory, and the original empty cgroup
removed or absent after durable revocation. Before removing a present domain,
this path MUST use the same bounded exact native observation and repeated
prepared-domain/closing/manager-retirement checks as completed-submission
closure; finding a `populated 0` line alone MUST NOT authorize removal.
A present or partial launch intent
MUST retain the completed-handoff requirement. Cold retry after cgroup removal
MUST recheck the durable revocation and all absence conditions before publishing
the ordinary immutable receipt; it MUST NOT fabricate a handoff or manager stop.
Pending submission records MUST also refuse this never-launched path; removal
of the cgroup or a closed tombstone alone MUST NOT imply receipt publication
succeeded when an incomplete receipt publication is retained.
Receipt publication replay with both final and pending paths MUST verify the
pending path is the same protected regular-file inode as the exact immutable
final receipt before synchronizing and removing that owned pending link.
A different, partial or unexpected pending file MUST remain untouched and
refuse publication replay; final receipt bytes and identity MUST remain unchanged.

Public supervised client/service wiring remains required before acceptance.

The protected entry operation MUST run as an unprivileged handler identity.
It MUST accept only a canonical namespace/generation/allocation route and
read an immutable root-owned entry grant from that authority; caller-supplied
argv MUST NOT authorize entry. The grant contains exactly `format`
(`fsm.native-entry/1`), `claim`, `journal_claim` and `argv`, within the 8 KiB
private-record limit. The gate MUST verify claim/domain routing, actual
authority and cgroup device/inode identities, current boot and its own exact
cgroup membership before replacing itself with the granted absolute command.
Root ownership protects the grant; the launcher MUST independently verify
current durable ownership before publication and revoke entry on closing.
An absent, malformed or mismatched grant MUST execute no handler code.
Before waiting for authorization, the gate MUST verify exact membership in
its canonically routed cgroup. An enrolled gate may wait at most five seconds
for the exclusively published grant, including optional exec-status association
within the same five-second entry wait, polling without retaining handler output
or executing handler code. Closing/closed markers, including malformed markers,
MUST refuse both during that wait and immediately before exec. Only absence
permits waiting; inaccessible or malformed grant files MUST refuse immediately.
This final check does not replace fencing/killing the gate during closure.

Entry authorization MUST require the already protected binding and re-read
current durable ownership under the authority lock before grant publication.
Any closing/closed marker MUST refuse authorization, including malformed
markers. The root-only authorization request contains exactly `grant` and
`group_id`; the grant's claim and original hash MUST match the immutable
binding. Group zero MUST be refused. Publication MUST create a fresh private
temporary file, assign the selected isolated group while retaining root
ownership, remove all write permissions, fsync, publish exclusively without
replacing an existing grant, and fsync its parent. This is a privileged
publisher interface; the future broker MUST derive the group from the trusted
enrolled gate and MUST NOT delegate arbitrary group selection to handlers.

The root-only `begin-close` operation MUST validate the prepared domain
against its route, authority device/inode, boot and actual cgroup device/inode
under the authority lock. It MUST durably publish `closing-<allocation>.json`,
containing exactly `format` (`fsm.native-closing/1`) and `domain`, before
removing the entry grant and any pending grant, then fsync the authority
directory. Existing closing material permits replay only if it matches
exactly. Unexpected grant file types or ownership MUST refuse removal.
Replay MUST fsync the validated marker and its parent before deleting grants;
a prior attempt may have made the marker visible without completing durability.
Runnable journal ownership is not required to revoke entry after cancellation.
This transition MUST NOT issue closure evidence, clear claims or imply
termination; manager admission fencing, native termination and permanent
closure remain required before settlement or capacity reuse.

The root-only `authorize-enrolled` operation accepts exactly `grant`, without
a caller-selected group. After fresh binding/argv validation under the
authority lock, it MUST derive the grant group from the routed system
manager's active running DynamicUser main process. Manager properties MUST
match the exact routed cgroup, nondelegation, protected control groups and
disabled restart. The process MUST be the root-protected installed
`/usr/libexec/fsm-containment-authority` executing exactly `gate` and that
native route, with matching cgroup membership, one canonical nonzero UID/GID
pair in the dynamic identity range (61184 through 65519), no additional supplementary group,
no capabilities in any observed set and `NoNewPrivs: 1`.
Manager/process/executable observations MUST be bounded and stable across
revalidation before publication; a missing, changed or malformed observation
MUST refuse without publishing a grant. Provisioning MUST reserve the dynamic
identity range from static accounts. This privileged operation does not
launch the gate, authenticate a broker peer or establish permanent closure.
The protected handoff MUST match the exact binding and freshly observed gate
PID/group/invocation. These corroborating diagnostics MUST NOT substitute for
native domain identity or establish termination/permanent closure. Before
publishing a grant, exact validated handoff replay MUST fsync the handoff and
its parent, covering a prior interrupted durability step. An absent or changed
handoff MUST refuse even when a live process appears enrolled.

The root-only `request-kill` operation MUST complete admission revocation
under the authority lock before writing `1` to the matched domain's
`cgroup.freeze` and then `cgroup.kill`. It MUST retain that lock throughout
both writes, reject symlink control files and verify actual domain identity
before and after each control-file open. Any failure preserves closing
admission and durable claim ownership. Successful submission MUST NOT imply
observed termination, permanent closure or permission to settle/reuse capacity.

The root-only `request-stop` operation MUST durably revoke entry under the
authority lock, then match the protected handoff/binding to that prepared
domain and the manager's current invocation, control group and fixed isolation,
control-group lifetime/kill and no-restart policy before stopping the unit.
It MUST use the fixed protected systemctl with replacement job semantics,
cleared environment, bounded capture and a two-second observation deadline.
Policy/identity/query failures MUST preserve closing and journal ownership.
After synchronous manager stop succeeds, the authority MUST exclusively
publish and fsync `manager-stopped-<allocation>.json`, a protected closed
object with `format: "fsm.native-manager-stopped/1"`, the exact `domain`,
`binding` and enrolled `gate` from the handoff. It MUST validate the complete
8 KiB/depth-bounded material before manager submission; an existing completion
path of any type MUST refuse replacement. Publication failure remains uncertain.
This record acknowledges the matched manager operation and MUST NOT stand in
for permanent closure or a native closure receipt.

The root-only `complete-close` operation MUST retain the authority lock and
require exact protected prepared and binding records for the same domain and
original claim. When a launch intent exists, it MUST additionally require exact
one-shot intent and completed handoff records; a partial or unreadable intent
MUST NOT select the never-launched path. Verified handoff MUST include
the fixed control-group lifetime/kill/no-restart policy in its admission checks.
It MUST durably revoke entry before observing retirement, checking any present
cgroup against the recorded native identity; a missing cgroup does not itself
authorize closure. A present manager-stopped record MUST match exactly; when
that record is absent, natural retirement MUST be acknowledged separately as
`fsm.native-manager-retired/1` with the exact domain, binding and gate, only
after the same full retirement observations succeed. Natural retirement MUST
NOT be represented as successful manager stop. Both submitted routes retain and recheck
the protected closing marker through publication.
After durable revocation and proof of manager unit/job retirement, Root MAY
remove a residual empty cgroup only when its protected directory identity still
matches the original domain and the bounded no-follow exact cgroup.events sample
reports populated=false. It MUST revalidate identity and manager retirement before
removal and reprove actual absence and manager retirement afterward; removal
failure, unknown children, population or replacement MUST retain uncertainty.
This cleanup MUST NOT recursively remove cgroups or infer closure from emptiness.
Before final publication it MUST refuse either grant path of any type, a different boot/authority,
present or unreadable native cgroup, a listed manager unit or a queued job
for that unit. Manager inventory capture MUST be bounded and use a shared
two-second deadline. The irreversible closing marker and one-shot launch
intent fence future authority admission for submitted runs; the matched accepted handoff and
manager inventories retire the accepted submission. Root/kernel administrators
remain outside this exclusion guarantee. An absent cgroup alone is insufficient.
For a never-launched bound allocation, exact absence of final and pending
launch/handoff/manager stop/retirement material, durable admission revocation
and empty manager unit/job inventories MUST instead be proved under the lock.
The original protected empty cgroup MAY then be removed; a cold retry with an
absent cgroup MUST require exact durable revocation and repeat all absence
checks. This route MUST NOT fabricate handoff or manager completion records.
After revalidating protected material and native absence, it MUST publish and
fsync the exact `fsm.native-domain-closed/1` domain tombstone, then exclusively
publish the matching `fsm.native-closure/1` receipt by writing an exclusive
private pending file, clearing all write bits and fsyncing it before an
exclusive hard link exposes the final path, then fsyncing the parent before
removing the pending link and syncing the parent again. Existing pending
material MUST NOT be replaced. Matching durable final records may be replayed;
different or partial records MUST NOT be replaced. Publication failures retain
journal ownership; this operation MUST NOT itself settle or clear a claim.
Uncertain submissions without matched completed handoff remain unresolved
and require the separate native reconciliation protocol.

The private root authority `execute` operation MUST derive argv, handler kind,
MCP tool/arguments and timeout from the protected approved catalogue and the
freshly verified claim's journal effect, never caller-supplied execution input.
It MUST use the same enrolled manager launch and grant path for process and
MCP handlers, pass owned bounded capture/protocol streams, and treat process
exit or an MCP answer only as a candidate. Before returning a result, it MUST
close admission, stop any surviving domain members, obtain the matching
file-verified closure receipt, reap its owned transport and join its MCP worker.
MCP I/O MUST have independent shutdown controls and bounded join observation.
Capture MUST share the public runner's prefix/digest accounting and drain
excess bytes within a fixed poll budget without spool files. Cleanup failures
MUST return uncertainty with journal ownership retained, never a settleable
candidate.
The private runner MUST accept an explicit in-memory cancellation control shared
with its execution host. A cancellation observed before launch MUST refuse launch
without changing the journal claim or publishing launch intent. After enrolled
handoff, cancellation observed before grant publication MUST withhold that grant, select
the existing `exec/cancelled` candidate before collecting another outcome, and
use the same revocation, descendant stop, closure proof and handle retirement
path as other candidates. A cancellation request alone MUST NOT prove closure
or release ownership; authenticated host/broker wiring remains required.
An unavailable live stop after natural exit MAY be resolved only by the
independent matching complete-close protocol; its error MUST NOT itself
authorize a result or be relabeled successful manager stop.
Its bounded response MUST bind the original claim/hash, receipt path,
candidate result and failure class. It MUST NOT write stopped results or
settle the journal itself; the production service/broker integration remains
required, and root-only operation MUST NOT imply an unprivileged client path.
The private response format is `fsm.native-run-result/3`, with exactly `format`,
`handler_kind`, `handler_contract`, `claim`, `journal_claim`, `receipt` (the exact protected receipt path),
`candidate` (existing runner acknowledgement-result shape), and nullable
`failure_class` (existing runner retry classification); canonical output is
at most 64 KiB. It is not an authenticated remote broker response by itself.
`handler_kind` MUST be `process` or `mcp` derived from the freshly verified
approved catalogue. Checked completion MUST reject a missing process status,
process use of MCP-only errors, MCP use of a process exit candidate, and unknown
kinds; generic timeout/cancellation/spawn errors retain their shared shapes.
`handler_contract` MUST be the complete canonical fsm.handler-contract/1 material
of the freshly verified catalogue handler. Checked completion MUST decode it
against the original claim fingerprint and require agreement with the original
retry snapshot and envelope kind before reading any closure receipt. Its
handler accessor MUST retain this checked original contract without consulting
a current handler table; contract values may contain secrets and MUST NOT enter
health output. Before returning a completed result, the authority MUST create the
Root-owned mode-0600 completed-ALLOCATION-RUN.json record once, containing the
complete canonical fsm.native-response/1 success envelope, then fsync the file
and its parent directory. This record has an explicit 64 KiB bound and existing
JSON limits; ordinary authority control records retain their 8 KiB bound.
The original fingerprint/retry/kind and closure receipt MUST verify before
publication. Any publication failure MUST retain unresolved journal ownership;
a missing or torn record MUST NOT manufacture an outcome or permit relaunch.
The provisioned broker's recover action accepts only a canonical positive
allocation number, derives the run from its protected original binding, checks
original authority path/inode and re-verifies completed identity, contract and
closure. It MUST return only the recorded original result, without current
catalogue lookup, launch, writer acquisition, journal mutation or record repair.
Recovery MAY return that original result after stopped/settled journaling;
current writer-held claim checks remain mandatory before applying it.
NativeRun::recover MUST request only this read-only operation, expose a distinct
Recovering phase and enforce the retained original claim/hash, global deadline,
actual helper retirement/stream EOF and completion single delivery. It MUST NOT
transition to binding or execution when recovery is missing, refused, cancelled
or uncertain. Pipeline::recover_native MUST require a supported durable,
unpoisoned store snapshot and its exact current owned claim/hash; read-only
snapshots are permitted and launch eligibility MUST NOT gate observation.
Neither recovery API writes a stopped result or consumes ownership.
Automatic host recovery MUST retain the original claim while its protected
completed response is absent, without consuming its one recovery request.
Root MUST stage, fsync and atomically publish the complete mode-0600 response
before its final name can trigger that request. Presence is only a readiness
hint: original store/authority identity, checked contract, result attestation,
closure, helper retirement and writer-held settlement checks remain mandatory.
Missing, torn or refused evidence MUST NOT trigger binding, execution, deadline
renewal or ownership clearance; a requested failed recovery remains uncertain.
NativePreparation MUST request only prepare and return one parsed NativeDomain
matching its original namespace/generation after successful helper retirement
and both stream EOF within the original deadline. Its identifier-free Preparing,
Prepared and Uncertain progress MUST NOT authenticate closure, claim ownership
or authorize handler entry. Refusal/cancellation/deadline failure MUST remain
uncertain and never select a launch fallback; preparation may have allocated an
empty domain even when its result was not collected. A returned domain MUST still
be durably claimed under a healthy writer before original-claim binding/launch.
Store::replay_execution_settlement MUST perform a non-writing lookup under the
exact original execution_settled fingerprint of the supplied full claim and
disposition. It MUST NOT claim an unused key, acquire a writer or consume
ownership; read-only handles are permitted. A claimed key without its original
fingerprint MUST refuse as store/execution_evidence, mismatched fingerprints
retain req/request_id_conflict, and carried sealed outcomes retain the existing
store/sealed_outcome_unavailable refusal rather than reconstruct guessed
settlement. An unclaimed key returns no response and grants no advance.
Pipeline::advance_native_settled MUST require a supported healthy durable
writer, a completion verified for the exact supplied original claim and a
successful exact Acked settlement replay. Before replay or event application it
MUST recheck the completion proof against the current physical store directory;
a copied or replaced directory MUST refuse without journal mutation.
Replayed disposition, instance,
effect, run, outcome and result MUST agree with the checked original completion
before any event. Missing evidence MUST refuse as exec/inflight_deferred;
conflicting or unavailable ledger evidence retains the wrapped store refusal.
The advance MUST come from that original contract's on_ok/on_failed, never a
current table, and MUST preserve existing event_rid, engine enablement and
acknowledgement-before-event/sequence retry behavior. No advance is sent for
unsettled, attempted, interrupted or substituted completion evidence.
Pipeline::settle_native_stopped MUST require a supported healthy durable writer,
exact retained claim/hash, matching closure and a previously persisted stopped
outcome equal to the checked completion. It MUST select disposition from the
original claim policy and current pending/lifecycle state, without consulting a
current handler table, then atomically consume that stopped result through the
existing store settlement mutator. Acked MUST use existing ack_rid and Attempted
MUST use existing attempt_rid; Interrupted MUST use exec-interrupted-EFFECT-RUN
so a pending interruption cannot consume a later acknowledgement/attempt key.
Missing stopped evidence MUST refuse as exec/inflight_deferred. This API sends
no outcome event and requires retained stopped ownership; recovery of a committed
transaction MUST use exact settlement replay instead of reselecting disposition.
This protected record durably retains the original contract for broker recovery
but does not complete restarted service stop/settlement/outcome-event recovery. Previous private result envelope
versions MUST refuse rather than infer absent kind or contract material.
The provisional Linux-only `run::native_io::NativeCapture` and
`NativeProtocol` adapters MUST reuse the runner's capture/worker implementation;
their constructors MUST NOT authorize launch, validate a claim or prove
closure. NativeProtocol collection MUST require observed worker join, while
cancellation and failed join MUST NOT establish native termination.
For process handlers, a still-running manager transport MUST NOT hide root
exit behind surviving descendants. The authority MUST periodically observe
the manager's `InvocationID`, `ExecMainPID`, `ExecMainCode` and
`ExecMainStatus` within the remaining handler deadline. Invocation and original
PID MUST match the protected handoff; canonical normal exit status is 0..255,
while killed/dumped roots project existing signal status `-1`. Unknown or
mismatched inspection MUST retain uncertainty. Launch MUST retain the original manager status with `RemainAfterExit=yes` and
`CollectMode=inactive`, without `--collect`, until matched Root stop. Stop MUST
durably revoke entry for a present verified original cgroup even when damaged
handoff material prevents manager stop or closure. If matched manager stop
refuses, both runner cleanup and authenticated broker close MUST attempt
original-identity kernel freeze/kill independently
of writer access; that attempt MUST NOT establish closure or release ownership.
An already absent cgroup MUST require the verified original completed handoff and
unchanged manager invocation/security policy before revocation; a present
replacement identity MUST refuse. A failed unit MAY be reset
only after stop and exact original invocation and ExecMainPID matching. Reset
MUST NOT establish closure: actual cgroup absence, unit unloading, absence of
queued jobs and owned helper retirement remain mandatory.
The native runner MUST establish a separate one-shot exec-status channel before
launch; handler stdout, stderr and exit codes MUST NOT authenticate spawn failure.
Root MUST create `exec-<allocation>/s` inside an initially Root-only 0700 directory
and publish a bounded private 0600 `exec-status-<allocation>.json` record with
exactly `format` (`fsm.native-exec-status/1`), original `binding`, `directory` and
`socket` identities, charging the largest 8 KiB envelope before path creation.
Before launch, Root MUST obtain 32 unpredictable bytes from the ready kernel
random device and deliver a fixed 41-byte magic/input-kind/nonce challenge solely
through the inherited manager stdin. An exec-status hello MUST be exactly 44
bytes (magic, original PID and nonce); error frames MUST be exactly 12 bytes;
error-frame inspection MUST read at most 13 bytes to detect excess and descriptor metadata at most 4097 bytes
against a 4096-byte limit. Association MUST share a two-second deadline and
MUST check it before every accept and hello-read retry, including interrupted
I/O; expiry MUST refuse without entry grant or ownership release. Before opening traversal/socket
access to the actual enrolled dynamic group, Root MUST match the protected handoff and verify the installed
gate, original manager invocation and sole original cgroup process. Only that
reserved group may connect; provisioning MUST reserve this identity range from
static accounts and supplementary membership, and the pre-grant UID/GID MUST
NOT substitute for authentication. The installed gate MUST be an ordinary Root
0711 executable, unreadable to its unprivileged identity; pre-launch inspection
MUST require `fs.suid_dumpable` to be 0 or 2 so the gate is nondumpable before
any nonce exposure. Enrollment MUST require kernel-observed
Root ownership of its private proc fd directory and no tracer. Root MUST verify
those conditions before and after association. Root MUST accept exactly one
bounded original-PID/nonce hello and repeat enrollment checks, close the listener and
remove only its identity-matched socket/directory before publishing entry grant.
The gate MUST keep its stream private and verify close-on-exec on that descriptor;
it MUST NOT pass the stream or nonce as stdio, argv or environment to the handler.
The gate MUST consume the complete challenge before hello; MCP protocol startup
MUST wait until that authenticated hello so buffered stdin cannot swallow MCP
bytes across exec. Process exec MUST restore null stdin. Root MUST reject an
incorrect nonce without grant even when a peer shares the gate UID/GID. Kernel
close-on-exec MUST retire the status descriptor before handler exec can restore
dumpability. Root/privileged tracing actors remain outside this guarantee.
Only an exec syscall failure reported on this pre-grant associated stream, using
a fixed bounded frame and positive OS error code followed by EOF, MAY select
existing `exec/spawn`/`spawn`; successful exec closes the descriptor. Empty EOF
MUST NOT prove successful user execution or closure: other candidates still
require original manager/protocol observation and complete cleanup. Malformed,
partial or changed channel evidence MUST retain uncertainty. Closure MUST retire
only identity-matched auxiliary paths under the authority lock before receipt
publication, including never-launched allocations; unknown or torn metadata MUST
refuse, and auxiliary metadata MUST NOT itself establish any outcome. Existing
administrative launch/enrollment controls may omit this runner-only channel and
MUST NOT derive a spawn candidate from their gate exit.
The owned manager launcher is
not the handler root: its exit status MUST NOT select a process candidate or
resolve uncertain root inspection, even after actual launcher retirement.
A manager-query deadline
failure MAY select the existing timeout candidate only when the approved handler
deadline has actually elapsed; an earlier manager-query deadline, identity
mismatch or other inspection failure MUST remain uncertain. This timer observation
MUST NOT infer root exit or native termination, and matching full cleanup/proof
remains mandatory before returning a timeout result. Root status remains only a
candidate; descendant termination, full closure proof and owned handle cleanup
remain mandatory. Neither PID absence nor root status alone proves closure.
Successful stop MUST NOT publish closure evidence, clear claims or authorize
settlement/capacity reuse; permanent closure requires independent completion.

The root-only `observe` operation MUST be read-only, validate the prepared
domain's route, authority, boot and actual cgroup identity, and accept at most
4 KiB from its no-follow regular root-protected `cgroup.events`, allowing one
excess byte solely to detect and refuse an oversized sample. It reports
exactly `format` (`fsm.native-observation/1`), `domain`, `closing`, `populated`
and `frozen`, with boolean observations. Missing/duplicate/unknown event
fields or noncanonical boolean values MUST refuse. Closing material must
match exactly and remain unchanged across the sample; domain identity MUST
be revalidated afterward. A live domain carrying any closed marker MUST refuse
observation as inconsistent. Failure retains uncertainty. The projection MUST
NOT create locks/records, revoke grants, issue closure evidence, clear claims
or authorize settlement/capacity reuse, even when `populated` is false.

Before empty-domain preparation creates its lock or publishes allocation intent,
the authority MUST validate the ordinary installed Root 0711 gate, the required
fs.suid_dumpable profile and readiness of its bounded original kernel-random
source. Refusal MUST leave the counter and allocation inventory unchanged.
Launch MUST repeat these checks and obtain a fresh inherited-input nonce; a
preparation readiness sample MUST NOT serve as launch authorization or a nonce.

Before publishing prepared-domain evidence, allocation MUST read the same
no-follow protected 4 KiB exact cgroup-events sample as observation, require
canonical populated=false and frozen=false, and revalidate its original
directory identity and protections. Failure MUST retain the burned intent
and counter; it MUST NOT publish prepared evidence or reuse that allocation.

Before allocation, a root provisioner MUST publish an immutable catalogue
containing exactly `format` (`fsm.native-catalogue/1`) and `table`, a fully
validated `fsm.handlers/1` document. Publication is exclusive and fsynced
under the authority lock, permitted only while the allocation counter is zero.
The counter must match current authority/boot provenance and the directory
must contain only fresh registration/counter/lock records; existing or lost
catalogue history MUST NOT be silently re-provisioned.
The wrapped document MUST remain within private record byte/depth limits.
The root-only `catalogue` command reads only a root-protected source file and
ancestors. Preparation MUST refuse an absent or invalid catalogue. Binding
and entry authorization MUST independently replay the claimed pending effect
from the registered store, select its approved handler, and match the full
handler fingerprint and retry snapshot. Granted argv MUST equal substitution
of that handler's template using only journal-derived effect arguments.
Neither an operator request nor a claim alone may select another command.
Catalogue validation MUST NOT launch handlers or establish native closure.

A private canonical binding (`fsm.native-claim-binding/1`) contains exactly
`format`, `claim` and `journal_claim`. The authority MUST independently read
the registered store, verify its path identity, the original durable claim
hash and every claim field, and require matching current unstopped ownership
of a still-pending effect on a running instance. It MUST also verify boot,
authority and cgroup identities against a protected allocator-produced
`prepared-<allocation>.json` (`fsm.native-prepared/1`, with exactly `format`,
`phase` and `domain`, phase `prepared`). Merely presenting a public
`NativeDomain` or claiming that a domain is prepared MUST NOT suffice.
Publishing `binding-<allocation>.json` MUST be exclusive and fsync the file
before its parent directory. A partial existing record MUST refuse replacement,
never be treated as a completed binding. Private authority records are bounded
to 8 KiB and regular files, and their reads MUST refuse symlink traversal.
Binding alone MUST NOT launch user code, authorize settlement or establish
closure; the launch path MUST revalidate current ownership because journal
state can change after binding. Allocation, broker authentication, protected
entry and closure integration remain required before this capability ships.

The claim-era writer MUST use VERSION 11. It adds `execution_claimed`,
`execution_stopped` and `execution_settled` records, and an
`execution_enabled` record for verified legacy-upgrade admission. A new store
starts enabled; a migrated legacy store starts quarantined for execution.
Migration alone MUST NOT establish quiescence. Non-executor operations and
strictly read-only inspection remain available in quarantine.

| Record | Required body and effect |
| --- | --- |
| `execution_claimed` | `run_id`, `instance_id`, `effect_id`, `attempt`, `handler_fingerprint`, `retry`, `domain`, `request_id`, `request_fp`. Allocates exactly the next store-local run ID and exclusive ownership of the pending effect |
| `execution_stopped` | `run_id`, `instance_id`, `effect_id`, `handler_fingerprint`, `closure`, `outcome`, `request_id`, `request_fp`. Records verified native closure and an immutable bounded result; ownership remains exclusive |
| `execution_settled` | `run_id`, `instance_id`, `effect_id`, `disposition`, `request_id`, `request_fp`, and the existing ack/attempt state-hash fields where applicable. Applies one disposition and consumes stopped ownership in the same record |
| `execution_enabled` | `previous_head`, `quiescence`, `request_id`, `request_fp`. Enables a quarantined migrated store only after trusted native evidence proves the identified legacy execution environment closed |

New claim-era genesis bodies MUST include `execution_admission: "enabled"`.
A historical genesis without that field MUST fold to quarantined execution;
a VERSION marker alone MUST NOT enable it. Only genesis may determine initial admission;
an unknown genesis admission value is corrupt. A migrated sealed base/1 likewise
starts with an empty quarantined execution block, never inferred enabled from
its lack of unresolved claims. Historical genesis, root/3, base/1 and archive
bytes MUST remain unchanged.

VERSION 11 MUST be durable before any claim-era record is appended. Migration
folds and verifies the authoritative legacy journal/base before stamping that
marker; it MUST NOT rewrite a historical genesis to add admission. A crash
before or after the marker replacement still reconstructs quarantine from the
historical genesis/base. Read-only opens infer the same quarantined state but
MUST NOT stamp VERSION or publish a cache. Historical checkpoint and seal
records select root/3 verification from their recorded discriminator; new
checkpoints and seals select root/4. The base/1 decoder must explicitly require
its historical root/3 discriminator, while base/2 requires root/4 and its
execution block. A version or discriminator mismatch is refused, never guessed.

Each claim-era request ID is at most 4 KiB of UTF-8 bytes; this independent
field ceiling is checked before copying it into a new record or request slot.

A claim, stop or settlement request MUST be conflict-checked and replayed under
the existing request-id rules before mutating ownership. The record's request
fingerprint binds its immutable input identity, contract, evidence or
settlement disposition as applicable; logical clock readings and allocated
journal sequence are not caller content. Idempotent request replay MUST NOT
allocate another run, change a stopped result, increment failed count or apply
an acknowledgement again. An execution-enabled record binds `previous_head`
to the immediately preceding journal hash and requires quarantined admission;
it MUST NOT enable a different journal prefix or discard existing ownership.

The Rust store execution methods accept `expected_seq` as an optional store
journal-head precondition, checked after request replay and before allocation.
They validate a complete projected fold before appending, then publish that
state only after the record is durable. A refused execution request MUST NOT
allocate a run, append a rejection record or claim its request ID. Responses
carry `seq`, `duplicate` and the operation's immutable record body, reproduced
from that original record on cold request replay. A request whose original
record is archived retains the existing sealed-replay refusal.

Run IDs are positive u64 counters local to the store, separate from failed
attempt counts and journal sequences. Execution admission regards an effect as
pending only while its instance is running and its effect ID remains in the
instance's pending list; cancellation preserves historical instance encoding
but makes that effect absent for execution admission and settlement.
The durable high-water mark MUST survive
reconstruction, snapshot, sealing and reopen, including after all claims
settle. Exhaustion refuses allocation; counters MUST NOT wrap, reset or be
reused. A refused stale observation MUST NOT burn a run ID or append a claim.

A stop or settlement naming a run with no current ownership MUST refuse with
`store/execution_stale` without appending or claiming its request ID; replay of
the original completed request remains permitted under ordinary idempotency.

A claim's attempt is the durable failed count plus one. Its retry object is a
snapshot of the immutable handler contract. Historical `effect_attempted`
records carry neither that contract nor a classified failure; production
claim admission MUST refuse a pending effect with such unbound attempts using
`store/execution_contract`, rather than silently restart its count at one.
The existing pending-effect seal pin keeps those records live until the
legacy effect is resolved; ordinary acknowledgement or cancellation remains
available. This refusal MUST survive reopening and cache selection.

The claim's retry object is a
closed snapshot of `attempts`, `backoff_ms`, `max_backoff_ms` and sorted unique
`on` failure classes, with the existing handler-table bounds and semantics.
The fingerprint is the immutable canonical handler-contract digest. For a
validated `HandlerSpec`, its material contains exactly `format`
(`fsm.handler-contract/1`), `effect`, `kind` (`process` or `mcp`), ordered
`argv`, `tool`, `arguments`, integer `timeout_ms`, `on_ok`, `on_failed` and
`retry`. Process `tool` and `arguments` are null; MCP retains the fixed tool
and complete argument template. Missing advances are null; an advance contains
exactly `event`, complete `payload` and ordered `stamps`. Retry contains exactly
`attempts`, `backoff_ms`, `max_backoff_ms` and sorted unique failure-class `on`.
Parsed defaults MUST be explicit in this material. The digest is `sha256:`
followed by lowercase SHA-256 of `fsm:handler-contract:1`, one LF byte, then
canonical material JSON. Table concurrency/manual policy and runtime effect
substitutions MUST NOT enter the per-handler digest. Computing it validates
no handler and grants no launch authorization. It is distinct from the
sanitized public executor-check report identity and changes no historical hash.
`HandlerSpec::contract_value` MUST expose exactly this existing fingerprint
material, with no change to its hash domain or bytes. `HandlerSpec::from_contract`
MUST enforce the existing JSON and handler bounds, require the closed explicit
material and canonical parsed defaults, and reject with `exec/config` when the
material or its digest differs from the caller-held original fingerprint.
It MUST recover literal templates, outcome payloads and stamp ordering without
consulting a current handler table; decoding MUST NOT grant launch or settlement
permission. Full contract values MAY contain secrets and MUST NOT be used as
sanitized health summaries. Contract decoding and native request/completion
validation MUST charge caller-owned Value depth and canonical byte size before
canonical serialization or copying, retaining the existing JSON and envelope
limits; invalid number syntax remains subject to the existing JSON parser.
This decoder alone does not establish durable original-contract storage or
stopped-result/advance recovery.
A retry
ledger retains the original fingerprint and policy, failed count, latest
failed timestamp/class and eligibility deadline while the effect remains
pending; a changed contract MUST NOT reinterpret it. Claim admission MUST
validate pending state, exclusive ownership, contract agreement, allowed
failure class, remaining attempts and supplied logical time under the writer.
At the deadline admission is eligible; one tick earlier it is refused. Delay
uses the existing saturating exponential backoff rule, never elapsed wall time.

The domain object identifies the native backend and its full non-reusable
authority: namespace, native allocation counter, boot identity, cgroup device
and inode, and supervisor/authority generation. These are recorded before
launch. PIDs, unit names alone, advisory-lock availability and time elapsed
MUST NOT establish ownership or closure. A closure receipt MUST bind the exact
recorded domain and run; unknown or mismatched evidence cannot produce a
settleable stopped result. Core validates and folds these values without I/O;
the store/native boundary authenticates closure evidence before publication.

The closed native domain fields are `backend: "linux-systemd/1"`, `namespace`
(32 lowercase hex digits), positive u64 `allocation`, canonical lowercase
UUID `boot`, `cgroup` and `authority` objects each containing exactly u64
`device` and positive u64 `inode`, and positive u64 `generation`. Device zero
is valid. Persisted retry failure classes MUST already be sorted and unique;
decoding MUST reject unknown fields/classes, duplicate classes and unsorted
arrays instead of repairing authenticated data. In-memory constructors may
normalize an input of at most four known classes.

The canonical execution block is a closed object with `admission` (`enabled`
or `quarantined`), u64 `run_high_water`, `claims` and `retry` arrays. Claims
are ordered strictly by run ID; retry entries are ordered strictly by
`(instance_id, effect_id)`. Each claim entry contains exactly `claim` and
`stopped`; `stopped` is null until closure, then contains exactly `closure`
and `outcome`. Claim metadata contains exactly the first seven identity and
contract fields of `execution_claimed` above (the request fields belong to the
request ledger). Instance/effect identifiers are nonempty strings, attempt is
positive and no greater than the policy's attempt limit, and handler
fingerprints are `sha256:` followed by 64 lowercase hex digits.

The initial native evidence reader accepts a bounded regular receipt file
owned by root with all write permission bits clear, opened without following
symlinks or waiting on a special file. Receipts MUST reside under the dedicated
root-owned, non-group/world-writable authority directory
`/var/lib/fsm-containment/<namespace>/authority-<generation>`; every ancestor
MUST be a root-owned non-symlink directory without group/world write access,
and that directory's device/inode MUST match the recorded domain authority.
Closure filenames are `closure-<allocation>-<run_id>.json`; legacy receipt
filenames are `quiescence-<allocation>-<previous_head_hex>.json`. A matching
root-owned file elsewhere is insufficient. A closure receipt material is a closed
canonical JSON object with `format: "fsm.native-closure/1"`, `domain`, positive
`run_id`, and `journal_claim` (the canonical SHA-256 hash of the durable claim
record). The receipt digest uses `fsm:native-closure:1`. A legacy receipt uses
`format: "fsm.native-quiescence/1"`, `domain` and `previous_head`, with digest
domain `fsm:native-quiescence:1`. Files are at most 8 KiB. The authority MUST separately publish an immutable root-owned 0444 canonical
`store-identity.json` object with exactly `format: "fsm.native-store-identity/1"`
and `identity` containing exactly u64 `device` and positive u64 `inode` of the
registered physical store directory, without exposing its private path.
Native evidence readers MUST apply the same protected bounded regular-file
checks to this metadata and retain its identity in the opaque proof.
Before a new stop or legacy-admission write, the store MUST compare its current
physical directory identity to the proof's registered identity and refuse
`store/execution_evidence` on missing or mismatched evidence, including a
byte-identical copied journal; no caller input may repair the metadata.
These checks MUST NOT alter receipt material or historical hash domains.
Receipt reference
constructors remain distinct from opaque file-verified store proof types.
Native authorities MUST publish these files only after their full closure
protocol succeeds; root ownership alone does not implement that protocol.
Unsupported native evidence platforms refuse without opening or mutating a
store. The root/kernel administration trust boundary is unchanged.

Every settlement carries the current `state_hash` and `state_format`, including
interruption over the unchanged instance. Failed disposition timestamps are
those of their single settlement records. A failed settlement's `attempt`
MUST equal its claim's attempt; ack/attempt `outcome` and optional `result`
MUST exactly match the stopped result's projected existing ack/attempt shape.
Interruption MUST NOT carry `outcome`, `result` or `attempt` fields.

A legacy `quiescence` value is a closed object containing the complete
`domain`, canonical SHA-256 `receipt`, and canonical SHA-256 `previous_head`.
The latter MUST equal both the enabled record's `previous_head` and the hash
of the immediately preceding record. This value names protected evidence;
core shape validation alone MUST NOT authenticate it or establish that a
legacy uncontained environment was closed.

A closure value contains exactly positive `run_id`, the complete `domain`,
and a canonical SHA-256 `receipt` digest naming protected native evidence.
Decoding that value MUST NOT authenticate the receipt; native publication
must independently verify the protected evidence and its exact domain/run
binding. An outcome is a closed object containing `status` (`ok`,
`failed` for a terminal failure without a retry class, `interrupted`, or one of the four existing failure classes) and an optional
`result` value. An omitted result and an explicit null result remain distinct.
Unknown outcome fields and statuses are refused. A stopped value's closure
run/domain MUST match its claim. A second stopped record is refused; a
request-id replay returns its original response without appending another.

Each retry entry contains exactly `instance_id`, `effect_id`,
`handler_fingerprint`, `retry`, positive `failed_count`, signed
`last_timestamp`, `failure_class`, and signed `eligible_at`. The latter MUST
equal the policy's saturating deadline, and the count MUST NOT exceed the
policy's attempt limit. A claim sharing a ledger MUST match its contract and
have attempt `failed_count + 1`. Decoders reject duplicate effect ownership,
duplicate run IDs, noncanonical array order, claims above the high-water mark,
contradictory ledger/claim contracts or counts, and any stopped binding
mismatch; they MUST NOT normalize such authenticated state.

An `acked` disposition requires a still-pending effect and a stopped `ok` or
failure result; an `attempted` disposition requires a still-pending effect
and a stopped classified failure result; terminal `failed` MUST NOT authorize
an attempted disposition or increase the failed-attempt ledger. An `interrupted` disposition requires a stopped
interruption while the effect remains pending; when the effect was externally
removed it may instead consume any proved-stopped outcome without applying
that result. Ack or removal discards the effect's retry ledger once ownership is
resolved; an unresolved claim retains its preceding ledger until settlement,
and removal cannot discard that ownership. All refusals MUST leave the
ownership state and high-water mark unchanged. Pure state constructors do not authorize native
launch or replace the store's pending-effect check.

Stopped outcomes distinguish `ok`, terminal nonretryable `failed`, an existing executor failure class, and
`interrupted`; they retain the bounded result needed by settlement. An unknown
termination state is unresolved ownership, not a stopped interruption. A
second stopped record with different outcome bytes is refused. A stopped run
MUST exclude every successor until its settlement is durable, even when the
root exited and no executor currently holds the writer.

Settlement is one atomic record, not a stopped-record removal followed by a
separate ack or failed-attempt append. `acked` applies the existing effect
acknowledgement semantics and result exactly once; `attempted` increments the
failed count once and records its class/timestamp/backoff; `interrupted`
consumes the proved-stopped run without incrementing that count or inventing
an acknowledgement or outcome event. Interruption preserves any still-pending
effect. Cancellation or an independently acknowledged effect does not erase
unresolved ownership; after closure, its interruption disposition may consume
ownership without recreating pending state. Stale run completions cannot
settle a successor. Existing acknowledgement/advance request-ID derivations
and the ack-before-outcome-event recovery order MUST remain unchanged.

The execution state block contains admission/quarantine state, the run
high-water mark, unresolved claims including stopped results, and retained
retry ledgers. Current state MUST be authenticated by `fsm.state-root/5` with hash domain
`fsm:state-root:5`, snapshot `fsm.snapshot/7` / `fsm:snapshot:7`, and sealed
base `fsm.base/3`, including the separate bounded `execution_handoffs` block. The root is separate from instance `fsm.state/3` hashes.
Historical roots and records MUST still verify under their recorded format;
the existing root/3, root/4, base/1 and base/2 functions and bytes MUST NOT be reinterpreted.
Old snapshots are disposable caches; old bases are authoritative and require
explicit validated decoding. Archives, archive verification and repeated seals
MUST retain ownership, policies and the high-water mark.

A base/2 or base/3 additionally carries `execution_claims`, a closed JSON object mapping
canonical positive decimal run IDs to the canonical `sha256:` hash of each
original claim record unresolved at the cut. Its keys MUST match exactly the
base execution block's unresolved claims, including stopped claims; it has at
most 4096 entries. `base_execution_claim_format` is
`fsm.base-execution-claims/1`; `base_execution_claim_root` hashes this map with
domain `fsm:base-execution-claims:1`. The seal MUST commit both fields, and
base decoding MUST check its recomputed root against both the base and seal.
Historical base/1 and root/3 seals carry no such fields and MUST remain valid.
This derived index MUST NOT enter logical state-root/4 material: a claim
record's hash includes its boundary root, so including that hash in the same
logical root would create a self-reference. Repeated seals MUST retain the
original hash, never replace it with a checkpoint, receipt or later run hash.

There are at most 4096 distinct effect entries in the union of unresolved
ownership and retry ledgers. A stopped outcome's canonical value is at most
64 KiB; each claim's canonical identity/contract metadata is at most 4 KiB.
The complete canonical execution state block is at most 8 MiB, counting all
keys, values, delimiters, native evidence and stopped results. Admission,
stopping, replay and cache/base decoding MUST charge these same units and
accept the exact limit while refusing limit-plus-one before mutation. The
existing 16 MiB persistence read cap still applies to each complete artifact.
The execution block MUST contain at most 63 nested JSON containers, counting
its outer object as one; scalars do not add depth, so its enclosing snapshot,
base or root object fits the existing 64-container parser ceiling.

Legacy VERSION 1–10 migration MUST retain historical journal/hash bytes and
quarantine execution until offline verified-quiescence evidence is bound to
the migrated journal prefix. A trusted authority must identify and close the
legacy environment; absence of its PID or a free LOCK is insufficient. An
uncontained legacy environment without verifiable native closure remains
quarantined. Operator assertions, force flags, timeout expiry and counter reset
MUST NOT substitute for evidence. Newer-format refusal happens before mutation
so an older binary cannot ignore claim-era ownership. Read-only inspection
MUST neither migrate, enable execution nor reconcile native work.

### Owned native execution host (provisional, plan 0022)

An owned `NativeExecution` MUST retain its original claim and checked completion
independently of writer availability; bounded observation MUST take no writer
and MUST NOT deliver the completion twice or release capacity on helper EOF.
Its progress MUST expose only phase, optional helper retirement facts and retained
capacity, without identifiers, handler contracts, authority paths or captures.
Starting MUST use the healthy writer-held current-claim admission check; recovery
MUST read the durable original completion and MUST NOT fall back to binding or
launch. Adopting verified completion MUST match the original full claim/hash and
MUST NOT itself assert current journal ownership or physical store identity.
Application MUST require a healthy durable writer, recheck the protected proof's
physical store identity and current original claim/hash, persist matching stopped
evidence before settlement, and refuse a mismatched existing stopped outcome.
Its stopped key is `exec-stop-<effect_id>-<run_id>`; existing stopped evidence can
be consumed without introducing a second stopped key. Capacity MUST remain
retained until durable settlement or exact original settlement-ledger replay;
refusals MUST preserve the checked completion and unresolved ownership.
If original ownership has already been consumed, application MUST use only exact
original interruption, attempt or acknowledgement request fingerprints and MUST
NOT reselect policy using current pending state or a changed handler table.
Original interruption and attempt keys MUST be checked before the acknowledgement
key which a later run may occupy; conflicts or unavailable archived responses
MUST refuse, never invent success. Outcome events remain a separate checked
acknowledgement-before-event pipeline operation. This host primitive MUST NOT be
interpreted as completed automatic production-host routing or gate acceptance.

The typed `Pipeline::claim_native_handler` admission method MUST require a
supported healthy durable writer, resolve the original instance/effect identity
from the journal, validate the handler contract and reject a handler effect-name
mismatch before claiming a request key. Admission MUST charge borrowed handler
strings and nested argument/outcome values against the existing JSON byte/depth
bounds before cloning full contract material or hashing it; excessive caller
material MUST refuse with `exec/config` before claiming or starting helpers.
The complete normalized contract MUST still pass the same exact JSON limits.
Its fingerprint and retry policy MUST
come from that same original contract rather than independently supplied values.
After idempotent claim publication it MUST return only a matching currently owned
claim for the prepared domain and available original record hash; an old duplicate
response MUST NOT recreate consumed ownership. This operation MUST start no
helpers and grant no fallback: hosts prepare, claim under the writer, then recheck
current launch eligibility through NativeExecution::start; Root binding still
verifies its immutable approved catalogue before actual entry.

A matching closure receipt MUST NOT by itself authenticate a native candidate.
NativeCompletion MUST also verify a separate immutable Root-issued attestation
of the complete bounded response under the private `fsm:native-response:1`
hash domain before exposing the result. The protected authority record is
`result-<allocation>-<run_id>.json`, with exactly `format` set to
`fsm.native-result-attestation/1`, `domain`, `run_id`, `journal_claim` and canonical
SHA-256 `response_hash`; it MUST match the original closure's full domain/run/hash
and the response digest. Protected bounded regular-file and authority identity
checks MUST be applied before accepting it. It MUST expose no full contract or
captured output, alter no historical closure material/hash domain, and grant no
launch, settlement or capacity release independently of current ownership.
After closure, Root MUST create once and sync this attestation, publish and sync
mode 0444, then verify completion before durably publishing its private response.
Missing, partial, writable, relocated or mismatched attestations MUST refuse;
recovery MUST NOT repair them or substitute caller-selected candidate material.

A watcher observation MUST project every unresolved original execution claim and optional stopped result from the same read-only journal prefix as pending effects, before filtering cancelled instances, removed effects or current handlers; observation MUST NOT consume ownership.

The scheduler MUST exclude a pending effect with matching unresolved instance/effect ownership before current handler lookup; root exit, stopped evidence and changed handler selection MUST NOT authorize a successor before durable consumption.

Scheduler capacity MUST count the union of observed unresolved original run identities and retained local handles, including stopped owners and owners absent from pending effects; only a locally retained matching original run identity may deduplicate an observed owner, and missing observation MUST NOT release a local reservation.

### Automatic native route discovery (provisional, plan 0022)

Automatic native hosts MUST discover a unique provisioned authority by matching
its protected public physical-store device/inode to the actual data directory;
caller-supplied paths, copied journal bytes and current handler configuration
MUST NOT select authority. Discovery MUST scan only the fixed
`/var/lib/fsm-containment` namespace and canonical positive authority-generation
levels, charging each directory entry to one shared 4096-entry budget before
retaining its name; the 4097th entry MUST refuse before preparation or claim.
Namespace names MUST be exactly 32 lowercase hexadecimal characters and
`authority-<generation>` MUST use a canonical positive u64 decimal generation.
Protected Root-owned non-symlink directory identities MUST be checked before
and after traversal. Public store identity and route documents MUST be read
without following symlinks, from regular Root-owned mode-0444 files, within a
4096-byte limit plus one-byte overflow detection; closed canonical JSON and
unchanged opened/path inode, owner, mode and length MUST be required.
Zero or multiple matching registrations MUST refuse; damaged or offline matching
registrations MUST NOT be ignored to choose another generation. The unique
matching route MUST satisfy current broker operator, boot, authority identity,
epoch and socket identity/access checks before preparation. Discovery MUST NOT
launch, acquire a writer, claim, reconcile or release capacity. Recovery MUST
use the original durable claim's route instead of current discovery results.
These requirements are pending production-host implementation and acceptance.

Native discovery MUST charge non-authority siblings within a namespace to the shared inventory budget and ignore them as registration candidates; an `authority-`-prefixed name with noncanonical generation MUST refuse, rather than being silently ignored.

An owned native host state constructed through `NativeExecution::retain_uncertain`
MUST retain the supplied original claim without starting helpers or synthesizing
completion. Without transport or verified completion it MUST report `Uncertain`,
retain capacity, and refuse observation/application as `exec/inflight_deferred`;
helper retirement alone MUST NOT authorize settlement. The constructor MUST NOT
be treated as authentication of current durable ownership.

The provisional `NativeExecution::start_retained` method MUST allow a host to
install original claim ownership before requesting binding. It MUST consume
its one startup opportunity before writer validation or helper startup, retain
the original claim and capacity on every refusal, and refuse subsequent startup
calls on that object with `exec/inflight_deferred`, including after a read-only
writer refusal or cancellation before startup. Recovery objects and objects with verified completion MUST NOT
start binding through this method. Actual startup MUST use the existing
writer-held current-claim/hash and launch-eligibility checks; this method MUST
NOT claim, settle, release capacity, or fall back to a direct child. Constructing
another retained object MUST NOT be interpreted as authority to retry an
uncertain run. Fresh shared-tick admission and original-run reconciliation use
these guarded operations; task 9403 acceptance passes for provisioned
Linux/systemd; the exhaustive 9404 crash matrix remains pending.

For installed-owner startup, observation MUST stop at `Bound` without requesting
execution, even after repeated polls. `NativeExecution::launch_bound` MUST
recheck supported healthy durable writer mode, the current original claim/hash,
enabled admission, absence of stopped evidence and a running instance with the
original pending effect before requesting execution. A read-only writer MUST
refuse with `exec/mode` without changing the bound transport or requesting entry.
The original bound claim/hash MUST also match the retained transport. A requested
execution or failed execution-helper startup MUST NOT permit another execution
request; original ownership MUST remain retained. Recovery objects MUST NOT
request execution through this method. The preexisting primitive constructor
retains its separate observation-driven behavior; production fresh admission
MUST use the installed-owner writer boundary and remains unimplemented.

The Root broker `close` operation MUST support a prepared allocation with no binding: under the authority lock it MUST refuse any binding or launch/handoff/manager submission material, including pending records, durably revoke admission, verify original domain identity and manager retirement, remove only the matching empty domain, and publish the original domain tombstone; it MUST NOT issue an execution closure receipt or authorize journal settlement, and cold retries MUST retain those checks.

The Root broker `discard-prepared` action MUST accept exactly a structurally valid complete NativeDomain, match it to the protected recorded domain under the authority lock before any revocation, refuse binding or submission material, and perform the prepared-only retirement checks; its successful result MUST echo that original domain without issuing execution evidence, and a different original identity MUST refuse without revocation.

NativePreparedCleanup MUST retain the complete original domain and use only its original route with `discard-prepared`; poll MUST report success only after a successful exact-domain response and actual helper retirement and EOF, while cancellation, errors or helper reap alone MUST NOT imply domain closure, journal settlement or capacity release.

Shared tick recovery MUST adopt unresolved original claims from the same verified read-only snapshot used for observation, independently of current handlers, cancelled instances or pending effects; the Runner MUST retain original identities and stopped evidence across missing observations, pin that retained state to the physical store directory, and refuse physical-store substitution before scheduling. Recovery MUST request only original completion, with at most one owned recovery transport active at a time; failed or retired transport MUST NOT authorize binding, launching, acknowledgement or capacity release. Shared Runner observation MUST continue bounded recovery transport polling and reaping when journal scanning fails. This is initial startup recovery wiring; production native admission, completion application and post-ack discovery remain provisional.

Shared native completion application MUST require a supported healthy durable writer and matching original physical-store proof, persist original stopped evidence and consume its exact disposition before any original-contract event. Helper readiness alone MUST NOT release capacity. After matching durable consumption, the host MUST exclude that owner from retained capacity, retain a declared but disabled/refused event handoff, and retry a disabled handoff only after a changed journal prefix; it MUST use the existing event request key and original checked contract, independently of current handlers. Each tick MUST apply at most one ready native owner, with fair rotation across ready identities; missing completion and writer/readonly refusal MUST retain ownership. This implements recovered-owner application only; native preparation/admission and authenticated post-ack cold discovery remain provisional.

### Native local reservation after authenticated consumption

Shared native settlement releases a local scheduler reservation only when its
full original claim matches and `NativeExecution::settle` has authenticated
durable consumption. Optional event deferral or an event error after consumption
does not retain that execution slot; the original completion remains available
for event reconciliation. Read-only refusal, uncertain closure, and a different
claim leave the local reservation intact. This internal composition changes no
public API, persisted bytes, hash domain, or published error code; fresh native
admission and cold recovery between Ack and event remain incomplete.

Shared Linux tick ownership pins the physical durable store from its first
verified watcher snapshot, including an empty owner set before the first
claim. A different directory identity at the same path is refused before
scheduler selection even when the copied journal prefix is identical; no
claim, handler entry, or settlement is authorized by that refusal. This
internal route check changes no public signature, persisted bytes, hash
domain, or published error code, and does not complete fresh native admission.

### Runner handoff for an already published native claim

Additive provisional `Runner::start_native` accepts a genuine eligible original
claim under a healthy durable writer and a scheduler that already holds its
local effect reservation. It pins the physical store, installs retained original
ownership, binds the local reservation, and only then requests one-shot helper
startup; post-installation failure retains the claim and capacity without an
automatic replacement request. Shared tick observation pauses at Bound, and
writer-held application rechecks current eligibility before requesting entry
once. Read-only entry refusal retains Bound for a later healthy writer; an
uncertain entry attempt cannot be repeated. Shared cancellation requests native
transport cancellation and retains ownership rather than creating a legacy
settlement. Unsupported platforms refuse startup with `exec/mode` and no
automatic direct-child fallback. Persisted bytes, hash domains, and error codes
are unchanged; default production host selection, bounded shutdown and cold post-ack recovery
remain incomplete, and this handoff does not release production acceptance gates.

Fresh Runner installation additionally MUST authenticate the claim namespace and
authority generation against the protected registration for the physical store
before retaining ownership or requesting binding; an identical copied journal
does not authorize the original domain. Shared native writer application MUST
match the host physical-store pin before changing entry permission or applying
a completion; refusal preserves original ownership and local capacity. These
checks change no public signature or persisted format, and provisioned native
process and MCP controls remain required before production acceptance.

Provisional `Runner::new_native()` explicitly selects fresh native admission
through both shared tick entry points, with no automatic direct-child fallback.
Public `Runner::spawn` MUST return `exec/mode` on a native-selected runner
before child startup, capture creation or MCP worker creation, including an
explicit direct invocation; direct-child primitives remain available through
`Runner::new`. This closes a bypass of the selected native admission boundary,
without changing journal formats, claim identities or request keys.
The host MUST retain the original journal-derived effect and checked handler
contract before requesting preparation, serialize allocator transport, collect
one original domain only after helper retirement/EOF, and publish its claim
under a healthy writer on the pinned physical store before one-shot binding and
entry. Its claim request key is `exec-claim-` followed by lowercase SHA-256 hex
of canonical JSON `[effect_id, domain]`; existing ack/event keys are unchanged.
Preparation does not consume handler runtime: the authority enforces the
original actual-entry timeout. A read-only borrowed tick MUST NOT enqueue or
start a new allocator request; owned observation and cleanup remain independent
of writer access. A cancelled queued request that never started may release
only its matching unclaimed local reservation; a delivered unclaimed domain
requires exact original-domain cleanup success plus helper retirement/EOF.
Unknown allocation, cleanup, or claim publication MUST retain capacity, never
produce legacy synthetic acknowledgement and never allocate a replacement.
An observed genuine claim after an uncertain append transfers to retained
original ownership. Existing ready native owners take precedence over further
claim publication, and readonly snapshots are dropped before the standalone
writer opens. This selected host path is not yet provisioned-runtime accepted;
default CLI/MCP/service host selection, shutdown, reconciliation and cold
post-ack recovery remain unfinished, with production acceptance flags false.


The additive pure `fsm_core::record::execution::AcknowledgedHandoff` value
reserves `fsm.execution-handoff/1` candidate material for cold post-ack
recovery: complete original Claim, lowercase original claim-record digest,
original handler contract, actual terminal StoppedOutcome, nonzero
acknowledgement sequence and the unchanged derived acknowledgement/event keys.
Decoding MUST charge the complete candidate to 128 KiB before copying, bound
contract and outcome individually to 64 KiB, reject unknown envelope fields,
match the handler fingerprint under `fsm:handler-contract:1` and the original
retry policy, and select the original on_ok/on_failed event from the actual
outcome; interrupted outcomes and absent/malformed event envelopes MUST refuse.
Omitted and explicit-null result fields MUST remain distinct.

This pure value authenticates neither acknowledgement, native closure nor
accepted event delivery, and does not replace the execution layer's checked
full handler parser. Store integration MUST compare its material against the
actual claim hash/stopped outcome and publish it atomically with acknowledgement,
then retire it only through a matching actually accepted event. Current
ExecutionState, journal VERSION 11, state-root/4, snapshots and sealed bases
do not yet retain this value; their formats and historical hash bytes remain
unchanged in this additive value-only change. Cold recovery and production
acceptance remain incomplete, and decoding a candidate grants no execution or
event permission.


`AcknowledgedHandoff::matches_acknowledgement` MUST compare the complete
original Claim, actual stopped result including omitted/null semantics,
stopped closure run/domain binding, verified original claim-record hash,
actual acknowledgement key and append sequence; a matching key or fingerprint
alone MUST NOT pass. The caller must supply verified journal/evidence inputs:
this pure comparison cannot authenticate arbitrary caller-owned values and
does not itself publish a handoff or consume an event.


In-memory journals MUST NOT write automatic or shutdown snapshot caches, including at the 10,000-record boundary; their checkpoint records and state roots retain the ordinary replay semantics.


Verified execution replay MUST retain each unresolved claim’s original record hash as separate, bounded replay context, sourced from the verified ExecutionClaimed record or authenticated sealed-base claim index; snapshot caches MUST reconstruct this context from those sources and MUST NOT supply it from mutable cache metadata. Producers MUST replace provisional claim hashes with final published hashes at checkpoint boundaries. This context MUST NOT enter execution logical serialization or historical state roots, avoiding claim-record self-reference, and MUST disappear with settled ownership.


VERSION 12 acknowledgement handoffs: an ExecutionSettled record MAY carry a bounded execution-handoff/1 only for disposition acked, matching its exact original owner, verified original claim hash, stopped outcome and acknowledgement identity. Replay MUST install the handoff atomically with acknowledgement and MUST remove it only in an accepted EventApplied transition matching its original instance, derived event key, original send fingerprint and payload including timestamp stamps; rejection, cancellation and missing observations MUST retain it. Each handoff is at most 128 KiB and the ordered collection at most 4096 entries and 8 MiB. New state-root/5 commits execution_handoffs separately, leaving historical root/4 bytes unchanged; snapshot/7 and base/3 carry the collection. Explicit historical base/1 and base/2 decoders retain their original root/3 and root/4 domains with empty handoffs. VERSION 1–11 migration never invents obligations for old acknowledgements or rewrites records.


Matched native manager stop MUST tolerate the original cgroup retiring between its initial metadata observation and live revocation only when a fresh no-follow observation proves absence; that observation MUST NOT establish fencing or closure. The operation MUST still validate the complete original handoff and manager identity, durably revoke entry, and independently prove manager retirement before closure publication. Surviving, replaced or unreadable groups MUST preserve refusal.


Cold native outcome-event recovery MUST adopt bounded authenticated execution_handoffs separately from execution ownership and capacity; it MUST require the original healthy physical-store writer, protected namespace/generation and authority device/inode, and the original checked handler contract. It MUST NOT prepare, bind, execute or acknowledge again. Only accepted-event fold retirement completes delivery; rejection, ignored responses, conflicting keys, disabled instances and cancellation retain the obligation. Ready event-only and ownership work MUST share bounded fair ticks, with failed event attempts parked until later journal progress; a retained warm original completion keeps its existing retry path.

Retiring a locally retained cold handoff after its absence from the current
healthy writer's verified handoff collection MUST also require the original
acknowledgement request slot, sequence and execution-settlement fingerprint.
A missing or changed original acknowledgement MUST park the retained local
obligation without mutating the journal; absence alone is not reconciliation.


### Claim-bound native closure request

The provisional Linux native client NativeShutdown MUST request closure only
for an exact unresolved claim from a verified durable store prefix and its
original journal claim-record hash, after matching the protected registration
to the original physical store and full authority identity. Read-only snapshots
are permitted: closure requests acquire no journal writer and MUST NOT append,
bind, execute, acknowledge, deliver events or release scheduler capacity.

The request MUST retain an independent helper transport and MUST NOT replace
or discard a still-owned execution transport. A successful broker close reply,
helper retirement or EOF MUST NOT establish domain closure. Polling may return
VerifiedClosure only after reading the immutable protected receipt from the
original authority-generation directory, matching the complete original domain,
run and journal hash, and checking the receipt's registered physical store.
Malformed replies, missing receipts, authority changes and deadline expiry MUST
remain uncertain and preserve durable ownership. A single checked monotonic
deadline MUST cover discovery, helper startup and receipt verification; discovery
elapsed time MUST NOT restart that deadline. This transport deadline does not
promise that filesystem operations or a control report cannot block: the public
bounded lifecycle driver still requires independent report/admission control.

This primitive supplies no original handler result and MUST NOT fabricate a
NativeCompletion. The caller may apply only existing authenticated interrupted
settlement under a healthy original writer, preserving pending effects and
sending no machine event. A published claim without a completed native binding
may have no claim-matched receipt; successful retirement of its prepared domain
MUST NOT consume that claim. Production shutdown must implement all admission
phases, local control authentication, report deadlines and quiet/blocked stdio
progress separately before task 9402 can complete.


### Original binding before claimed closure

The provisional NativeShutdown request uses the additive close-claimed action
with the full original fsm.native-claim-binding/1 value. The protected authority
MUST compare its complete stored binding and recorded domain before any fencing
or closure side effect; a mismatched claim, original hash or domain MUST refuse.
Allocation-only close remains a separate existing low-level operation and MUST
NOT be used by this claim-bound client. Missing bindings retain uncertainty;
this action does not create bindings or resolve published-before-binding claims.
Receipt authentication after closure remains mandatory and cannot substitute
for validation before the action. Older helpers lacking close-claimed refuse;
the client MUST NOT fall back to allocation-only closure. No persisted format,
hash domain or public Rust signature changes in this correction.

### Receipt-only native interruption application

NativeShutdown::settle_interrupted MUST require a healthy original durable
writer and retained authenticated original closure proof. It MUST recheck the
proof's physical store, complete original claim/hash and current ownership
before recording status interrupted with result omitted and consuming through
the existing Interrupted settlement. Existing non-interrupted stopped outcomes
MUST refuse without overwrite; an existing interrupted outcome retains its
original omitted/null/result semantics. Pending effects and their retry counts
MUST remain intact, and no acknowledgement or machine event may be invented.
When the effect is already absent, existing Interrupted retry retirement rules
remain applicable. An absent or replaced current claim permits only exact
original interrupted transaction replay; missing or pruned original keys MUST
refuse, never establish reconciliation through absence. Original stop and
interrupted request-key derivations and persisted formats remain unchanged.
Application MUST retain both owned transports and cannot alone release local
capacity or establish a bounded production shutdown report.


### Uncertain-run inspection and reconciliation (task 9403)

Production `execute runs` implements the journal-only inspection portion;
`execute reconcile` selects shared authenticated result recovery or guarded
closure; startup recovery covers owned pre-run and runner-phase claims with the
same original identity and closure checks; task 9403 acceptance passes for provisioned
Linux/systemd; the exhaustive 9404 crash matrix remains pending.
The `fsm.execution-runs/1` report contains observed_seq, inventory_complete,
inventory_limit (4096), and run-ID-ordered runs with run_id, instance_id,
effect_id, backend, phase, native_evidence and next. The execution_ownership
summary MUST match MCP health's enabled, unresolved_runs, stopped_runs and
outstanding_handoffs counts from the same verified observation. Phase is unresolved or
durably stopped; native_evidence is unverified because inspection performs no
native query. Inventory completeness covers current unresolved ownership in
the verified fold, not historical settled runs or native liveness.
CLI inspection MUST refuse missing/uninitialized directories with neither an
initialized format marker nor a verified journal prefix as exec/inflight_deferred,
without initializing or migrating them; an initialized empty store remains valid.

Inspection MUST use the verified read-only journal fold without taking a writer,
creating a directory, issuing a broker action or changing native metadata.
It MUST enumerate unresolved claims in run-ID order with an explicit bounded
result inventory and journal horizon, expose run/effect identity and recorded
backend, and distinguish durable stopped state from unverified native state.
It MUST omit original handler contracts, arguments, command lines and results.
MCP health counts MUST derive from that same unresolved inventory; unavailable
or degraded observation MUST NOT be represented as an empty verified inventory.

Operator reconciliation and automatic startup recovery MUST share the original
claim/hash, physical-store and protected authority/domain validation and the
same authenticated closure acceptance. Neither missing completion publication,
missing native metadata, elapsed time nor a PID observation proves death.
Reconciliation MUST refuse a live original owner across claim publication,
binding and execution; a transiently free journal writer or an idle runner is
insufficient. This refusal MUST precede any domain fencing or termination, and
authorization MUST prevent a concurrent owner from entering after the check.
Uncertain owner observation MUST refuse without native or journal mutation.

After proved original closure, a healthy original writer MUST revalidate current
ownership and commit the matching stopped record before consuming ownership or
admitting a retry. Authentic original completion MUST retain its original
outcome and event semantics; closure without a result permits only the existing
Interrupted disposition, preserving pending effects without fabricated events.
Repeated or concurrent application MUST use existing exact transaction replay;
a stale run ID MUST NOT target its successor. Changed handlers, copied stores,
unknown backends and unavailable facilities MUST NOT reinterpret or clear the
original claim. No force-clear, age threshold or kill-by-PID fallback is allowed.

### Local native admission provenance

Public native preparation MUST use owned allocations: `start` and `for_store`
request `prepare-owned`, and `poll` returns `NativePreparedOwner`; callers retain
the guard and read metadata through `domain()`. The unprivileged broker and
client MUST refuse legacy `prepare` allocation requests. Existing protected
legacy allocations retain their original recovery rules and gain no ownership
witness. This corrects the unreleased Rust preparation API before native release;
its `poll` return type changes, with no journal format, hash or error-code change,
and downstream native acceptance remains required.

The additive `NativePreparation::start_owned` / `poll_owned` path now returns
a non-cloneable `NativePreparedOwner` only after acquiring the original operator
lease and checking the protected authority, boot, route and lease inode; `poll` and `poll_owned` both retain the original lease in their returned guard. Production admission now requests owned
preparation, retains its guard across
prepared and uncertain claim states, and moves it into the original execution
owner before dropping the admission reservation; native acceptance remains pending.

The internal `prepare-owned` broker route now establishes both leases before
publishing a `prepared-owned` domain, and binding requires its operator lease
to be held; reconciliation retains that lease before considering closure.
Reconciliation can publish an absent original binding after exact claim validation
while retaining both original leases; production guard transfer is wired but native acceptance remains pending,
so this route does not establish end-to-end pre-run reconciliation support.
The planned pre-run ownership extension is not complete. Its allocator MUST
establish both the protected runner lease and a separate owner lease before
delivering a domain that may be claimed. The owner lease MUST be created by the
authority in its non-writable namespace, accessible only to its configured
operator, empty, single-linked and opened without following symlinks; its inode,
operator and authority identity MUST be revalidated before use. Operator access
MUST NOT permit replacing the lease inode or its parent namespace.
The preparation owner MUST exclusively retain this kernel lease before exposing
the domain, across claim publication (including uncertain append), binding and
execution, until authenticated retirement and settlement or proved unclaimed
cleanup. Transfer between admission and execution MUST move the owned guard
without an unlock interval; a cloned domain or observed PID is not that guard.
Public admission/entry paths MUST NOT bypass the same ownership requirement.
Pre-run reconciliation MUST exclusively retain both original leases through
original claim/hash and prepared-domain validation, any original binding
publication, fencing and authenticated closure. A held, missing, replaced or
unverifiable lease MUST refuse before mutation; legacy allocations MUST NOT be
retrofitted with ownership material to infer retirement. An absent binding may
be published only after proving the exact current durable original claim under
the healthy writer and original authority, while both guards exclude entry;
absence alone MUST NOT authorize that publication or settlement.

The protected claimed runner MUST acquire an exclusive kernel-backed
`runner-<allocation>.LOCK` lease before binding validation or handler entry and
retain it until native cleanup and original completion publication have returned.
The empty regular lock file MUST be root-owned with mode 0600 and one link,
opened without following symlinks, and durably established before entry.
Contention MUST refuse execution without launching or replacing that lease.
An idle lease or missing lease is not closure proof; the planned reconciliation
path still requires original binding, authority, physical store and authenticated
closure checks. Missing lease material MUST NOT authorize fencing or settlement.
This lease does not establish ownership before the runner starts; reconciliation
of claim/binding phases remains unimplemented and cannot infer death from absence.
The internal `reconcile-claimed` broker action MUST exclusively hold an existing
protected runner lease through original-binding validation, fencing and closure;
it MUST refuse a held or missing lease before any closure side effect, MUST NOT
publish or reinterpret a completion, and MUST NOT settle journal ownership.
The operator command and startup recovery use this primitive; owned pre-run
recovery additionally retains both original leases as specified above.
While holding that lease, reconciliation MUST refuse any original result
attestation, completed response or pending completed response before fencing;
unreadable or partial publication MUST remain uncertain rather than becoming
an interrupted outcome. Existing original results require authenticated recovery.
`NativeShutdown::start_reconciliation` MUST select this guarded action while
retaining shutdown's original claim, physical store and receipt verification;
it MUST NOT treat the broker response alone as sufficient closure evidence.
The shared `service::reconcile_run` path MUST require a healthy durable writer
and either its current exact run ID or an exact original settlement replay.
A settled run MUST replay only through the existing request ledger using its
original claim, recorded disposition and settlement request ID from verified
history; unavailable original history or replay MUST refuse. Historical replay
MUST perform no native query, closure, append, event or successor mutation.
When original completion is published, it MUST
use the same authenticated `NativeExecution::recover` and original settlement
path as startup, retire its helper before settlement, and preserve the original
contract and outcome without loading current handlers. Otherwise it MUST
authenticate closure and retire its helper before writing Stopped/Interrupted
through the existing guarded settlement path. Partial result material MUST
remain uncertain. It MUST refuse runs without current ownership or exact
original settlement replay, and refuse active owners;
it MUST NOT load a replacement handler table, launch work or fabricate an event.
`execute reconcile --run-id <id>` MUST select that shared path for canonical
positive run IDs, with an optional finite `--timeout-ms` (default 8000), and
refuse missing stores or runs with neither current ownership nor recorded
settlement before opening a writer; historical eligibility MUST be revalidated
through exact request replay under that writer. Other
platforms MUST report unsupported mode without native or journal mutation.

Startup recovery MAY request the same guarded claimed reconciliation for an
observed original run without a published completion, at most once per retained
run per incarnation. It MUST NOT bypass an active or missing protected runner
lease, reinterpret result material, or infer closure from transport retirement.
It MUST retain the original run until authenticated closure and helper retirement
permit writer-revalidated interrupted settlement. A refused closure attempt MUST
NOT prevent later authenticated recovery of an original completed publication;
all closure helpers MUST remain included in transport retirement inventories.
Shutdown and reconciliation MUST preserve a broker refusal's bounded, sanitized
reason only from the closed native response envelope; malformed responses MUST
remain protocol refusals and no refusal may establish closure or settlement.
This implements runner-phase and owned pre-run startup reconciliation;
task 9403 acceptance passes for provisioned
Linux/systemd; the exhaustive 9404 crash matrix remains pending.

An executor incarnation MUST distinguish its own admissions from journal claims
retained only through observation or recovery. An observed claim alone MUST NOT
authorize that incarnation to request active cancellation or protected closure.
Local publication responsibility MUST survive failed binding and uncertain
claim append. When a prepared admission observes its publication after append
uncertainty, it MUST match the original instance/effect, complete prepared native
domain, handler fingerprint and retry policy before transferring provenance;
failed matching MUST retain the admission and its reserved capacity. Repeated
observation MUST NOT downgrade a locally admitted claim. Provenance authorizes
no settlement without independently authenticated original closure evidence.


### Bounded queued protocol output

Notifier::queued MUST atomically enqueue complete canonical frames, bounded by 256 retained frames and 8 MiB of retained Vec allocation capacity including in-flight writes; serialization temporaries are outside this accounting unit. Budget exhaustion MUST return WouldBlock without partial publication. Close MUST refuse further admission without waiting on the actual writer; drained MUST require successful write/flush of every admitted frame after close. Write failure MUST remain broken and MUST NOT report successful drainage. Drop grants no drainage guarantee.


### Shared bounded protocol input

Both ordinary MCP stdio frames and SessionIo reverse-protocol replies MUST
limit retained payload to 16 MiB of wire bytes excluding the final LF. Exact
limit frames MUST remain accepted; limit-plus-one frames MUST refuse. Oversized
frame drainage MUST use bounded borrowed input chunks without accumulating the
discarded tail, and MUST consume only that frame so a later frame remains intact.
Elicitation reads MUST report oversized or invalid UTF-8 input as InvalidData,
which request_and_await maps to the existing io/read error. Ordinary serve MUST
retain its existing parse-error response for an oversized frame. Read failures
during drainage MUST propagate rather than claim successful resynchronization.
This blocking framing primitive establishes no silent-client or cleanup deadline.


### Admission-free native completion observation

On supported Linux native runners, service::observe_admitted_with MUST observe retained native transport and recover original durable ownership without scheduling pending effects, new preparations, bound handler entry, retries or machine deadlines; it MAY apply at most one original completion or event-handoff action through the healthy original physical writer, and MUST refuse legacy runners, memory/read-only/poisoned writers and copied-store routes using existing exec/mode or exec/inflight_deferred errors. Recovery transport is permitted; it MUST NOT authorize handler entry. This writer-dependent pass grants no independent shutdown deadline or blocked-I/O progress guarantee.


### Original interrupted native retirement

On supported Linux, Runner::retire_native_interrupted MUST release an
original locally admitted owner only after matching original NativeShutdown
closure authentication, exact original Interrupted settlement replay under the
healthy original physical writer, and actual reap/stdout EOF/stderr EOF of both
closure and execution helpers. It MUST refuse foreign observed ownership,
missing/pruned replay and replacement routes; actual retained completion MUST
keep its original settlement/event policy. It MUST NOT append a record, launch
work, consume pending effects, clear unrelated reservations or infer closure
from helper death. A false result means helper retirement remains incomplete.
The caller retains NativeShutdown independently until its helper is retired.


### Shared native admission closure and local targets

On supported Linux native runners, NativeAdmissionControl::close MUST irreversibly close that original runner's shared admission fence without waiting for journal or native I/O; clones MUST share closure and MUST NOT affect a successor runner. Queuing, allocator dispatch, claim publication and bound entry MUST check the fence at their authorization boundaries, after preceding route/writer validation where applicable. A transition authorized before closure MUST retain its original reservation and any published claim; original binding may finish but a separately closed entry fence MUST forbid handler entry. Owner observation MUST cancel never-requested queues and request cleanup for delivered preparations while retaining unknown allocation and uncertain publication. Original completion and event-handoff application remain eligible. Closure alone MUST NOT assert native termination, writer release, completed shutdown or a bounded report. Native runner Drop closes this local fence only and grants no native cleanup guarantee. Runner::local_native_claims MUST borrow original locally admitted execution-retained claims in run-ID order, excluding foreign observation and already consumed completion owners; it MUST NOT claim to enumerate unclaimed preparations or prove liveness.

### Explicitly owned native lifecycle driver

On supported Linux, `service::OwnedNativeExecutor` MAY own a healthy durable
writer and native runner; the host MUST explicitly drive `tick` or `poll`.
Its cloned `ExecutorControl::stop` MUST validate a finite timeout in
`1..=MAX_TIMEOUT_MS` before closing the original shared admission fence.
Repeated requests MUST preserve the first absolute deadline; abort MUST NOT
de-escalate to drain. Control waits MUST operate only on metadata, never
journal, native or protocol I/O, and MUST report `Uncertain` at the deadline
when the worker has not confirmed cleanup.

Shutdown polling MUST NOT admit pending effects, launch bound entries, schedule
retries or advance machine deadlines. Drain permits original completion until
its deadline; abort requests exact original local claim closure immediately.
Closure starts are bounded to four attempts and four retained helpers per
poll, with fair target rotation. An authenticated original completion MUST
retain its original policy; interruption MUST wait for execution transport
reap and both EOFs before deciding that completion is absent. Authenticated
interruption MUST preserve pending effects and MUST NOT invent acknowledgement
or outcome events. Foreign client helpers MAY be cancelled; foreign native
domains MUST NOT be closed by this driver.

`Stopped` MUST require an empty retained local claim inventory, zero unclaimed
preparation reservations, actual helper retirement, a successful original-store
observation and completed release of the owned writer. Unknown preparation
ownership MUST remain charged. `Drop` MUST NOT publish guaranteed cleanup.
This opt-in, explicitly driven library API does not establish independent
production stdio progress or install a native authority.

Closing a transferred runner's admission fence alone MUST NOT count as an
explicit lifecycle stop request or authorize releasing its owned writer;
writer release requires actual stop request metadata in addition to quiescence.

### Owned native MCP session composition

On supported Linux, `mcp::serve::serve_owned_native_session` MAY explicitly
compose an owned native driver with one protocol/journal owner and one clock.
Its input factory MUST construct the sole reader on its worker; existing
borrowed session helpers MUST retain their existing bounds. The worker queue
MUST retain at most one complete frame, in addition to the current consumer
frame and worker frame, each under the existing 16 MiB wire-byte ceiling;
oversized-frame markers MUST NOT synthesize an oversized allocation or lose
the following frame. LF MUST be delivered separately from frame storage.

Quiet-input wakeups MUST only observe admitted work through the original
driver; they MUST NOT schedule pending effects, retries or machine deadlines.
Reverse-reply waiting MUST use the same reader and MUST be interruptible by
independent lifecycle control. Queued output MUST preserve its existing
256-frame/8 MiB allocation-capacity accounting, including in-flight writes.
Owned-session feed shutdown MUST request stop without joining a blocked feed;
detachment MUST NOT be reported as actual retirement.

The owned session MUST validate finite shutdown bounds before starting
workers or closing admission. EOF and protocol/startup errors MUST initiate
explicit abort when no earlier request exists; an existing drain/abort request
MUST preserve its first deadline and escalation. Native cleanup and output
drain MUST use that original deadline without extending it. Native and journal
I/O remain worker operations; independent control waiting MUST report
uncertainty when that worker stalls. `ShutdownReport::timed_out` distinguishes
elapsed deadline without confirmed native cleanup from other uncertainty.

`OwnedSessionReport::shutdown` describes original native ownership, native
helper retirement and writer release; `output_drained` MUST only confirm
actual successful output write/flush, separately. Input/feed detachment MUST
NOT be advertised as proved I/O-worker retirement. The owned entry MUST NOT
claim installed native tree acceptance or change production CLI selection.


### Opt-in local owned executor control

On Linux, an explicit LocalControlEndpoint publisher MAY expose its actual
OwnedNativeExecutor control beneath an existing owner-only mode-0700 root.
The endpoint MUST use a kernel-random 256-bit incarnation and the original
writer's physical data-directory device/inode at publication, never a PID.
Its directory MUST be mode 0700 and its socket/discovery file mode 0600.
Requests MUST use the closed six-field fsm.executor-control/1 schema, match
that exact identity and validate finite timeout bounds before closing admission.
Discovery MUST refuse ambiguous matching endpoints, symlinks and nonprivate
endpoint files; it MUST NOT select an arbitrary newest endpoint or process.

Requests MUST be bounded to 1024 wire bytes before LF and reports to 128 KiB
before LF. One nonblocking transport loop MUST retain at most 64 connections,
each with at most one bounded input and response allocation; read and output
frames MUST have absolute 250 ms budgets. Outstanding response waits MUST NOT
occupy parser workers or exclude later abort requests: saturation MAY evict
an old response connection but MUST preserve every already accepted stop.
Control processing MUST only use actual control metadata and MUST NOT open,
lock or write the journal or perform native closure operations.

The client MUST bound discovery, connect and transport together under its
original finite caller deadline and recheck that deadline before transmitting
after connect. Detached client workers MUST remain charged until retirement,
with at most eight per process. Missing/evicted/timed-out responses MUST mean
transport uncertainty, never confirmed admission, native termination or writer
release. Control reports MUST NOT substitute for authenticated native closure
proofs. The server MUST preserve the actual control's first accepted deadline
and abort escalation; the client's transport budget is a separate local bound.

Explicit endpoint close MUST validate finite bounds before stopping transport
admission, wait only to that bound and confirm cleanup only after removing its
exact captured inode identities. Replacement files MUST remain untouched.
Drop MUST NOT claim native shutdown, cleanup or response-delivery guarantees.
This opt-in publisher does not change production backend selection and requires
the host to drive the owned executor independently.


### Operator local executor stop

`fsm execute stop --data-dir <dir> --mode drain|abort --timeout-ms <n>` MUST
use the proved local control client and MUST NOT load handlers, open Store,
acquire a journal writer or create a missing data directory. Mode and finite
timeout MUST be validated before transmission. The lookup root MUST default to
HOME/.cache/fsm/control, with optional --control-dir for a different explicit
publisher root; discovery MUST remain read-only and ambiguity MUST refuse.

A validated actual Stopped report MUST be emitted on stdout with exit zero.
An actual Uncertain report MUST be retained in existing exec/inflight_deferred
error details on stderr with exit one. Transport failure MUST use that existing
error with admission_closed, writer_released and native_cleanup_confirmed
explicitly null, not false or true. Invalid arguments MUST use the existing
args error and exit two; unsupported platforms MUST refuse with exec/mode.
The finite client bound covers discovery/connect/transport; ordinary CLI output
rendering remains synchronous. This command targets only the exact publisher
incarnation and MUST NOT claim all durable data-directory ownership is closed.
It MUST NOT activate native production selection or fabricate an endpoint for
an executor that has not published one.


### Paired native lifecycle writer strategy

On supported Linux, service::PairedNativeExecutor MAY retain original native
components and a verified durable read-only snapshot across temporary writer
leases. Construction MUST NOT acquire or initialize a writer. Data-directory
physical identity MUST remain pinned; replacement MUST defer rather than
redirect original work. Explicit tick MAY schedule ordinary work; poll MUST
only observe admitted transports, original completion and claim-bound closure.
Closure transport MUST progress before writer attempts, using the original
verified snapshot and existing full-claim/physical binding guards; settlement
MUST still require a healthy original writer and actual helper retirement.

Writer release MUST be unconfirmed before temporary writer I/O and confirmed
only after the actual lease drops. A blocked writer MUST retain every unresolved
local claim and reservation; native closure alone MUST NOT publish Stopped
before durable original settlement. An empty paired actor MAY stop while a
foreign actor holds its writer, without acquiring or releasing that foreign
lease. Stopped actors MUST NOT recover new helpers. LocalControlEndpoint MAY
publish_paired from that actual driver's pinned physical identity and control;
a replaced directory or already stopped publisher MUST refuse. These opt-in
APIs MUST NOT claim installed two-live-native-actor acceptance or activate the
current production selector. Drop remains outside the shutdown guarantee.

The opt-in paired native driver exposes `tick_reporting`, preserving the structured
writer-unavailable fact from its explicit scheduling tick; `tick` remains the
line-only convenience entry. `ExecutorControl::wait_for_request` validates a
finite timeout before waiting on independent metadata, wakes for an explicit
request and returns false on an idle timeout; it closes no admission, renews no
shutdown deadline and publishes no cleanup or writer-release fact.

The Linux opt-in `fsm_cli::standalone::run_paired` drives an actual paired
native owner with independently queued diagnostic stdout supplied by the host;
ordinary ticks follow the configured interval while admitted polling and explicit
request notification remain independent of it. Its `StandaloneReport` separates
shutdown facts, successful output drainage, dropped diagnostic line count and
initiating failure. Logs share the existing 256-frame/8 MiB allocation budget;
oversized (over 64 KiB), multiline or saturated lines are counted as dropped,
and broken output requests abort without blocking ownership. Output close/drain
uses the original shutdown deadline and never joins a blocked writer. The host
must publish and close the exact actor endpoint explicitly; production command
routing and installed nonempty native acceptance remain pending.

The Linux standalone shutdown report exposes `shutdown_deadline`, the original
monotonic request deadline, for transport retirement.
`LocalControlEndpoint::close_until(deadline)` stops transport admission even
when that deadline has expired, without starting another waiting budget;
its boolean confirms actual endpoint removal separately from native shutdown,
writer release and response delivery. Deadlines beyond the finite executor
bound are refused before transport admission changes.

Ordinary `fsm execute` now retains a paired native owner and publishes its
private incarnation endpoint at `--control-dir` or `$HOME/.cache/fsm/control`.
It uses bounded diagnostic delivery, independent admitted observation and the
original shutdown deadline for endpoint retirement; unavailable or unsupported
native execution refuses without a legacy fallback. Final success requires
authenticated empty shutdown, endpoint removal and complete diagnostic delivery.
This source integration remains pending production acceptance; embedded MCP
and the low-level service host have not yet changed selection.

Production standalone final errors preserve the initiating code, message and
hint, nesting its original details under `initiating_details` and attaching
separate observed shutdown, endpoint and output facts. Unavailable output facts
are null; run IDs and counts are decimal strings, avoiding JSON number precision
loss. Endpoint removal does not imply native cleanup or successful delivery.

The Linux owned native session reporting entry
`serve_owned_native_session_reporting` retains the initiating protocol I/O
failure in `OwnedSessionReport.failure`, alongside actual shutdown/output
facts and `shutdown_deadline`, the original monotonic control deadline.
The existing `serve_owned_native_session` entry preserves its error-return
behavior. Invalid timeout options refuse before worker startup or admission
changes; this additive entry does not select production embedded execution.

Production embedded stdio through `run_with_mode` now retains a native writer
owner and publishes its private endpoint under `$HOME/.cache/fsm/control`.
Quiet input permits admitted observation and explicit control; sole reader
construction occurs in its worker. Healthy native startup has no legacy
fallback; contended/unhealthy startup preserves the exact observed diagnostic
prefix without publication or reopening into execution. Borrowed session APIs
retain their bounds and explicit backend selection. Unsupported production
embedded stdio refuses; HTTP ownership remains incomplete. This routing change
does not establish installed native or autonomous plan 20 acceptance.

Production native stdio failures expose the initiating I/O message and kind
with separate actual shutdown, endpoint removal/error and output drainage
facts in the existing exec/inflight_deferred error frame. Typed native startup
errors retain their executor code. Cleanup refusal does not replace an
initiating protocol failure or imply confirmed native retirement.

Native stdio endpoint publication refusal is reported through the executor
error frame; a startup transport refusal does not confirm native cleanup.

The Linux owned native MCP session MUST admit operator action diagnostics through a bounded independent writer worker, with the same finite queue and line limits as standalone diagnostics; stderr write/flush MUST NOT execute on the native owner. Shutdown MUST close both output queues and use the original stop deadline for delivery; `operator_output_drained` reports actual operator write/flush completion and `operator_lines_dropped` counts rejected diagnostic lines, independently of native cleanup and protocol delivery. Diagnostic loss MUST NOT imply execution loss or verified containment closure.

Native owned MCP protocol warnings about requests preceding `notifications/initialized` use the same bounded operator diagnostic queue as executor action lines, with the same separate drainage and loss facts; borrowed sessions retain their existing warning behavior, and no journal or wire format changes.

Supported Linux native standalone and owned stdio final CLI error delivery MUST preserve the initiating code, hint and observed cleanup details, MUST use the original shutdown deadline and MUST NOT join a blocked writer or prevent failure exit; small pipe frames MAY attempt atomic nonblocking delivery at expiry, while other frames use an independent bounded worker observed only until that deadline. Final delivery is best effort and MUST NOT imply confirmed cleanup; pre-owner startup, borrowed and HTTP error rendering retain their existing behavior.

Native preparation cancellation retains its original finite transport deadline:
when eligibility is lost or admission closes, an already requested preparation
continues bounded observation so an authenticated delivered domain can be
cleaned through the original prepared-domain protocol; no bind or execute is
permitted after cancellation, and transport failure or unknown allocation
retains uncertainty and capacity rather than treating helper death as closure.
This changes cancellation cleanup only, with no journal format, hash domain,
request key, dependency or MSRV change.

The Linux private owner control additionally accepts the closed four-field
fsm.executor-observe/1 request (format, incarnation, store_device, store_inode)
through local_control::observe(root, data_dir, timeout_ms). It MUST authenticate
the same original private endpoint and physical identity and use the existing
1024-byte request, 128-KiB response, 64-connection and eight-client-worker bounds.
It MUST return only the actual bounded control metadata snapshot, without
opening or writing Store, requesting stop, changing admission, extending any
existing shutdown deadline or authorizing closure/settlement. Observation may
report running/draining/stopping as well as terminal phases; stop clients still
MUST accept only terminal identity-matched reports. An observation snapshot
MUST NOT be interpreted as a native closure receipt or current durable prefix.
This is additive local metadata observation, with no journal/format/hash-domain,
dependency or MSRV change; production race and full lifecycle acceptance remain
separate requirements.

Private observation version 2 keeps the closed four-field identity request
fsm.executor-observe/2 and returns fsm.executor-observation-report/1 with the
original twelve metadata fields plus preparation_phases. This is either null
(unpublished/poisoned, never inferred zero) or exactly ten bounded nonnegative
counts: queued, preparing, prepared, cleaning, unknown_allocation,
uncertain_preparation, uncertain_cleanup, uncertain_domain, claim_uncertain,
and closed; their sum MUST equal unclaimed_reservations and be at most 4096.
The original owner's coherent ExecutorControl::observation snapshot supplies
these counts, with no journal/native I/O or control mutation. The fixed array
uses the same phase order and is optional when unpublished/poisoned. Counts do
not grant cleanup, settlement or closure permission. Legacy observation /1
and terminal stop reports keep their original twelve-field schema unchanged.
local_control::observe now requests /2; this additive health API/protocol version
changes no persisted format, hash domain, dependency or MSRV.


Native prepared-cleanup failure diagnostics are best-effort lifecycle log lines
beginning `native-prepared-cleanup-uncertain`, bounded to 1024 UTF-8 bytes with
control characters replaced by spaces. A runner retains at most one pending
line and drains it once through owned or paired lifecycle polling; additional
simultaneous failures may be omitted. It reports the cleanup transport/refusal
message, not handler output or native closure evidence, and never releases an
uncertain reservation, changes shutdown eligibility or renews a deadline.
This additive diagnostic leaves the provisional public Rust surface, private
control versions, persistent journal/hash formats, dependencies and MSRV intact.

For a closed authenticated prepared-cleanup refusal response, the diagnostic
preserves its broker reason prefixed `prepared cleanup refused`, capped to
1024 Unicode characters before the lifecycle line's stricter 1024-byte limit;
success still requires the complete matching original domain, and malformed
or extended responses never confirm cleanup.

Native owner observation failures also produce best-effort
`native-execution-uncertain` lifecycle lines under the same 1024-byte UTF-8 and
single-line rules. A runner retains at most one cleanup line and one execution
line (2048 bytes total), drains one per lifecycle poll with cleanup first, and
may omit simultaneous additional failures. These messages do not authorize
retry, launch, closure or settlement and leave original claims, reservations,
helpers, deadlines and public/wire/persistent formats unchanged.

Native preparation MUST tolerate contention before allocation publication:
only a protected authority lock's `WouldBlock` acquisition may be retried,
within one two-second monotonic budget established at preparation entry.
Preparation MUST revalidate the original authority identity after acquisition
and before publishing an allocation intent or advancing its counter. Other
lock errors and acquisition exhaustion MUST refuse without those mutations.
No already-published preparation or execute action may be retried, no host or
handler deadline is renewed, and no refusal proves native closure or releases
uncertain ownership; public APIs and persistent formats are unchanged.

A native transport worker MUST NOT finish solely because a later reap observes
helper exit and both EOFs after its preceding response poll returned pending.
It MUST retain that original transport and decode its response, or publish an
explicit refusal, before finishing; delivery still requires actual worker join.
A joined transport worker without a published response MUST refuse as uncertain
rather than remain pending. Neither case grants native closure or claim release,
retries an execute action, or renews the original absolute deadline; public APIs
and persistent formats are unchanged.

Binding validation MUST tolerate contention of its protected authority lock
before any binding, entry authorization or manager submission mutation: only
`authority busy` during acquisition may be retried, within one two-second
monotonic acquisition budget established at validation entry. Validation MUST
check the original authority identity after acquiring the lock. Exhaustion
MUST refuse without publishing binding or launch state; subsequent validation
still checks the current original claim and all existing entry guards. This is
not permission to retry an execute action, renew a host/handler/shutdown deadline,
release ownership or infer closure, and changes no public or persistent format.

Exec-status association MUST tolerate protected authority-lock contention only
before changing socket or directory access: only `authority busy` acquisition may
be retried, within the existing two-second monotonic association budget; lock
acquisition, original handoff/domain revalidation and peer authentication share
that budget. Other lock failures refuse immediately, exhaustion retains the
claim without entry authorization, and no execution or shutdown deadline is
renewed; public APIs and persistent formats are unchanged.

Prepared-domain discard MUST retain the complete original domain while the
protected authority lock is contended: only the `authority busy` refusal before
revocation/native mutation may be retried, and acquisition plus retirement share
a single two-second cleanup budget established at request entry. Other refusals
remain uncertain immediately; exhaustion never releases capacity or proves
closure. Original prepared identity, no-submission, durable revocation, manager
retirement and empty-cgroup checks still govern success. The host transport and
shutdown deadlines are unchanged, with no new public or persistent format.

Native preparation failures now share the single pending lifecycle diagnostic
slot with prepared cleanup, using `native-preparation-uncertain`; startup and
poll failures retain their original uncertainty and reservation semantics.
Closed native-response/1 refusal envelopes preserve preparation and completion
broker reasons with their operation prefix, sanitized and capped to 1024 UTF-8
bytes; malformed envelopes retain generic errors. These additive diagnostics
change no error code, journal/hash format, control version, dependency or MSRV
and grant no closure, ownership release or renewed deadline.

The public low-level `service::run` loop now selects `Runner::new_native()`
and uses the shared preparation, writer-held durable claim and one-shot entry
sequence; unavailable native authority or unsupported platforms refuse fresh
effects with `exec/mode`, without a direct-child fallback or synthetic ack.
Explicit legacy Runner primitives remain available. The existing borrowed
clock/emitter signature, polling and contention policy are unchanged, as are
journal/hash formats and error codes; this is a provisional native
host selection change, not completion of bounded shutdown or recovery.
Its borrowed emitter still executes inline and this loop has no stop handle;
callers requiring independent control should use the owned/paired lifecycle
drivers, and full service-loop shutdown integration remains unfinished.

Exclusive public service loops now probe and release the writer when native
admission leaves a tick without a write attempt, so a held writer still
produces its store diagnosis and the existing three-blocked-tick exec/mode
refusal; paired loops keep their existing writer-on-demand behavior. The probe
claims no execution ownership and changes no journal/hash format, error code,
public signature or shutdown deadline; complete bounded shutdown remains open.

Borrowed MCP ExecutorLoop construction now selects native shared-tick
admission: the public borrowed session helper requires protected native
authority for fresh effects and refuses unavailable capability with exec/mode,
without direct-child fallback or synthetic acknowledgement. Read-only borrowed
ticks retain their prohibition on fresh allocation or entry. This provisional
host selection changes no public signature, error code, journal/hash format,
dependency or MSRV; protocol input still drives these borrowed ticks and its
inline diagnostics/EOF shutdown remain outside the bounded owned-host claim.

## HTTP reverse-response idle and closure

A quiet HTTP reverse-response mailbox poll MUST remain idle rather than imply
client EOF; reverse waiting MUST return to its deadline check between idle
polls. Closing an original session MUST close its mailbox and wake its waiters
before acquiring session execution state; late responses MUST NOT reopen that
mailbox. This does not establish autonomous HTTP ownership, bounded mailbox
admission or reverse-request streaming, which remain implementation obligations.

HTTP reverse-response admission MUST bound each original mailbox to 64 queued
messages and 32 MiB of charged owned payload bytes; count/byte exhaustion MUST
return HTTP 503 before adding the response, without journal mutation. Charges
include Value storage, owned String/array capacities, worst-case JSON escaping
and a conservative 4096-byte allowance per object entry; queue slots have a
separate count bound. Dequeue transfers payload ownership to the reader and
releases the queue charge; close discards queued responses and remains available
at saturation. This is a mailbox queue budget, not a complete host/output budget.

## Private execution-host command ownership

The staged private execution-host owner MUST retain its owned Store and injected
logical clock; application handles MUST submit owned tool arguments and RPC IDs
without obtaining a Store borrow. Each complete operation MUST capture its
immutable result, committed journal sequence and appended interval before the
next operation. Idempotent replay MUST retain the ordinary Store fingerprint
and request-key semantics. Application admission MUST permit at most 32 commands
and 32 MiB of charged retained allocations per host, and 8 commands and 16 MiB
per original session generation; charges MUST survive dequeue until command
retirement. Charge Value storage, owned String/array capacities and a conservative
4096-byte allowance per object entry, plus the reserved envelope/control
allowance below; no original wire-frame copies are retained in this boundary. A separately reserved coalesced stop
control and original-session close MUST remain available at saturation. Stop
MUST reject queued commands before Store dispatch, finish any already executing
operation, and reject further admission. The writer-only private owner has no executor. A staged Linux native owner
retains the original OwnedNativeExecutor, including its sole writer, and uses
the same complete command boundary. Production Linux embedded stdio constructs this owner; HTTP and borrowed
helpers retain their existing contracts. Interactive continuations and bounded
raw input/output are integrated; complete egress ordering and diagnostic
isolation remain outstanding acceptance requirements.

Private host cancellation MUST reserve control metadata for every admitted
request, including the retained RPC-ID copy and a conservative 12 KiB allowance
for envelope/control BTree entries, the bounded host-local numeric flag key and
its cancellation-set allocations, in addition to payload capacities. Controls
MUST remain bounded by admitted/in-flight commands and retire with their
reservation. Unknown or retired RPC IDs MUST NOT install future cancellation.
Cancellation MUST match the original session generation and RPC ID; cancellation
before dispatch MUST perform no Store operation, claim no journal request key
and suppress the response. Cancellation after dispatch MUST retain the existing
coarse-loop flag even without progress metadata; a single engine step remains
noninterruptible and an already-created durable workflow is not cancelled.

The staged private native host MUST drive native decisions without requiring
a client command. Its wait deadlines MUST use a monotonic clock and MUST NOT
supply journal timestamps; each decision pass samples the injected logical
clock exactly once and supplies that fixed timestamp to retry/deadline decisions
and every journal operation in the pass. Shutdown observation similarly uses
one sampled timestamp per observation pass. Service a native pass after at most eight admitted commands or the
configured finite poll interval. Independent lifecycle controls MUST be observed
within 50 ms of an idle wait regardless of that interval. A host stop MUST close
native admission through the original control before rejecting queued commands.
Shutdown MUST poll the existing original lifecycle driver under its original
deadline; return the driver and its report to the transport even when closure
is uncertain, rather than treating owner exit as evidence of stopped work.
Operator diagnostics MUST use the existing bounded output worker, with loss
counted explicitly. Complete completion fairness, real-handler responsiveness
and production transport integration remain acceptance obligations.

An ordinary private native owner decision MUST use one injected logical sample
for at most eight original driver ticks. Follow-up ticks MUST require a changed
durable prefix or retained native readiness; an unchanged prefix with no ready
work, a refusal, or lifecycle stop MUST end the batch. If work remains at the
bound, the owner MUST schedule an immediate continuation and offer an admitted
application command before that continuation. Monotonic waits MUST NOT advance
logical deadlines; the existing scheduler determines every eligible action.
The private owner MUST keep its wait clock independently injectable from the
logical clock, use the monotonic clock in production, and treat equality with
a wait deadline as ready. Wait-clock injection MUST NOT replace original
lifecycle shutdown deadlines.


Private store-backed protocol reads (resource listing/resolution and argument
completion) MUST share the tool command mailbox, original session generation,
count/byte admission and cancellation controls. Charge retained resource-URI
String capacity and completion Value storage/capacities alongside the reserved
envelope and RPC-ID copy. Read results MUST capture their committed prefix
before another owner command, return no appended interval, and use the existing
resource/completion implementations. Native resource resolution uses the
original driver's sanitized handler table; no transport obtains a writer
borrow or installs a second writer through these commands.

The staged hosted protocol entry MUST use the same method implementation as
borrowed compatibility helpers, submitting store-backed tools and reads through
owned host commands. Session-side response formatting and subscription state
MUST remain outside the writer owner. Application admission exhaustion MUST
return the existing JSON-RPC server-busy code -32004 before dispatch; output
backpressure after a commit MUST NOT be reported as admission refusal. A
pre-dispatch cancelled hosted request MUST remain unanswered with its journal
key unclaimed; closed host admission MUST terminate the original session.
This private entry is not yet selected by production transports; interactive
continuations, progress forwarding and complete egress admission remain open.

The staged owned stdio composition MUST reuse the existing capped byte framing
and shared method handler, with a session store facade that supplies no Store
reference to the hosted adapter. Open quiet input MUST leave the native owner
free to progress; EOF, failed output and independent control MUST enter the
original lifecycle stop. Preserve its first absolute deadline and requested
mode rather than escalating a previously requested drain merely to reject
queued application commands. Never join a live owner after the deadline;
retain the original worker or returned native driver and report uncertainty.
Protocol/operator delivery facts MUST remain separate from native writer
release; unknown diagnostic loss is None, not a fabricated zero count.
This private composition is not selected by the production process entry yet;
interactive/progress forwarding, complete egress and fsm.executor/2 discovery
remain required before that selection.

The private hosted adapter's admitted-response wait also observes original
session close, failed protocol output and native lifecycle stop at finite
intervals, without requiring
another owner turn. Retirement suppresses the response and cancels the original
session; output failure retains its BrokenPipe classification instead of
becoming a silent retirement or admission-busy reply. This does not prove
completion, undo committed work, or release the
writer. The composition still retains and reports the original uncertain owner.
This internal control integration adds no public signature, error, journal,
hash, dependency, wire version or production-backend selection.

The staged hosted elicitation path MUST split preparation, client waiting and
settlement: only preparation and settlement run on the writer owner. Its
continuation MUST retain the original admitted count/byte reservation, session
generation, RPC ID, journal request key and cancellation control until it is
settled or dropped. Resuming an admitted continuation MUST reuse that slot,
including at a full application count, and MUST NOT accept another session's
continuation. Retained question/continuation allocations and the returned
answer MUST be charged conservatively to the existing host/session byte limits;
growth that cannot fit MUST refuse before settlement, with no mutation or key
claim. Parser/operation temporaries remain independently bounded, and this
staging does not claim the complete encoded-egress budget is finished.

Preparation MUST retain the immutable original machine identity, not a cloned
compiled machine or Store reference. It captures the journal prefix and the
instance's latest touched-record sequence from the derived history index.
Accepted settlement MUST use the original machine's payload typing and the
ordinary send path: if the target history changed, pass the captured prefix as
expect_seq; otherwise pass the current prefix, allowing unrelated writes.
Ordinary dedup/fingerprint lookup MUST remain before the sequence precondition.
A stale accepted answer therefore returns existing req/seq_mismatch without
journaling or claiming its key; declined/cancelled answers remain unjournaled.
The current instance/machine and enabledness checks still belong to ordinary
send. Borrowed compatibility helpers retain their existing unchecked-prefix
behavior and blocking-reader bounds.

Client waiting MUST run in the adapter; original RPC cancellation, original
session close and native stop MUST remain observable between finite owned-input
waits without an application slot. Cancellation after preparation returns the
existing req/cancelled tool outcome and cannot cancel an already committed
workflow. EOF or an unanswered-client timeout MUST release the continuation
reservation without changing durable workflow state. Production selection,
full progress/egress integration and versioned discovery remain outstanding.

The private hosted stdio output adapter MUST cap one complete canonical frame,
including its trailing LF, at 16 MiB before allocating encoded bytes. Its
preflight MUST traverse borrowed values with checked size arithmetic and a
maximum nesting depth of 256, including protocol wrappers, then allocate only
the accepted complete-frame size and use the ordinary canonical encoder.
This is an output bound, not a claim that construction of the response Value
is bounded by encoded size. Public borrowed/direct output remains unchanged.
Hosted output MUST retain at most 64 frames and 32 MiB of frame allocation
capacity, including any blocked in-flight frame. An encoded-frame or queue
refusal MUST close hosted output admission and expose failure to independent
input/lifecycle observation; successful enqueue MUST NOT imply delivery.
Already retained frames stay charged until the actual writer retires them.

Private hosted stdio tool envelopes MUST preserve progress metadata and the
original cancellation control while dispatching. Progress emission MUST use
only the hosted bounded queued notifier, never a direct transport writer.
Admission MUST conservatively charge retained adapter metadata/RPC copies,
owned tool-context allocation, and up to four metadata/progress-token copies
plus a 32 KiB context allowance to the original request reservation. Progress
retains the existing rate limit and final-report behavior. Refused progress
output MUST remain visible to hosted session/lifecycle failure observation.
This does not move long diagnostics off the writer or establish mutation
response/notification ordering; both remain integration obligations.

Hosted stdio response waits with owned input MUST continue bounded frame reads
while waiting for owner replies, routing cancellation to the original session
and treating EOF as original-session retirement without undoing committed work.
They MAY answer ping immediately; other incoming requests MUST remain in wire
order for the ordinary dispatcher. A separate input backlog MUST retain at most
eight raw frames and 16 MiB of String allocation capacity, with queue metadata
bounded by that count. These frames are not yet application-admitted; their
parsed copies MUST be dropped. Backlog refusal uses server-busy before any
store operation. Cancellation of a known deferred request MUST suppress it
without installing future cancellation for unknown IDs. Borrowed input helpers
MUST retain their existing blocking behavior. Owned idle intervals and original
stop/output failure observation MUST remain finite during the wait.
Owned response waits MUST observe `notifications/initialized` on the original
session and enqueue the usual initialization warning once per incoming request,
including requests refused by the backlog; deferred dispatch MUST NOT repeat it.
The wait-path warning MUST retain at most 512 method characters before bounded
diagnostic admission, so hostile method names cannot expand its buffer.
After original-owner retirement, a broken operator output queue MUST permit
shutdown to return without waiting for impossible drainage, retaining failure
and reporting operator delivery as unproven.

An unwind from the hosted protocol adapter MUST retire its original session,
request shutdown through the original native control and retain the adapter
failure in the shutdown report; catching an adapter panic MUST NOT establish
native containment, worker retirement or output delivery without their usual
evidence, and MUST NOT join a live owner beyond the original shutdown deadline.

Production Linux embedded stdio now constructs the original owned native host,
with independently scheduled decision passes and bounded owned protocol input
and output. The active host MUST expose `fsm.executor/2` with
`progress: "autonomous"` at the unchanged `fsm://executor` URI, retaining
sanitized handler fields and `external_executor: "unknown"`; the closed v1
progress enum MUST NOT be extended. Polling observes this host rather than
causing progress, and an open quiet stdin remains a live session. EOF and
output refusal request original-control supervised shutdown; uncertainty
remains explicit until native, endpoint and output retirement are proven.
HTTP, contention/degraded fallback and borrowed helpers keep their existing
contracts. This stdio capability and wire discriminator change has a pre-1.0
minor-version consequence, with no core semantics, journal bytes, hash domains,
public Rust signatures, dependency or MSRV changes. Production acceptance and
complete bounded egress ordering remain pending in plans 20–23.

Linux embedded stdio MUST accept `serve --execute --poll-interval-ms N`
with a positive whole-millisecond interval no greater than 86400000; the
default is 250 ms. Invalid intervals or use outside embedded stdio MUST
return the existing CLI args error before opening the store or starting
execution. The interval bounds scheduler timed wakes, while reserved native
stop observation remains bounded independently at 50 ms; wait duration MUST
NOT supply logical journal time. Borrowed helper behavior remains unchanged.
While running, the original stdio owner MUST also observe already-admitted
native work at idle intervals no greater than 50 ms, independently of the
configured scheduler interval. Each observation MUST sample the injected
logical clock once and retain the native publication guard through original
driver return; observation MUST NOT schedule effects, retries or machine
deadlines and MUST NOT reset the eight-command scheduler allowance.
After an ordinary tick, the owner MUST publish original lifecycle inventory
before relying on quiescence; a complete empty inventory with actual helper
retirement MUST suppress further observation scans until the next tick.
`OwnedNativeExecutor::has_ready_native_work` MUST inspect only retained local
readiness without I/O, clock reads or ownership changes; it MUST NOT establish
completion, closure or writer availability. After admitted observation makes
a retained preparation, bound entry or original outcome ready, stdio MUST
schedule its next ordinary owner decision without waiting for the configured
timer, while preserving application-command service between decision passes.

The opt-in owned native driver's worker polling mode MUST reserve capacity
before dispatching immutable startup material and MUST perform protected helper
validation, socket setup and helper spawn on the transport worker; the caller
MUST NOT wait for startup. The original absolute deadline MUST cover queued
startup and transport observation. Cancellation observed before startup MUST
refuse startup; cancellation during startup MUST remain pending until the
original worker observes the original returned helper. Already-running
transferred helpers MUST retain their original process, streams and deadline,
without a new helper startup, and MUST reserve before worker adoption.
Socket polling, reap and final transport drop MUST run on that original worker.
A started transport MUST NOT report retirement before its original worker is
joined and its actual reap and both EOFs are observed. A joined worker that
completed startup without creating a helper MUST report `not_started` separately
from reap and EOF, MUST leave all three actual child/stream observations false,
and MAY retire only that empty transport; it MUST NOT authorize native-domain
cleanup, claim release or successful completion. Before worker join, absent
helper creation MUST remain unobserved. NativeHelperProgress::is_retired MUST
recognize only this joined startup refusal or all three actual started-helper
observations; it supplies no native closure evidence.
The pool MUST reserve one of 128 transport slots and a fixed 16 MiB transport
charge before helper startup; the charge MUST remain until both owner handle
and worker retire. Parsed response storage MUST be preflighted at 2 MiB using
Value storage, String/array capacities and a conservative 4096 bytes per object
entry. Refusal MUST retain uncertain native ownership and MUST NOT settle a
different attempt. Synchronous standalone construction and polling remain
unchanged. In this mode original completion and shutdown receipt verification
MUST run on independent proof workers after actual transport response collection,
reusing the original reserved transport charge without taking a new slot. They
MUST capture only immutable original response/identity/route material, never a
Store or journal allocator; current ownership and writer-protected settlement
checks MUST remain with the owner. Original deadlines and cancellation MUST
suppress proof delivery before and after verification. Neither a completion
nor shutdown proof MUST be delivered before actual proof-worker join, and
transport inventory MUST withhold retirement while that original proof worker
is unjoined. A proof refusal MUST retain uncertain original ownership and MUST
NOT append an outcome or release capacity. Store-route discovery and current
writer-held physical-store checks still execute on the owner; full retained
completion storage accounting remains unfinished.

NativeCompletion::verify MUST apply the same 2 MiB retained-response storage
preflight before canonical serialization, material cloning or receipt access,
including for caller-built Values and synchronous standalone verification;
small encoded content MUST NOT exempt excess String or array capacity.
The storage preflight MUST enforce JsonLimits::DEFAULT.max_depth before
descending into an array or object, including before canonical serialization
validates caller-built material; excess nesting MUST return refusal.
Failure MUST return bounded uncertainty without supplying completion evidence.
This response-entry guard does not account for all derived completion storage.

Sequential binding and execution transports of one original NativeRun MUST
reuse that run's reserved slot and byte charge; a full pool MUST NOT refuse
the execution phase solely because the binding phase retains its reservation.
Successor startup MUST require actual predecessor transport retirement and
MUST preserve the original claim, claim hash and run deadline; this reuse
MUST NOT authorize concurrent helpers, writer entry, closure or settlement.
If the predecessor has no worker reservation, successor startup MUST honor
the currently selected worker mode and reserve before dispatch, retaining
synchronous standalone behavior when worker mode is absent.
Original NativeRun binding, recovery and execution startup MUST carry the
same absolute Instant deadline into prepared worker material; route validation,
serialization and phase changes MUST NOT restart or extend that deadline.

Hosted stdio MAY finish its retirement wait after the original owner has
returned and operator diagnostics have drained when protocol output is
permanently broken; it MUST retain the initiating failure and actual output
drainage facts rather than waiting solely for the original deadline or
claiming delivery. A merely blocked, healthy output still uses the original
deadline and MUST NOT be classified as broken.

The installed process panic hook MUST permit unwind on the current owned
stdio protocol-adapter catcher and on internally marked opt-in transport/proof
worker bodies; it MUST NOT synchronously format panic text, write stderr or
abort on those marked threads. `filter_native_worker_panics` MUST wrap an
embedding hook without installing global process state and MUST forward all
unmarked panics unchanged, including a thread with a forged worker name.
Only internal original worker entry MUST arm this thread-local permission;
thread names, environment values and sibling threads MUST NOT grant it.
Actual worker join failure MUST override any previously published response or
proof with bounded uncertainty, MUST NOT manufacture helper observations or
native closure, and MUST retain original claim/capacity until authoritative
settlement. Observations published before a panic MAY still establish actual
transport retirement after join; unobserved reap/EOF MUST remain unknown.
Other host/handler threads and borrowed serve paths retain their fatal panic
behavior; this scoped unwind boundary establishes no native-domain closure.

Owned hosted stdio MUST defer change-feed publication across an application
mutation from admission through response queueing, and across each native
decision pass until its journal operations return. The guard MUST hold no
store or transport I/O lock; a skipped feed pass MUST preserve its watermark.
Elicitation MUST acquire its application publication guard only after the
client has answered, so unanswered questions do not suppress autonomous
updates. Scoped guards MUST retire on response completion, cancellation,
disconnect or unwind, without undoing committed idempotent results.

Hosted change-feed listing invalidation MUST remain membership based:
`MachineDefined`, `InstanceCreated` and `InstanceInvoked` may produce one
coalesced `notifications/resources/list_changed` per observed batch; an
`EventApplied` or `DeadlineApplied` alone MUST NOT produce that notification.
Application to an existing instance still invalidates its subscribed resource
URI after the original application response publication scope releases.

Pending-contract evidence MUST accept an explicitly manual pending effect when its current executable closure is compatible, without acknowledging it or authorizing automatic execution.

The shared service native preparation path MUST check pending contract evidence against the complete loaded table before queuing a preparation helper; incompatibility or unknown evidence MUST return the typed contract refusal without a preparation reservation or journal mutation, leaving other eligible work serviceable; final writer-held claim and bound-entry contract revalidation remain a separate integration obligation.

Warm native completion reconciliation MUST validate the exact replayed acknowledgement handoff against the original claim, closure-bound claim hash, handler contract, stopped outcome, acknowledgement request and sequence before consulting the healthy writer's verified handoff collection; an absent matching obligation then proves accepted-event fold retirement and MUST retire the retained completion without another send, while conflicting membership MUST refuse and outstanding membership MUST use the original handoff delivery checks.

Warm original completion delivery MUST retain its existing verified closure and healthy physical-writer authorization; it MUST NOT add cold-discovery operator routing as a substitute for that proof. Warm and cold paths MUST share exact accepted-event fold retirement checks, while cold adoption MUST retain its additional original protected route and operator validation.

Executor watcher retry observations MUST derive failed attempt number and original backoff timestamp from both legacy EffectAttempted records and native ExecutionSettled records whose disposition is attempted; acknowledged and interrupted native settlements MUST NOT consume failed attempts or establish a backoff timestamp, and restart MUST retain the original journal timestamp rather than renewing the deadline.

Native preparation refused by worker capacity before helper dispatch stays queued for a later owner turn; it does not create an unknown allocation or publish an attempt outcome.
