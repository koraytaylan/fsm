---
id: notification-ordering-suite
title: "Notification Ordering Suite"
workstream: "0061"
kind: task
depends_on:
  - live-golden-transcripts
gated: false
touches:
  - crates/fsm-cli/tests/mcp_notification_ordering.rs
status: planned
merged_as: ""
---
# Notification Ordering Suite

The protocol does not order notifications against responses and this plan does not claim to, but it claims something narrower and absolute: no message's bytes ever appear inside another message's line.

**Steps:**

1. Create `crates/fsm-cli/tests/mcp_notification_ordering.rs`. Its central property: drive a response-producing loop while a second thread pushes notifications through a cloned `Notifier` into a shared buffer, then assert **every** line parses as a complete JSON-RPC message and the multiset of parsed messages equals exactly what was produced.
2. Run that property at pressure: at least four notifier threads, at least 500 messages each, with message sizes spanning small notifications and large tool results, so the interleaving window is genuinely exercised rather than nominally.
3. Assert the feed's de-duplication contract under load: a batch of records touching one instance yields one notification per affected URI, never one per record, even when polls and writes race.
4. Assert the watermark contract: across many polls interleaved with writes, no seq is ever reported twice and none is skipped.
5. Assert lifecycle under stress: the feed thread exits within one poll interval of EOF across repeated sessions, and a stdout closed mid-batch ends the thread without a panic and without further writes.
6. Assert the subscription boundary holds under concurrency: subscribing and unsubscribing while the feed polls never produces a notification for an unsubscribed URI, and never drops one for a URI subscribed before the write.
7. Keep the suite free of wall-clock assertions apart from the single bounded shutdown check, so it is not a flake generator on a loaded CI machine.

**Tests:**

- The interleaving property at pressure, as described, with no truncated or merged line.
- One notification per affected URI per batch, under racing writes and polls.
- No seq reported twice and none skipped, across at least 100 interleaved poll/write rounds.
- The feed exits within one poll interval of EOF, across 20 sequential sessions.
- A stdout closed mid-batch ends the feed with no panic and no further writes.
- Subscribing mid-poll: a URI subscribed before a write is notified; one subscribed after the write is not notified for that write.
- Unsubscribing mid-poll: no notification for the removed URI after the call returns.
- The suite is deterministic enough to pass 50 consecutive runs locally — report that in the commit message.

- **Done when:** `cargo test -p fsm-cli --test mcp_notification_ordering` passes every property above at pressure, the no-interleaving property holds across four threads and 2000 messages, the suite carries no wall-clock assertion beyond the bounded shutdown check, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
