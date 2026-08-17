Load any example with `fsm machine add examples/<name>.json`. The grammar lives at `fsm://docs/spec`.

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
effects_pending:
$ fsm instance ack inst-ol1 inst-ol1/3/0 --outcome ok --request-id ol-ack
effects_pending:
$ fsm instance send inst-ol1 confirmed --payload '{"at":"1"}' --request-id ol-early
# exit 1
run/unhandled
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
$ fsm instance send inst-inv1 receive --payload '{"amount":"60.00"}' --request-id inv-r2
$ fsm instance send inst-inv1 match --request-id inv-m2
leaf: matched
```
