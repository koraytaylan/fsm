---
id: session-shutdown
title: "Session Shutdown"
workstream: "0057"
kind: task
depends_on:
  - capability-negotiation
gated: false
touches:
  - crates/fsm-cli/src/mcp/notify.rs
  - crates/fsm-cli/src/mcp/serve.rs
  - crates/fsm-cli/tests/mcp_shutdown.rs
  - crates/fsm-cli/tests/mcp_shutdown.rs
status: done
merged_as: ""
---
# Session Shutdown

A background thread that outlives its session writes to a closed pipe from a process that has moved on, so the lifecycle is decided here — before anything is spawned — and the cheapest lifecycle is not spawning at all.

**Steps:**

1. In `crates/fsm-cli/src/mcp/notify.rs`, add `pub struct FeedHandle { stop: Arc<AtomicBool>, join: Option<JoinHandle<()>> }` with `stop_and_join(&mut self)`, and an `impl Drop` that calls it so no early return can leak the thread.
2. In `crates/fsm-cli/src/mcp/serve.rs`, hold an `Option<FeedHandle>` in the session. Set the stop flag and join on `Line::Eof`, on a fatal write error, and on every early return path.
3. **Spawn lazily.** The thread starts only when a session's first successful `resources/subscribe` arrives. A server nobody subscribes to spawns nothing, does no I/O between requests, and behaves exactly as it does today — which is what keeps every existing non-subscribing golden byte-identical and makes this plan inert for callers that do not use it.
4. Make the stop responsive: the feed sleeps in slices no longer than 25 ms and checks the flag between them, so shutdown never waits a full poll interval. A `sleep(250ms)` that ignores the flag turns every client disconnect into a quarter-second stall.
5. On a broken stream — `Notifier::is_broken` — the feed stops on its own without further writes and without a panic. The client is gone; the main loop will discover EOF.
6. Confirm no shutdown path writes to the protocol stream: a goodbye notification after the client closed stdout is a write to a closed pipe, and there is nothing to say anyway.

**Tests:**

- `crates/fsm-cli/tests/mcp_shutdown.rs`: a session that never subscribes spawns no thread — assert by observing that thread count is unchanged across the session, or by a test-only counter incremented on spawn.
- A session that subscribes and then reaches EOF joins the thread before returning; the test completes without hanging.
- Shutdown latency is bounded: after EOF the thread exits well inside one poll interval.
- Dropping the session without an explicit stop still joins the thread, via `Drop`.
- A write error on the notifier ends the feed without a panic and without further writes.
- An early return from an error path still joins.
- Two sequential sessions against the same data directory each spawn and join their own thread with no interference.
- A non-subscribing session's full transcript is byte-identical to the pre-plan build's, apart from the `initialize` line `5702` changed.

- **Done when:** `cargo test -p fsm-cli --test mcp_shutdown` passes every case above, no session leaks a thread on any exit path, a non-subscribing session spawns nothing, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.

**Landed:** `FeedHandle` with `spawn`, `stop_and_join`, and a `Drop` that calls it; `sleep_unless_stopped` with its 25 ms slices; the session's `Live::ensure_feed` and `Live::shutdown`, the first called only from the subscribe arm and the second from EOF; a process-wide spawn counter so "spawns nothing" is a claim a test can check; and the suite — no spawn without a subscription, one feed per session however many subscriptions, a bounded stop, `Drop` joining, a broken stream ending the feed, two sequential sessions not interfering, and a non-subscribing transcript that says nothing on its own.

**Corrections.** The feed spawned here runs a real loop rather than waiting for `5902` to spawn it: a lifecycle nothing exercises is a lifecycle nobody has tested, and every one of this task's own tests — lazy spawn, join at EOF, bounded shutdown, drop safety — needs a thread that actually exists. What `5902` adds is the poll in the middle of the loop, not the loop. The spawn counter is process-global, so every test in the file that spawns a feed takes one lock: a delta read while another test's thread starts is measuring the wrong thing, which release-mode scheduling found and debug-mode did not.
