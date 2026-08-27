---
id: post-endpoint
title: "POST Endpoint"
workstream: "0070"
kind: task
depends_on:
  - session-lifecycle
gated: false
touches:
  - crates/fsm-cli/src/mcp/serve.rs
  - crates/fsm-cli/src/mcp/methods.rs
  - crates/fsm-cli/src/mcp/mod.rs
  - crates/fsm-cli/tests/http_server.rs
  - crates/fsm-cli/src/http/endpoint.rs
  - crates/fsm-cli/tests/http_post.rs
status: done
merged_as: ""
---
# POST Endpoint

The protocol loop already takes a reader and a writer, so this endpoint's job is routing and content negotiation — and choosing between a JSON response and a stream is a decision the handler makes, never one the transport guesses.

**Steps:**

1. Create `crates/fsm-cli/src/http/endpoint.rs` routing the configured path (default `/mcp`) by method: POST here, GET in `7003`, DELETE to `7001`'s termination, and anything else `405` with an `Allow` header.
2. Accept exactly **one** JSON-RPC message per POST body. Batching stays refused with the existing "batch requests are not supported" message, matching stdio and the `2025-06-18` revision that removed it.
3. Reply `202 Accepted` with **no body** to a notification or a response — there is nothing to say, and a body would invite a client to parse one.
4. Reply to a **request** with either `application/json` carrying the single response, or `text/event-stream` carrying any server-initiated messages followed by the response. The handler declares which it needs; the transport must not guess from the method name.
5. Use the stream form when handling may produce server-initiated messages before the response — an elicitation (plan 0013) or a progress-reporting call (plan 0012). Default to JSON everywhere else, because a stream for a call that produces one message is overhead a client has to unwrap.
6. Honour the client's `Accept` header: a client that does not accept `text/event-stream` and makes a call that requires it gets `406`, rather than a stream it cannot read.
7. Route an inbound **response** — a client answering a server request — to the waiting `request_and_await` from plan 0013, matching by JSON-RPC id within the session. Over stdio that response arrived as a line; here it arrives as a POST, and that is a routing difference and not a protocol one.
8. Reject a body that is not valid JSON with the same JSON-RPC parse-error response the stdio loop produces, carried in a `200` with a JSON-RPC error object — a malformed **JSON-RPC** message is a protocol-level error, not an HTTP-level one.

**Tests:**

- `crates/fsm-cli/tests/http_post.rs`: `initialize` over POST returns `application/json` with the response and an `Mcp-Session-Id`.
- A notification POST returns `202` with an empty body; a response POST likewise.
- A tool call returns `application/json` by default.
- A call carrying a `progressToken` returns `text/event-stream` with progress events followed by the response, framed correctly.
- A client whose `Accept` excludes `text/event-stream` making such a call gets `406`.
- A batch body is refused with the existing message.
- A malformed JSON body returns a JSON-RPC parse error inside a `200`, matching the stdio loop's error object byte-for-byte.
- An inbound response is routed to a waiting elicitation in the same session and completes it.
- An inbound response for a different session does not complete another session's wait.
- `PUT` and `PATCH` return `405` with an `Allow` header.
- A POST to an unconfigured path returns `404`.

- **Done when:** `cargo test -p fsm-cli --test http_post` passes every case above, `202` is returned for notifications and responses, the JSON/stream choice comes from the handler, inbound responses route to the right session's waiter, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.

**Landed:** One JSON-RPC message per POST, routed by shape. A notification or a response is `202` with **no** body, because there is nothing to say and a body would invite a client to parse one. A request is answered in JSON, or in an event stream when the handling may speak before it answers — decided from the request (a `progressToken`, or `instance_elicit`) rather than guessed from a method name, and refused with `406` when the client's `Accept` says it could not read the stream. Batching is refused in stdio's own words, and a malformed body is a JSON-RPC parse error inside a `200`, because the transport delivered exactly what was sent.

An inbound response reaches the question waiting for it through a per-session mailbox that `request_and_await` reads as a `BufRead` — so plan 0013's exchange is unchanged and the difference really is routing. One session's answer cannot complete another's ask, which is asserted rather than assumed.

**Corrections.**

- *`handle_request` and `Live` are now public, and `serve.rs` split at the seam it already had.* A second transport needs the protocol's per-request entry point, and reaching it was the whole "the transport and the protocol are separable" claim the architecture makes. Exposing it pushed `serve.rs` past the thousand-line ceiling — which 6702's notes predicted — so `methods.rs` holds what a request *means* and `serve.rs` keeps the loop: modes, startup, reading lines, the executor tick.
- *The connection-cap test was racing the accept loop, and racing it differently in release.* Sixty-four sockets that are merely queued in the listen backlog occupy no slot; each held connection now completes a request before the next is opened, so every slot is provably occupied when the sixty-fifth arrives.
