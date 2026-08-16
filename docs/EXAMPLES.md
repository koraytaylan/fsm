Load any example with `fsm machine add examples/<name>.json`. The grammar lives at `fsm://docs/spec`.

## expense_approval

Intent: route an expense through peer or manager review using a decimal limit, with an ancestor `withdraw` and a child-first override.

The spec is a tree `draft` → compound `review` (`peer_review`, `manager_review`) → terminal `approved` / `refused`. `submit{amount decimal(2)}` uses document-order guards (`evt.amount <= ctx.limit` first). `withdraw` on `review` is ancestor-sourced; `manager_review` declares its own `withdraw` back to `draft`. Invariant `ctx.total >= 0.00` is enforced.

```
$ fsm validate examples/expense_approval.json
ok: true
$ fsm machine add examples/expense_approval.json
created: true
$ fsm instance new expense_approval
leaf: draft
$ fsm instance send <id> submit --payload '{"amount":"10.00"}'
leaf: peer_review
$ fsm instance send <id> submit --payload '{"amount":"-1.00"}'
# exit 1
run/invariant
  hint: …
$ fsm instance send <id> approve
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
$ fsm instance new order_lifecycle
leaf: placed
$ fsm instance send <id> place
leaf: picking
$ fsm instance send <id> confirmed --payload '{"at":"1"}'
# exit 1
run/unhandled
  hint: …
$ fsm instance send <id> pick
$ fsm instance send <id> ship
$ fsm instance send <id> confirmed --stamp at
leaf: closed
```

## invoice_matching

Intent: accumulate exact decimals and match inside a tolerance band using `abs` and `div(..., 4, half_even)`.

```
$ fsm validate examples/invoice_matching.json
ok: true
$ fsm machine add examples/invoice_matching.json
created: true
$ fsm instance new invoice_matching
leaf: open
$ fsm instance send <id> receive --payload '{"amount":"40.00"}'
$ fsm instance send <id> match
# exit 1
run/not_enabled
  hint: …
$ fsm instance send <id> receive --payload '{"amount":"60.00"}'
$ fsm instance send <id> match
leaf: matched
```
