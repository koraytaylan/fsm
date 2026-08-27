---
id: inbound-response-routing
title: "Inbound Response Routing"
workstream: "0064"
kind: task
depends_on:
  - completion-capability
gated: false
touches:
  - crates/fsm-core/src/error.rs
  - docs/SPEC.md
  - crates/fsm-cli/tests/naive_caller/infra_support.rs
  - crates/fsm-cli/tests/naive_caller/tool_outcomes.rs
  - crates/fsm-cli/src/mcp/jsonrpc.rs
  - crates/fsm-cli/src/mcp/elicit.rs
  - crates/fsm-cli/tests/mcp_inbound_responses.rs
status: done
merged_as: ""
---
# Inbound Response Routing

The serve loop has never parsed a response, because the server has never sent a request; elicitation makes it a requester, and this is the structural change that has to land before anything can ask a client a question.

**Steps:**

1. Add `Incoming::Response { id, result, error }` to `crates/fsm-cli/src/mcp/jsonrpc.rs` and teach `parse_line` to recognise it: a message carrying an `id` and either `result` or `error`, and **no** `method`. A message with both a `method` and a `result` is malformed and stays `WireError::Invalid`.
2. Create `crates/fsm-cli/src/mcp/elicit.rs` with `pub fn request_and_await(io: &mut SessionIo<'_>, method: &str, params: Value, clock: &mut dyn Clock) -> Result<Value, ErrorObj>`.
3. Generate server-side request ids from a monotonic per-session counter with the prefix `fsm-elicit-`, so a server id can never collide with a client id no matter what the client chooses.
4. Write the request through the `Notifier`, then read lines from the same input until a **response with the matching id** arrives.
5. **Handle client requests that arrive while waiting**, by re-entering the request handler at nesting depth 1. A client is not obliged to stop working because the server asked a question, and a server that ignored inbound requests would deadlock a well-behaved client. Handle inbound notifications normally too.
6. **Cap nesting at 1.** A second `request_and_await` while one is outstanding returns an error immediately. A recursive ask is a design mistake, and a cap turns it into a diagnosable one instead of a stack.
7. Implement a **timeout** read from the injected clock, defaulting to 300 seconds, returning an error naming the timeout. Honour a `notifications/cancelled` naming the outstanding server request id the same way. A server that waits forever for a client that will never answer is a hung server.
8. Treat EOF while waiting as a normal session end — the client is gone and there is nothing to answer. Treat a response carrying `error` as a returned error, not a panic.
9. Discard a response whose id matches nothing outstanding, with a debug-level log line. It is a client bug, and dropping it is strictly better than failing the session.

**Tests:**

- `crates/fsm-cli/tests/mcp_inbound_responses.rs`: `parse_line` recognises a result response, an error response, and rejects a message carrying both `method` and `result`.
- `request_and_await` writes the request and returns the matching response's result.
- A client request arriving **before** the awaited response is handled and answered, and the awaited response is still returned afterwards — assert both outputs appear in the right order.
- A client notification arriving while waiting is handled and does not disturb the wait.
- A response with a non-matching id is discarded and the wait continues.
- Nesting: a second `request_and_await` while one is outstanding returns an error without writing a second request.
- Timeout: with a `FixedClock` advanced past the limit, the call returns the timeout error and writes nothing further.
- A `notifications/cancelled` naming the outstanding server id ends the wait with a cancellation error.
- EOF while waiting ends the session cleanly with no panic.
- A response carrying `error` is returned as an error rather than panicking.
- Server-generated ids are monotonic and carry the `fsm-elicit-` prefix; a client using the same literal ids does not collide.

- **Done when:** `cargo test -p fsm-cli --test mcp_inbound_responses` passes every case above including the interleaved-client-request case, the nesting cap, and the timeout, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.

**Landed:** `Incoming::Response` is a message with an id and a `result` or an `error` and no `method`; a message carrying both a method and a result is `Invalid`, because guessing which the sender meant is how a protocol loop starts inventing semantics. A response arriving when nothing is waiting is dropped with a `debug` line rather than ending a working session.

`request_and_await` writes one request under an `fsm-elicit-N` id — monotonic, and prefixed so no client id can collide — and reads until its answer. While it waits the client keeps working: notifications are handled, a `notifications/cancelled` naming the outstanding id ends the wait, and inbound requests are **answered**. The nesting cap is structural: the session's halves are borrowed for the whole exchange, so a second ask cannot take them, and `ask` turns the failed borrow into `req/elicit_nested` rather than a panic.

**Corrections.**

- *A client request arriving mid-wait cannot be re-entered into the request handler.* The tool that asked the question is holding `&mut Store`; re-entering would need it again, which does not typecheck and would not be sound if it did. What lands instead answers what is answerable without the store — `ping`, `tools/list`, `prompts/list`, `resources/templates/list` — and answers everything else with `-32004` and "retry after answering it". That keeps the deadlock closed, which is the reason step 5 exists: the client is never left waiting for a response it will not get.
- *The timeout bounds a talking client, not a silent one.* It is checked before each read, so a client that sends nothing leaves the loop blocked in `read_line` until the transport closes. Bounding silence needs a reader that can be woken, which stdio does not portably provide. The module doc says so rather than implying otherwise.
- *Three codes are registered here and allowlisted until 6403.* `req/elicit_timeout`, `req/elicit_nested` and `req/elicit_failed` are returned by this task's code but reachable by a caller only through `instance_elicit`. Both every-code gates carry an entry naming 6403 as the task that makes them reachable and removes the entry — the precedent plan 0011 set for exactly this.
- *The suite serialises its id predictions.* Ids are monotonic per process and the tests run concurrently, so predicting the next one means holding a lock while it is taken and used.
