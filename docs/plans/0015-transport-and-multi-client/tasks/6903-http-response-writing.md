---
id: http-response-writing
title: "HTTP Response Writing"
workstream: "0069"
kind: task
depends_on:
  - http-server-core
gated: false
touches:
  - crates/fsm-cli/src/http/response.rs
  - crates/fsm-cli/tests/http_response_write.rs
status: planned
merged_as: ""
---
# HTTP Response Writing

The streaming writer is the whole reason this file is separate: an SSE stream has no `Content-Length` and must flush per event, and that is the one response shape a naive writer gets wrong.

**Steps:**

1. In `crates/fsm-cli/src/http/response.rs`, implement the buffered response writer: status line, headers, blank line, body. Every response carries `Content-Type`, and every response with a body carries `Content-Length`.
2. Implement the **streaming** writer for SSE: no `Content-Length`, `Content-Type: text/event-stream`, `Cache-Control: no-cache`, `Connection: keep-alive`, and an explicit flush after **every** event. An event that sits in a buffer is an event that did not happen, and this is the failure mode that makes a live surface feel broken.
3. Implement `impl Write for StreamWriter` so plan 0012's `Notifier` can hold it as its `Box<dyn Write + Send>` with **no change above the transport**. That substitutability is the point of the whole design; if the notifier needs editing, something is wrong here.
4. Frame SSE correctly: `id:` line, `data:` line, blank line terminator, with a data payload that never contains a bare newline — the canonical JSON serializer already guarantees that and `send_line`'s existing `debug_assert` documents it.
5. Emit a keep-alive comment line (`: keepalive`) on an interval, driven by the caller rather than by a timer inside this module, so the writer stays free of a clock.
6. Provide the standard error responses used across the plan — `400`, `401`, `403`, `404`, `405`, `408`, `409`, `411`, `413`, `414`, `431`, `500`, `503` — each with a short plain-text body and no server internals. A stranger learns the status code and nothing else.
7. Never leak an internal error message, a path, or a backtrace into a response body.

**Tests:**

- `crates/fsm-cli/tests/http_response_write.rs`: a JSON response writes exactly the documented bytes, with correct `Content-Type` and `Content-Length`.
- A streaming response writes no `Content-Length` and the documented SSE headers.
- Each SSE event is flushed before the next is written — assert by observing a writer that records flush points.
- SSE framing is exact: `id:`, `data:`, blank line, byte-for-byte against a fixture.
- A `Notifier` constructed over a `StreamWriter` emits notifications in valid SSE framing with **no change** to the notifier itself.
- The keep-alive comment is written when the caller asks and never otherwise.
- Each error status writes its documented status line and a body free of paths, internal messages, and backtraces — assert by scanning for a temp-directory substring and for `panic`.
- A body containing multi-byte UTF-8 is written with a byte-correct `Content-Length`.
- A response to a `HEAD`-like path writes headers and no body if that path is supported, or `405` if it is not.

- **Done when:** `cargo test -p fsm-cli --test http_response_write` passes every case above, SSE framing byte-matches its fixture and flushes per event, a `Notifier` works unmodified over the stream writer, no error body leaks internals, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
