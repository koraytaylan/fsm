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
status: planned
merged_as: ""
---
# Worked Examples

Three complete machines in neutral business-process domains each exercise a distinct engine capability — hierarchical review with conflict rules, effects with acknowledgement and stamped timestamps, and exact-decimal accumulation with tolerance guards — and every path shown to users is driven by a test first.

**Steps:**

1. Author `crates/fsm-cli/tests/examples.rs` first: for each of the three machines, validate the file, drive one happy path to a terminal state, drive one rejection path asserting the expected error code and a non-empty `hint`, and — for `order_lifecycle` — assert the emitted effect must be acknowledged via `effect_ack` before the `confirmed` domain event advances the instance.
2. Author `examples/expense_approval.json` per architecture: `draft` → compound `review` (`peer_review`/`manager_review` routed by a decimal-limit guard in document order), ancestor-sourced `withdraw`, a child-first override, and an enforced non-negativity invariant.
3. Author `examples/order_lifecycle.json` per architecture: compound `fulfilment` with an entry-block `request_confirmation` effect, `awaiting_confirmation`, the stamped `confirmed{at timestamp}` event, an internal `note_added` transition, and ancestor-sourced `cancel`.
4. Author `examples/invoice_matching.json` per architecture: decimal accumulation via `receive{amount}`, the tolerance-band matching guard using `abs`, and a `set` demonstrating explicit-scale `div(..., 4, half_even)`.

- **Done when:** `cargo test -p fsm-cli --test examples` drives happy and rejection paths for all three machines green (including the ack-before-advance assertion), and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
