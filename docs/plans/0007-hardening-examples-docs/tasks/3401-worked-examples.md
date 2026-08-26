---
id: worked-examples
title: "Worked Examples"
workstream: "0034"
kind: task
depends_on: []
gated: false
touches:
  - examples/expense_approval.json
  - examples/order_lifecycle.json
  - examples/invoice_matching.json
  - crates/fsm-cli/tests/examples.rs
status: done
merged_as: ""
---
# Worked Examples

Three complete machines in neutral business-process domains each exercise a distinct engine capability — hierarchical review with conflict rules, effects with acknowledgement and stamped timestamps, and exact-decimal accumulation with tolerance guards — and every path shown to users is driven by a test first.

**Steps:**

1. Author `crates/fsm-cli/tests/examples.rs` first, encoding exactly the inventory under **Tests**.
2. Author `examples/expense_approval.json` per architecture: `draft` → compound `review` (`peer_review`/`manager_review` routed by a decimal-limit guard in document order), ancestor-sourced `withdraw`, a child-first override, and an enforced non-negativity invariant.
3. Author `examples/order_lifecycle.json` per architecture: compound `fulfilment` with an entry-block `request_confirmation` effect, `awaiting_confirmation`, the stamped `confirmed{at timestamp}` event, an internal `note_added` transition, and ancestor-sourced `cancel`.
4. Author `examples/invoice_matching.json` per architecture: decimal accumulation via `receive{amount}`, the tolerance-band matching guard using `abs`, and a `set` demonstrating explicit-scale `div(..., 4, half_even)`.

**Tests:**

- Validity, per machine in `examples.rs`: each JSON file loads and passes full definition validation with zero findings.
- `expense_approval`: a small amount routes to `peer_review` and a large one to `manager_review` (two runs pinning the document-order decimal-limit guard); a happy path reaches the `approved` terminal leaf; `withdraw` fires from a review child via the ancestor-sourced transition while the child-first override case still beats it where declared; the rejection path — a negative amount tripping the enforced non-negativity invariant → `run/invariant` with a non-empty `hint` naming the invariant.
- `order_lifecycle`: entering `fulfilment` emits `request_confirmation` into `effects_pending`; `effect_ack` empties it; the stamped `confirmed{at}` event (sent with `stamp`) carries the `FixedClock` value and advances to the terminal leaf; a variant run sends `confirmed` *without* acknowledging first — the instance still advances but the effect stays pending, pinning that acknowledgement is outbox truth, not a transition gate (the documented host flow is ack-then-advance); `note_added` is internal (leaf and entry counters untouched); the rejection path — `confirmed` sent from the initial state → `run/unhandled` with the hint listing the enabled events.
- `invoice_matching`: two `receive` events accumulate to a hand-computed exact decimal (the `div(..., 4, half_even)` set's value asserted digit-for-digit); a within-tolerance total passes the `abs`-guard into the `matched` terminal leaf; the rejection path — an over-tolerance total → `run/not_enabled` whose guard trace shows the `abs(...)` comparison bindings.

- **Done when:** `cargo test -p fsm-cli --test examples` drives every listed path for all three machines green — including the ack-is-not-a-gate variant and the digit-exact accumulation — and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
