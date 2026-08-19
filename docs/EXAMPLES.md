Load any example with `fsm machine add examples/<name>.json`. The grammar lives at `fsm://docs/spec`.

## parallel_review_deadline

Intent: run review and audit concurrently while keeping the engine's
one-transition guarantee, and expire review only through an explicit poll.

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
