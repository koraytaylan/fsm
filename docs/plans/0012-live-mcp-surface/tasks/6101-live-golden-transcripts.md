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
  - crates/fsm-cli/tests/fixtures/mcp_live/quiet.expected
  - crates/fsm-cli/src/mcp/watch.rs
  - crates/fsm-cli/src/mcp/serve.rs
  - crates/fsm-cli/src/mcp/notify.rs
status: done
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

**Landed:** The injected trigger step 3 asks for is `watch::ByHand`: a guard that arms hand-driving for **its own thread**, so a session started on that thread parks its feed instead of spawning a poller and the caller runs the pass itself. Per-thread rather than global because the test binary runs its tests concurrently, and a global switch is one test reaching into another. The session's bookkeeping is unchanged — `FeedHandle::parked()` is tracked and shut down exactly like a spawned one — so a hand-driven session is the same session, which is the whole point of driving it by hand.

Lines reach the server through a `Scripted` reader that runs a hook after each line is answered. The server is blocked reading when the hook runs, so whatever the pass emits lands between two responses, in one place, every run. The hook runs after *every* line, not only the write: a feed that spoke twice about one change would appear in the golden as an extra line.

The session is the one step 2 lists, and the stream it produces is twelve lines: the `initialize` result, the subscribe result, the write's result, the `resources/updated` the write caused, the resource read, the level change, two progress notifications and the simulate result, two `debug` log lines for the arriving cancellation and the skip it caused, and the unsubscribe result. Id 7 is answered by silence, and so are both notifications.

**Corrections.**

- *A byte-exact fixture cannot be hand-derived, but the property step 5 is protecting can be.* Instance reports carry state hashes; nobody derives a sha256 by hand, and a fixture that omitted them would compare less than the whole stream. So the fixture is generated (`REGEN_MCP_LIVE=1`, following `mcp_full`'s house pattern) **and** the test carries an independently hand-written expectation of the entire stream's shape — every line, in order, by method or by the id it answers — asserted before the bytes are. A fixture that drifted to match a wrong implementation still fails, which is the property step 5 wants. It earned its keep during development: suppressing the `resources/updated` notification failed the shape assertion first, ahead of the byte compare.
- *`touches` was three files short.* The injected trigger is production code by construction — the seam has to be where the feed is spawned — so `watch.rs`, `serve.rs` and `notify.rs` carry it, and the quiet session's fixture is a second file.
- *The three-operating-system claim is asserted by proxy.* This host runs one; what the suite can check, it checks — no absolute path, no temp directory, no `\r`, and no ISO-8601 instant anywhere in the fixture (the negotiated `2025-06-18` is a protocol version, not a timestamp, and the test says so). CI runs the other two.
- *"The pre-plan transcript apart from the `initialize` line" is asserted as a property, not against a pre-plan file.* A session that subscribes to nothing is compared to its own committed fixture and asserted to contain no `notifications/` line at all, and both fixtures are asserted to share one identical `initialize` result. Diffing against a file from before the plan would pin the old capabilities, which is a claim this plan deliberately falsifies.
