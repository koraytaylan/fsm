Load any example with `fsm machine add examples/<name>.json`. The grammar lives at `fsm://docs/spec`.

## parallel_review_deadline

Intent: run review and audit concurrently while keeping the engine's
one-event-one-macrostep guarantee — an event still fires at most one
transition of its own — and expire review only through an explicit poll.

The two `regions` enter in document order. An event scans review before audit
and still applies at most one global transition. `review_timeout` is scheduled
when `awaiting_review` is entered; no background task fires it. The poll below
uses a later injected timestamp and applies exactly that one due deadline.

```
$ fsm validate examples/parallel_review_deadline.json
created:    true
dry_run:    true
$ fsm machine add examples/parallel_review_deadline.json
created: true
$ FSM_CLOCK_MS=1000 fsm instance new parallel_review_deadline --request-id pr1 --json
{"children":[],"configuration":{"kind":"parallel","leaves":{"audit":"auditing","review":"awaiting_review"}}, ...}
$ fsm instance send inst-pr1 audit_ok --request-id pr-audit
status: running
$ FSM_CLOCK_MS=31000 fsm instance poll inst-pr1 --request-id pr-timeout
deadline: review_timeout
deadline_applied: true
status: completed
```

A no-due poll is journaled. Retrying its `request_id` returns the original
no-due observation even after time advances; use a new `request_id` for a new
observation.

## case_review_parent and case_review_child

A parent that delegates one step to a child machine and takes the child's
finding back as an event. The two are separate definitions on purpose: the
child is a reusable review step, and the parent names it **by digest**, so the
definition the parent invokes cannot change under it.

```
$ fsm machine add examples/case_review_child.json
$ fsm machine add examples/case_review_parent.json
$ fsm instance new case_review_parent --request-id new-1
$ fsm instance send inst-new-1 open --request-id send-1
$ fsm instance invoke inst-new-1 check --request-id inv-1 --json
{"child_instance_id":"inst-a111dfb0920dcaf7dd51064b","child_machine_id":"case_review_child ...}
$ fsm instance send inst-a111dfb0920dcaf7dd51064b clear --request-id child-1
$ fsm instance return inst-new-1 check --request-id ret-1 --json
{"child_instance_id":"inst-a111dfb0920dcaf7dd51064b","duplicate":false,"ok":"true","outcom ...}
$ fsm instance show inst-new-1
leaf: decided
```

The parent's context now holds the child's finding: `outcome: clear`.

The parent's history is the whole story, four records for the parent and two
for the child:

| seq | kind | what it says |
|---|---|---|
| 3 | `instance_created` | the parent exists |
| 4 | `event_applied` | `open` moved it to `delegating`, creating one pending slot |
| 5 | `instance_invoked` | the child exists, its id derived from `(inst-new-1, check)` |
| 6 | `event_applied` | the child's own `clear`, in the child's history |
| 7 | `invocation_returned` | the child's `finding` reached the parent as `$done.invoke.check`, which moved it to `decided` |

There is no `instance_created` for the child: seq 5 **is** its creation, and
the fold re-derives the child by running the same creation the record
describes. The id is a function of the parent and the slot, so a second writer
issuing the same invocation computes the same id and the store replays rather
than creating a second child.

`fsm validate examples/case_review_parent.json` on its own reports
`expr/unknown_field` for `evt.finding`, and says why: a done-invoke payload is
typed from the **child's** declarations, which live in a store. Add the child
first, or point `--data-dir` at a store that already holds it, and the same
command validates.

## parallel_fork_join

Intent: fork into two regions and join when one of them finishes, with the
whole reaction sealed in the record of the event that caused it.

`approve` ends the `review` region. The engine raises `$done.region.review`,
the `audit` region's handler takes it in the same macrostep and lands in
`reconciling`, and the eventless transition out of `reconciling` closes the
instance. One send, one record, two reaction microsteps — `instance history`
lists them under that record, and `explain --seq 3` shows each with its own
candidates and pipeline.

```
$ fsm validate examples/parallel_fork_join.json
created:    true
dry_run:    true
$ fsm machine add examples/parallel_fork_join.json
created: true
$ FSM_CLOCK_MS=1000 fsm instance new parallel_fork_join --request-id fj1
status: running
$ FSM_CLOCK_MS=2000 fsm instance send inst-fj1 approve --request-id fj-approve
context:
  joined: true
microsteps:
  → microstep 1 (internal $done.region.review): auditing → reconciling
  → microstep 2 (eventless): reconciling → closed
status: completed
$ FSM_CLOCK_MS=2000 fsm instance history inst-fj1
chain_verified: true
```

A generated `$done.*` event is raised only when some transition names it; a
region finishing in a machine that never joins on it leaves no trace of it.

## expense_approval

Intent: route an expense through peer or manager review using a decimal limit, with an ancestor `withdraw` and a child-first override.

