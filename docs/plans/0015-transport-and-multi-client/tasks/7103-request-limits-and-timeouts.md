---
id: request-limits-and-timeouts
title: "Request Limits And Timeouts"
workstream: "0071"
kind: task
depends_on:
  - bearer-token-auth
  - http-request-parsing
  - http-response-writing
gated: false
touches:
  - crates/fsm-cli/src/http/server.rs
  - crates/fsm-cli/tests/http_limits.rs
status: planned
merged_as: ""
---
# Request Limits And Timeouts

Every bound `6902` defined has to be enforced where it is cheapest — before authentication where possible, and always before the engine — so that a stranger's traffic costs a thread and a timeout and nothing else.

**Steps:**

1. In `crates/fsm-cli/src/http/server.rs`, enforce the `6902` bounds at the earliest point each can be checked: the request-line and header bounds before any header interpretation, and the body bound from `Content-Length` **before** reading a single body byte.
2. Order the pipeline explicitly and comment the order: connection cap → read timeout armed → request line and header bounds → `Origin` → `Authorization` → `Content-Length` bound → body read → session lookup → dispatch. Each stage refuses without doing the next stage's work, and the ordering is the whole cost argument.
3. Enforce a per-connection request cap so a keep-alive client cannot pin a thread by pipelining indefinitely, closing the connection after the cap with a final response rather than an abrupt reset.
4. Arm read and write timeouts on the socket at accept time so a slow-loris connection costs one thread for the timeout window and no more. Re-arm the read timeout between keep-alive requests rather than only once.
5. Give an **SSE stream** a write timeout but no read timeout, since a stream is legitimately idle for minutes; a write timeout is what detects a client that stopped reading.
6. Return the documented status for each refusal — `414`, `431`, `413`, `408`, `503` — from `6903`'s error responses, with no internals in the body.
7. Add a total-request-size ceiling as a backstop over the sum of the request line, headers, and body, so an adversary cannot combine three individually-legal maxima into something larger than intended.

**Tests:**

- `crates/fsm-cli/tests/http_limits.rs`: each bound produces its documented status over a real socket, driven by raw bytes.
- An oversized `Content-Length` is refused **without** reading the body — assert by sending the header and no body and observing an immediate `413`.
- A request with a bad `Origin` and an oversized body returns `403`, proving origin validation precedes the body read.
- A request with a bad token and an oversized body returns `401` for the same reason.
- A slow-loris connection sending one byte per second is closed at the read timeout, and the server keeps serving.
- Keep-alive re-arms the read timeout: a connection idle between two requests is closed at the timeout rather than living forever.
- The per-connection request cap closes the connection after a final response, not with a reset.
- An SSE stream stays open through a long idle period, and a client that stops reading is detected at the write timeout.
- The total-request-size backstop refuses a request whose parts are individually legal but jointly oversized.
- No refusal path allocates a buffer sized from unvalidated client input — assert by review and pin with a case sending a `Content-Length` of `usize::MAX`.

- **Done when:** `cargo test -p fsm-cli --test http_limits` passes every case above, refusals happen in the documented order with no wasted work, the `Content-Length: usize::MAX` case allocates nothing, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
