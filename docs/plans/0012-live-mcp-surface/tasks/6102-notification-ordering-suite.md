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
status: done
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

**Landed:** Eight properties, one wall-clock assertion. The pressure test puts four notifier threads and a response producer on **one** `Notifier` — the production arrangement, since one stream has one writer and everything else clones a handle — and pushes 2,500 messages sized from a bare notification to 48 KB. Every line parses whole, and the multiset out equals the multiset in.

The feed's contracts are asserted per pass, which is where they are decided: ten records touching two instances produce two notifications, not ten; forty rounds of write-then-poll never name one URI twice in one pass; a hundred rounds show a pass reporting exactly one notification when it has records and none when it has not, with the watermark monotone and ending exactly at `last_seq` — "none reported twice, none skipped" stated as something a test can see.

The boundary is checked twice, deterministically and concurrently. Hand-driven: subscribed-before-the-write is notified, subscribed-after is not notified for that write, unsubscribed-before is silent. With a real feed thread running at 25 ms beside 40 writes and 40 subscription toggles: a URI nobody ever subscribed to is never named, and every line is still whole.

A closed stream ends the feed with no panic, nothing further written, and the unreported batch still unreported — the watermark stays put, so the loss is recoverable rather than silent. A watchdog thread bounds that test so a regression fails instead of hanging.

**Fifty consecutive local runs: 50 passed, 0 failed.**

**Corrections.**

- *Step 6's concurrent unsubscribe claim is not sound as written, so the suite asserts the sound half.* The watched set is read once per pass, so a pass already in flight when `unsubscribe` returns can still name the URI it read a moment earlier — that is the design, and no lock the feed could take would change it without making the feed perturb the writer it promises not to perturb. What holds under concurrency is that a URI never subscribed is never named; what holds per pass is the full boundary, and the deterministic test says so pass by pass.
- *The fixture machine takes one event that always applies.* Alternating two events means tracking each instance's state in the test, and a suite that mis-tracks it fails on the store's refusal rather than on the property under test.
- *Two tests share a mutex.* `feeds_spawned` is per-process, so a test counting spawns and a test starting a session would otherwise count each other's threads.
