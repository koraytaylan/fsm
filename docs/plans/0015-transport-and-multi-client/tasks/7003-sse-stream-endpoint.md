---
id: sse-stream-endpoint
title: "SSE Stream Endpoint"
workstream: "0070"
kind: task
depends_on:
  - post-endpoint
gated: false
touches:
  - crates/fsm-cli/src/http/endpoint.rs
  - crates/fsm-cli/src/http/sse.rs
  - crates/fsm-cli/src/http/endpoint.rs
  - crates/fsm-cli/tests/http_sse.rs
status: done
merged_as: ""
---
# SSE Stream Endpoint

This is where plan 0012's whole point survives the change of transport: the change feed and the notifier are untouched, and they write into a socket instead of a pipe.

**Steps:**

1. Create `crates/fsm-cli/src/http/sse.rs` and route GET on the endpoint path to it. Require `Accept: text/event-stream`; a GET without it is `406`.
2. Construct a `Notifier` over `6903`'s `StreamWriter` and hand it to the session, so plan 0012's change feed and every server-initiated message flow through **unmodified** code. If this task edits `notify.rs` or `watch.rs`, the abstraction has failed and the right fix is here, not there.
3. Allow **at most one** GET stream per session; a second is `409`. Two streams would split notification ordering with nothing to reassemble it, and a client that wants two should open two sessions.
4. Emit a `: keepalive` comment every 15 seconds so an idle proxy does not close the connection, driven from the stream loop rather than from a timer inside the writer.
5. Detect client disconnect on a write failure, stop the session's change feed, release the stream slot, and leave the **session** alive — a client that reconnects with the same `Mcp-Session-Id` gets its subscriptions back, which is exactly what `7004`'s resumability builds on.
6. Assign a monotonic `id` to every event on the stream and retain it in a bounded replay buffer for `7004`. Assign ids here so the buffer and the wire agree by construction rather than by two pieces of code staying in step.
7. Close the stream cleanly on session `DELETE` and on shutdown, without writing a farewell event — there is nothing to say and the client may already be gone.

**Tests:**

- `crates/fsm-cli/tests/http_sse.rs`: a GET with the right `Accept` opens a stream; a subscription notification arrives on it in valid SSE framing.
- A GET without `Accept: text/event-stream` is `406`.
- A second GET for one session is `409`; opening one for a different session succeeds.
- A keep-alive comment appears on an idle stream at the documented interval.
- Client disconnect stops the change feed and frees the stream slot; a subsequent GET for the same session succeeds and the subscriptions are still registered.
- Event ids are monotonic across the stream and match the replay buffer's ids.
- Session `DELETE` closes the stream and writes no farewell event.
- **No change above the transport:** the notification bytes on the SSE `data:` line are byte-identical to the lines the stdio transport writes for the same events — assert against plan 0012's committed golden.
- A slow client that stops reading does not block other sessions — assert a second session's stream keeps receiving.

- **Done when:** `cargo test -p fsm-cli --test http_sse` passes every case above, notification payloads byte-match the stdio golden, `notify.rs` and `watch.rs` are unmodified by this task, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.

**Landed:** GET on the endpoint path opens the session's one stream, requiring `Accept: text/event-stream` and refusing a second with `409` — two streams would split notification ordering with nothing to reassemble it, and a client that wants two can open two sessions for nothing. Event ids are assigned where the replay buffer records them, so the wire and the buffer agree by construction rather than by two pieces of code staying in step, and the buffer is bounded at 256 events with a resuming client told plainly when something older was dropped.

`notify.rs` and `watch.rs` are **unmodified**, which the task made the test of the whole design: a `Notifier` over a `SessionStream` writes the same bytes for the same event as one over stdout, asserted by producing both and comparing the payload.

A disconnect releases the stream slot and leaves the **session** alive, so a client that comes back gets its subscriptions back — which is what 7004 resumes on. A `DELETE` closes the stream and says nothing on the way out: there is nothing to say and the client may already be gone.

**Corrections.**

- *The gap flag is about the caller's position, not the buffer's history.* A client resuming from near the end missed nothing, and telling it otherwise would send it looking for events that were never lost.
