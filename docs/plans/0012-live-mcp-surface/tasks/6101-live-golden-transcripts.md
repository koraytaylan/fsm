---
id: live-golden-transcripts
title: "Live Golden Transcripts"
workstream: "0061"
kind: task
depends_on:
  - list-changed-notifications
  - resource-links-in-tool-results
  - cancellation-registry
gated: false
touches:
  - crates/fsm-cli/tests/mcp_live_golden.rs
  - crates/fsm-cli/tests/fixtures/mcp_live/session.expected
status: planned
merged_as: ""
---
# Live Golden Transcripts

A push surface is only trustworthy if its whole stream is byte-compared, and the only way to byte-compare a timer-driven feed is to take the timer out of it.

**Steps:**

1. Create `crates/fsm-cli/tests/mcp_live_golden.rs` driving one full live session against a temp store with a `FixedClock`, byte-comparing the entire output stream against `fixtures/mcp_live/session.expected`.
2. The session, in order: `initialize` with the new capabilities → `notifications/initialized` → `resources/subscribe` on an instance URI → a write that advances that instance → the resulting `notifications/resources/updated` → `resources/read` of the instance URI → `logging/setLevel` to `debug` → a `simulate` call carrying a `progressToken` and its progress notifications → a `notifications/cancelled` for a not-yet-dispatched id and the silent skip that follows → `resources/unsubscribe` → EOF.
3. Drive the feed through `5902`'s `poll_once` seam rather than by sleeping. The golden must be deterministic and the suite must not spend wall time; a feed driven by an injected trigger produces exactly the same bytes as one driven by its timer.
4. Add exactly **one** timing-tolerant test alongside it that starts the real feed, makes a write, and asserts a notification arrives at all — never when, never how many. That test proves the timer is wired; the golden proves the bytes are right. Keep the two concerns apart.
5. Hand-derive the expected file from the architecture and the specification rather than from a first run's output. A golden captured from the implementation tests that the implementation is self-consistent, which is not the property anyone wants.
6. Assert the whole stream, not selected lines: every response, every notification, in order, with nothing extra. An assertion that permits unexpected extra lines cannot catch the failure this suite exists to catch.
7. Cover the two silences explicitly, since they are absences and absences are what goldens are worst at: no response for the cancelled request, and no response for either notification.

**Tests:**

- The byte-comparison itself is the test: the full session stream equals the committed fixture.
- The timing-tolerant companion test observes at least one notification from a real feed after a real write.
- Re-running the golden twice produces identical bytes, and running it on all three CI operating systems produces the same bytes — no path, line ending, or timestamp leaks into the stream.
- The fixture contains no absolute path, temp directory, pid, or wall-clock timestamp.
- Removing any single notification from the implementation makes the golden fail — verify during development for the `resources/updated` line, then restore.
- A session that subscribes to nothing produces the pre-plan transcript apart from the `initialize` line, asserted against a second committed fixture.

- **Done when:** `cargo test -p fsm-cli --test mcp_live_golden` byte-compares a full live session including notifications, progress, and both silences, the feed is driven deterministically with one separate timing-tolerant test, the fixture is hand-derived, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