The spec is a tree `draft` → compound `review` (`peer_review`, `manager_review`) → terminal `approved` / `refused`. `submit{amount decimal(2)}` uses document-order guards (`evt.amount <= ctx.limit` first). `withdraw` on `review` is ancestor-sourced; `manager_review` declares its own `withdraw` back to `draft`. Invariant `ctx.total >= 0.00` is enforced.

```
$ fsm validate examples/expense_approval.json
created:    true
dry_run:    true
$ fsm machine add examples/expense_approval.json
created: true
$ fsm instance new expense_approval --request-id e1
leaf: draft
$ fsm instance send inst-e1 submit --payload '{"amount":"-1.00"}' --request-id e1-bad
# exit 1
run/invariant
hint: adjust the action or invariant nonneg
$ fsm instance send inst-e1 submit --payload '{"amount":"10.00"}' --request-id e1-submit
leaf: peer_review
$ fsm instance send inst-e1 approve --request-id e1-approve
leaf: approved
```

## order_lifecycle

Intent: emit a confirmation effect on entering fulfilment, stamp `confirmed{at}`, and keep acknowledgement as outbox truth rather than a gate.

Entering `fulfilment` emits `request_confirmation`. `note_added` is internal. Ancestor `cancel` reaches `cancelled`.

```
$ fsm validate examples/order_lifecycle.json
created:    true
dry_run:    true
$ fsm machine add examples/order_lifecycle.json
created: true
$ fsm instance new order_lifecycle --request-id ol1
leaf: placed
$ fsm instance send inst-ol1 place --request-id ol-place
leaf: picking
$ fsm instance show inst-ol1
effects_pending: inst-ol1/3/0
$ fsm instance ack inst-ol1 inst-ol1/3/0 --outcome ok --request-id ol-ack
effects_pending:
$ fsm instance send inst-ol1 confirmed --payload '{"at":"1"}' --request-id ol-early
# exit 1
run/unhandled
hint: add a transition or send a handled event
$ fsm instance send inst-ol1 note_added --payload '{"text":"hold"}' --request-id ol-note
leaf: picking
$ fsm instance send inst-ol1 pick --request-id ol-pick
$ fsm instance send inst-ol1 ship --request-id ol-ship
$ fsm instance send inst-ol1 confirmed --stamp at --request-id ol-conf
leaf: closed
```

## invoice_matching

Intent: accumulate exact decimals and match inside a tolerance band using `abs` and `div(..., 4, half_even)`.

```
$ fsm validate examples/invoice_matching.json
created:    true
dry_run:    true
$ fsm machine add examples/invoice_matching.json
created: true
$ fsm instance new invoice_matching --request-id inv1
leaf: open
$ fsm instance send inst-inv1 receive --payload '{"amount":"40.00"}' --request-id inv-r1
$ fsm instance send inst-inv1 match --request-id inv-m1
# exit 1
run/not_enabled
hint: adjust the payload or add a child override
$ fsm instance send inst-inv1 receive --payload '{"amount":"60.00"}' --request-id inv-r2
$ fsm instance send inst-inv1 match --request-id inv-m2
leaf: matched
```

## order_lifecycle.handlers (executor table)

`order_lifecycle` has a companion handler table,
`examples/order_lifecycle.handlers.json`, for the `fsm execute` loop. When the
instance enters `fulfilment`, the machine emits the `request_confirmation`
effect; the table maps that effect to a supplier-notification subprocess and
names the advance event (`pick`) plus the failure event (`cancel`).

Both events are ones `picking` accepts, which is the part of a table that is
easy to get wrong: an `on_ok` naming an event the instance's state does not
handle is journalled as an ack with no advance, and the executor holds the
advance until the instance reaches a state that takes it. `request_confirmation`
declares no fields, so the table's `argv` carries no `{placeholder}` — one
naming an argument the emit did not produce is `exec/config` at run time, and
`--check` cannot catch it because a table is validated on its own, without a
machine.

Run it unattended with:

```
$ fsm execute --check --handlers examples/order_lifecycle.handlers.json
ok:           true
$ fsm execute --data-dir ./data --handlers examples/order_lifecycle.handlers.json
```

`examples/case_review.handlers.json` is the other committed table, and it is
**illustrative rather than runnable**: it shows retry, the `mcp` handler kind
with a templated `arguments` object, and both concurrency caps, against effect
names no example machine declares. Read it for the format; do not point
`fsm execute` at it and expect work.

