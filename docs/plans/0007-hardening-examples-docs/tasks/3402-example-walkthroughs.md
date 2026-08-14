---
id: example-walkthroughs
title: "Example Walkthroughs"
workstream: "0034"
kind: task
depends_on:
  - worked-examples
gated: false
touches:
  - docs/EXAMPLES.md
status: planned
merged_as: ""
---
# Example Walkthroughs

`docs/EXAMPLES.md` is both the human tutorial and the `fsm://docs/examples` resource the model reads, so each machine gets an intent statement, a feature-by-feature spec walkthrough, and one complete CLI transcript matching the tested flows.

**Steps:**

1. Replace the plan-0006 placeholder `docs/EXAMPLES.md` with a `## expense_approval`, `## order_lifecycle`, and `## invoice_matching` section, each stating the machine's intent and walking the spec (which engine features it demonstrates and why the guards/blocks are shaped that way).
2. Add one complete CLI transcript per machine — `fsm validate` → `fsm machine add` → `fsm instance new` → a happy `fsm instance send` → a deliberate rejection showing the rendered hint → the corrected send reaching a terminal state — mirroring exactly the flows `crates/fsm-cli/tests/examples.rs` drives.
3. Open the file with a short paragraph explaining how to load any example (`fsm machine add examples/<name>.json`) and pointing at `fsm://docs/spec` for the grammar.

- **Done when:** `docs/EXAMPLES.md` contains all three sections each with a full CLI transcript, the plan-0006 resources test still passes (it embeds this file), and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
