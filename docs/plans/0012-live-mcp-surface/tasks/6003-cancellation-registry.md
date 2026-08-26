---
id: cancellation-registry
title: "Cancellation Registry"
workstream: "0060"
kind: task
depends_on:
  - progress-notifications
  - subscription-registry
gated: false
touches:
  - crates/fsm-cli/src/mcp/cancel.rs
  - crates/fsm-cli/src/mcp/tools/dispatch.rs
  - crates/fsm-cli/src/mcp/tools/handlers/simulate.rs
  - crates/fsm-cli/src/mcp/tools/handlers/instance.rs
  - crates/fsm-cli/tests/mcp_cancel.rs
status: planned
merged_as: ""
---
# Cancellation Registry

`notifications/cancelled` is written to stderr and thrown away today. This task makes it do the two things it honestly can, and documents the third thing it cannot — because a capability that overpromises is worse than one that is absent.

**Steps:**

1. Create `crates/fsm-cli/src/mcp/cancel.rs` holding `pub struct Cancellations { ids: Arc<Mutex<BTreeSet<Value>>> }` with `cancel`, `is_cancelled`, and `clear`, keyed by the JSON-RPC request id.
2. Fill the `notifications/cancelled` body `5702` already routed to this module: insert the id into the registry. Keep a stderr line as well, at debug level through `6001`'s logger, so an operator can still see it. The routing exists; do **not** edit `serve.rs`.
3. **Effect one — pre-dispatch.** Before executing a tool call, check whether its id is already cancelled and, if so, do not execute it. This is genuinely reachable: a client can cancel request 7 while the server is still working on request 6.
4. **Effect two — coarse boundaries.** Carry a `CancelFlag` on the `ToolCtx` that `5702` threaded into `dispatch`, and check it between events in `simulate` (`handlers/simulate.rs`) and between chunks in `instance_history` (`handlers/instance.rs`) — the same two loops `6002` reports progress from. A cancelled call returns a **tool error** carrying `req/cancelled`, not a JSON-RPC error, because the call was dispatched and the outcome is a tool outcome.
5. **Send no response for a request that was never executed.** Per the specification, a cancelled-before-dispatch request gets nothing, and the `notifications/cancelled` itself never gets a response. Pin both in the golden — this is the rule most likely to be implemented as a courtesy reply.
6. Clear an id from the registry once it has been consumed, so a client reusing an id later is not silently cancelled by a stale entry.
7. Add `req/cancelled` to the CLI's tool-error vocabulary and document, in the module doc, the limit this plan accepts: **a single `step` is not interruptible.** Engine operations are bounded by the evaluation budget and are short by construction; threading a cancellation token through the pure core would cost the core its purity and buy nothing. `6103` states the same limit to users.

**Tests:**

- `crates/fsm-cli/tests/mcp_cancel.rs`: a `notifications/cancelled` for a not-yet-dispatched id causes that request to be skipped with **no** response written.
- The `notifications/cancelled` itself never produces a response.
- A long `simulate` cancelled at a coarse boundary returns a tool error carrying `req/cancelled`, and the events after the cancellation point are not simulated.
- A cancelled `instance_history` behaves the same way between chunks.
- A cancellation for an unknown id is accepted silently and affects nothing.
- An id is cleared after use: cancelling id 5, letting it be skipped, then sending a new request with id 5 executes normally.
- A cancelled call writes **nothing** to the store — assert the journal length is unchanged.
- A session that sends no cancellations produces a byte-identical transcript to the pre-plan build.
- A `step`-only tool such as `instance_send` is not interrupted mid-call, and the test asserts it completes — pinning the documented limit rather than leaving it implicit.

- **Done when:** `cargo test -p fsm-cli --test mcp_cancel` passes every case above including the no-response rules and the documented non-interruption of a single step, `req/cancelled` is returned as a tool error, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