See the *Executing workflows* section of [EMBEDDING.md](EMBEDDING.md#executing-workflows)
for the full `fsm.handlers/1` format and the three run modes.

## Machine cases

Every example above shows what a machine *does*. A case file states what it
**should** do, and turns a change that breaks it into a failing command rather
than a discovery in production. Three case files are committed beside the
machines they test:

| case file | what it exercises |
|---|---|
| `examples/expense_approval.cases.json` | `send`, a context override, and a partial `expect` |
| `examples/order_lifecycle.cases.json` | `ack`, and that an ack moves nothing |
| `examples/parallel_review_deadline.cases.json` | `poll` at an explicit time |

No committed example machine declares both an effect and a deadline, so all
three script steps are exercised across three files rather than crammed into
one machine that would exist only to hold them.

A case file is a `fsm.cases/1` document. This is the whole of
`examples/expense_approval.cases.json`'s third case:

```json
{
  "name": "a_raised_limit_moves_the_boundary",
  "context": {"limit": "1000.00"},
  "script": [
    {"send": "submit", "payload": {"amount": "900.00"}}
  ],
  "expect": {
    "configuration": ["peer_review"],
    "context": {"total": "900.00"}
  }
}
```

`context` overrides the machine's declared initial values before creation.
Each `expect` field asserts **only itself**: this case says nothing about
`enabled`, `effects`, or `terminal`, and stays true when they change.

Run them:

```
$ fsm machine test examples/expense_approval.json --cases examples/expense_approval.cases.json
machine test — expense_approval
  ok   an_amount_within_the_limit_goes_to_peer_review
  ok   an_amount_over_the_limit_goes_to_manager_review
  ok   a_raised_limit_moves_the_boundary
  ok   an_approval_from_peer_review_approves_and_finishes
  4 passed, 0 failed
$ fsm machine test examples/parallel_review_deadline.json --cases examples/parallel_review_deadline.cases.json
machine test — parallel_review_deadline
  ok   a_poll_before_the_deadline_changes_nothing
  ok   a_poll_at_the_deadline_times_the_review_out
  ok   an_approval_before_the_deadline_wins
  3 passed, 0 failed
```

The command opens **no store**, takes no lock, claims no `request_id`, and
writes nothing. It is a pure function of two files, so it is free to run in an
editor loop and in CI over a repository of definitions that has never held a
store.

### What a failure looks like

Success teaches nothing about the format. Take the first case, expect
`manager_review` where the machine goes to `peer_review`, and write the total
as `120.0` where the machine holds `120.00`:

```json
"expect": {"configuration": ["manager_review"], "context": {"total": "120.0"}}
```

Running `fsm machine test examples/expense_approval.json --cases broken.cases.json`
against that edit exits **1** and prints:

```text
machine test — expense_approval
  FAIL an_amount_within_the_limit_goes_to_peer_review
       configuration (compared as a set) at step 0: expected manager_review, found peer_review
       context.total (compared by key) at step 0: expected 120.0, found 120.00
  0 passed, 1 failed
```

Two things to read here. The report names the **field**, both values, and the
**step index**, so a ten-step script says where. And `120.0` is not `120.00`: a
decimal's scale is part of its value in this engine, exact arithmetic is the
reason, and a comparison that coerced one into the other would hide exactly the
change a case exists to catch.

The exit code tracks the result — zero when every case passes, non-zero when
any fails — so CI can use it directly.

### The `supersedes` delta

This is the reason to keep case files rather than write them once. A definition
that declares `supersedes` is claiming to be a corrected version of a specific
earlier machine, and the earlier machine's cases are what check that claim:

Run `fsm machine test new.json --cases old.cases.json --against old.json` and
the report looks like this:

```text
machine test --against — case_review
  unchanged a_scored_review_above_the_bar_is_approved
       mapped to: accepted
  changed   a_withdrawn_review_only_has_to_land_in_rejected
       configuration: was rejected, now withdrawn
  0 unchanged, 1 changed, 0 refused, 0 uncovered
  this is a report and never a gate: a corrected machine usually changes behaviour on purpose
```

Expected configurations are translated through the mapping the new definition
already declares, using **the same code** `fsm instance migrate` uses — so this
report cannot disagree with what a real migration would do. Each case is
`unchanged`, `changed` with the fields that moved, `refused` where the new
definition rejects a script the old one accepted, or `uncovered` where the
mapping has no entry for a state the case names. That last one is the same gap
`migrate --dry-run` reports for instances, met here *before* any instance moves.

**A completed run exits zero, whatever the deltas.** A corrected machine
usually changes behaviour on purpose; a rule forbidding that would be wrong,
and a gate with an override is a gate everyone overrides. Only an actual
failure to run — a definition that does not compile, a missing mapping — is an
error.

### Regenerating a case file

When a change to a machine is deliberate, `FSM_REGEN_FIXTURES=1` rewrites the
`expect` blocks that moved, this repository's established fixture idiom:

Running `FSM_REGEN_FIXTURES=1 fsm machine test machine.json --cases cases.json`
prints what it rewrote, so the terminal and the version-control diff say the
same thing:

```text
regenerated cases.json
  a_raised_limit_moves_the_boundary configuration: peer_review -> manager_review
```

It **refuses to run against a case file that is untracked or has uncommitted
modifications**, and that refusal is the point rather than a precaution: a case
file rewritten from the code agrees with the code by construction and proves
nothing at all. The only thing that makes it evidence again is a reviewer
reading the diff, and a rewrite that cannot be reviewed as a diff is a rewrite
that should not happen. It also never widens what a case asserts — a block
naming one field still names one field afterwards — and leaves a case that
*errored* alone, because writing an error into the file would encode the bug.

Without the variable, the command never writes.
