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
ok: true
$ fsm machine add examples/parallel_review_deadline.json
created: true
$ FSM_CLOCK_MS=1000 fsm instance new parallel_review_deadline --request-id pr1
configuration: {"kind":"parallel","leaves":{"audit":"auditing","review":"awaiting_review"}}
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
$ fsm instance invoke inst-new-1 check --request-id inv-1
{"child_instance_id":"inst-a111dfb0920dcaf7dd51064b", ...}
$ fsm instance send inst-a111dfb0920dcaf7dd51064b clear --request-id child-1
$ fsm instance return inst-new-1 check --request-id ret-1
{"outcome":"completed", ...}
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
ok: true
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
ok: true
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
ok: true
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
ok: true
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
names the advance event (`confirmed`, with the `at` stamp from the ack) plus
the failure event (`cancel`). Run it unattended with:

```
$ fsm execute --check --handlers examples/order_lifecycle.handlers.json
ok: true
$ fsm execute --data-dir ./data --handlers examples/order_lifecycle.handlers.json
```

See the *Executing workflows* section of [EMBEDDING.md](EMBEDDING.md#executing-workflows)
for the full `fsm.handlers/1` format and the three run modes.
