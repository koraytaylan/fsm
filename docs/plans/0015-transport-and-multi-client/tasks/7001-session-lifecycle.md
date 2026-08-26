---
id: session-lifecycle
title: "Session Lifecycle"
workstream: "0070"
kind: task
depends_on:
  - http-request-parsing
  - http-response-writing
gated: false
touches:
  - crates/fsm-cli/src/http/session.rs
  - crates/fsm-cli/tests/http_session.rs
status: planned
merged_as: ""
---
# Session Lifecycle

Over stdio a session is the process; over HTTP it is a header, and everything plans 0012 and 0013 made per-session has to move into an object with a lifetime, an owner, and an expiry.

**Steps:**

1. Create `crates/fsm-cli/src/http/session.rs` with `pub struct Session` holding exactly what the stdio session held per client: the subscription set, the logging level, the cancellation set, the elicitation counter and outstanding ask, the negotiated protocol version, and the initialized flag.
2. Assign `Mcp-Session-Id` at `initialize` using architecture §0070's construction: `hex(sha256("fsm:session:1" || seed || counter || pid || nanos))[..32]`. **Do not reach for an RNG** — std has none, the workspace has zero dependencies, and `unsafe_code = "forbid"` rules out FFI to `getrandom` or `BCryptGenRandom`. Read `seed` once at server start from `/dev/urandom` where readable, and fall back to two `u64`s from `std::collections::hash_map::RandomState` plus the pid where it is not. Never a bare counter and never a bare timestamp: each of those alone is guessable.
3. Require the header on every subsequent request. A request without it is `400`; one naming an unknown or expired session is **`404`**, which is the code the specification assigns precisely so a client knows to re-initialize rather than retry.
4. Expire a session after 30 minutes idle, sweeping lazily on access rather than from a timer thread, and terminate one explicitly on `DELETE`. A `DELETE` for an unknown id is `404`; a `DELETE` for a live session closes its SSE stream and drops its state.
5. Cap concurrent sessions at `MAX_SESSIONS = 32`, refusing an `initialize` beyond it with `503`. Session state includes a bounded SSE replay buffer, so unbounded sessions are unbounded memory.
6. Validate `MCP-Protocol-Version` on every non-`initialize` request against the version negotiated at `initialize`; a mismatch is `400` naming both versions. An absent header is accepted and treated as the negotiated version, matching the specification's backwards-compatibility guidance.
7. Keep every piece of session state **per session**: two clients subscribing to the same instance each hold their own subscription and receive their own notification on their own stream. Nothing in this struct may be shared, and the shared `Store` lives elsewhere by design.

**Tests:**

- `crates/fsm-cli/tests/http_session.rs`: `initialize` returns an `Mcp-Session-Id`; a subsequent request carrying it succeeds.
- A request without the header is `400`; with an unknown id is `404`; after `DELETE` is `404`.
- Session ids are 32 hex characters, differ across 1000 initializations, and show no sequential or time-correlated structure — assert that consecutive ids share no common prefix beyond chance and that sorting them does not recover creation order.
- The seed is read **once** per server, not per session: assert `/dev/urandom` is opened at most once across 1000 initializations, via a counter or by observing that ids still differ when the path is made unreadable after start.
- On a platform where `/dev/urandom` is not readable, the fallback still produces 1000 distinct ids — exercise it by forcing the fallback path in a test, so Windows is covered on every platform's CI run.
- No `unsafe` and no new dependency: `cargo clippy --workspace -- -D warnings` is clean under `unsafe_code = "forbid"`, and `cargo test -p fsm-cli --test zero_deps` passes.
- A session expires after the idle window and then returns `404`.
- `DELETE` closes an open SSE stream for that session and drops its state.
- The 33rd concurrent session is refused `503` and the existing 32 keep working.
- `MCP-Protocol-Version` mismatching the negotiated version is `400` naming both; an absent header is accepted.
- Two sessions subscribing to the same instance hold independent subscription sets, and unsubscribing in one does not affect the other.
- A session's logging level and cancellation set are independent across sessions.

- **Done when:** `cargo test -p fsm-cli --test http_session` passes every case above, session ids use §0070's construction with no RNG, no dependency, and no `unsafe`, unknown sessions return `404` rather than `400`, per-session state is provably independent, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
