---
id: cancel-cascade-and-orphans
title: "Cancel Cascade And Orphans"
workstream: "0049"
kind: task
depends_on:
  - invocation-return-operation
gated: false
touches:
  - crates/fsm-store/src/store/instance/cancel.rs
  - crates/fsm-cli/src/cli/ops.rs
  - crates/fsm-store/tests/cancel_cascade.rs
status: planned
merged_as: ""
---
# Cancel Cascade And Orphans

A parent that walks away from a running child leaves work nobody will ever consume, and this task is the only place in the plan that writes two records for one request — so its crash window is documented and reconciled rather than denied.

**Steps:**

1. In `crates/fsm-store/src/store/instance/cancel.rs`, implement the parent-exit cascade: when an applied transition removed a slot that was `Running` (the flag `4802` sets), journal the child's `instance_cancelled` with `reason: "parent-exit:<parent_id>/<slot>"` in the same operation, immediately after the parent's own record.
2. Document the window in the code, precisely: a crash between the two records leaves a child `Running` with no parent slot. That is safe because the second record is a **cancellation** — idempotent and state-independent — so nothing is corrupt, only unreferenced, and step 4's sweep finishes it. Do not attempt a group commit; it would change the one-fsync-per-record durability claim for one recoverable window.
3. Implement the parent-cancel cascade: cancelling an instance cancels every `Running` child depth-first, bounded by `MAX_INVOKE_DEPTH`, each with the same reason string form. A child already `Completed` or `Cancelled` is skipped, not re-cancelled.
4. Implement orphan **reporting** at open and orphan **repair** as an explicit command. `fsm doctor` (in `crates/fsm-cli/src/cli/ops.rs`) reports every `Running` child whose parent slot is gone or whose parent is settled. `fsm repair --cancel-orphans` cancels them, one journaled `instance_cancelled` each with `reason: "orphan"`. An open must **never** write, so nothing here happens automatically — that rule is already in SPEC for `open_read_only` and applies with equal force to the writer path.
5. Confirm a child may be cancelled directly by any ordinary path, and that its parent then sees `outcome: "cancelled"` on return. Nothing about being a child removes an instance's ordinary operations.

**Tests:**

- `crates/fsm-store/tests/cancel_cascade.rs`: a parent transitioning out of an invoking state with a `Running` child journals the parent's record and the child's `instance_cancelled` with the parent-exit reason.
- Cancelling a depth-3 parent cancels all three descendants depth-first, each with its own record.
- A child already `Completed` is not re-cancelled by a parent cancel.
- The crash window: truncate the journal between the two records and reopen — the store is coherent, the child is `Running` and unreferenced, `doctor` reports exactly one orphan, and `repair --cancel-orphans` settles it with one record.
- `repair --cancel-orphans` on a clean store writes nothing and reports nothing.
- Opening a store with orphans writes **no** records — assert the journal length is unchanged across an open.
- A directly-cancelled child returns `outcome: "cancelled"` to its parent through `4902`.
- The cascade respects `MAX_INVOKE_DEPTH` and does not recurse without bound on a graph that admission already refused.

- **Done when:** `cargo test -p fsm-store --test cancel_cascade` passes every case above including the truncated-journal window and the no-write-on-open rule, `fsm doctor` reports orphans, `fsm repair --cancel-orphans` settles them, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
