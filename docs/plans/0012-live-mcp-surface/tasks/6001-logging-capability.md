---
id: logging-capability
title: "Logging Capability"
workstream: "0060"
kind: task
depends_on:
  - capability-negotiation
gated: false
touches:
  - crates/fsm-cli/src/mcp/logging.rs
  - crates/fsm-cli/tests/mcp_logging.rs
  - crates/fsm-cli/src/mcp/serve.rs
  - crates/fsm-cli/tests/mcp_logging.rs
status: done
merged_as: ""
---
# Logging Capability

The embedded executor writes its tick lines to stderr, where the model driving the conversation cannot see them — which means the one mode designed to advance a workflow mid-conversation is also the one whose progress is invisible.

**Steps:**

1. Create `crates/fsm-cli/src/mcp/logging.rs` holding the per-session level (default `info`) and `pub fn message(&self, level: Level, logger: &str, data: Value)`, which drops below-threshold messages **before** serialization rather than after.
2. Fill the `logging/setLevel` body `5702` already routed to this module, taking `{level}` from the eight RFC-5424 names the specification uses — `debug`, `info`, `notice`, `warning`, `error`, `critical`, `alert`, `emergency` — returning an empty result. An unrecognised level is `INVALID_PARAMS` with a hint listing all eight. The routing exists; do **not** edit `serve.rs`.
3. Emit `notifications/message` with `{level, logger, data}`, `logger` naming the producer: `fsm.serve`, `fsm.store`, or `fsm.execute`.
4. Wire three producers: the startup mode line; store warnings that today reach stderr; and — the one that matters — the **embedded executor's tick lines**, which `drive_executor` currently sends to stderr only.
5. **Keep writing all three to stderr as well.** An operator reading a terminal must not lose output because a client attached, and the two audiences are different. Say so in a comment; a later reader will otherwise "clean up" the duplication.
6. Send nothing before `initialize` completes: a notification to a client that has not negotiated the capability is a protocol error. Buffer or drop pre-initialize messages — drop is correct here, since the only pre-initialize producer is the startup line, which stderr already has.
7. Keep `data` structured rather than pre-rendered: `{"effect": "...", "request_id": "...", "outcome": "ok"}` is actionable and a formatted sentence is not. Carry identifiers only, honouring plan 0008's rule that tick output holds no path, pid, or duration.

**Tests:**

- `crates/fsm-cli/tests/mcp_logging.rs`: `logging/setLevel` with each of the eight names succeeds; an unknown name is `INVALID_PARAMS` with all eight in the hint.
- A message below the current level produces no notification; raising the level to `debug` then produces it.
- The default level before any `setLevel` is `info`.
- An embedded-mode tick emits a `notifications/message` from logger `fsm.execute` **and** writes the same line to stderr.
- `data` is a structured object carrying identifiers only — assert no absolute path, pid, temp dir, or duration appears in any emitted message.
- No `notifications/message` is emitted before `initialize` completes.
- A session that never calls `setLevel` and triggers no producers emits no logging notifications, keeping its transcript byte-identical to the pre-plan build.
- Level changes take effect for the next message without restarting anything.

- **Done when:** `cargo test -p fsm-cli --test mcp_logging` passes every case above, embedded executor ticks reach both the client and stderr, nothing is emitted pre-initialize, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.

**Landed:** the eight levels with their severity order, `DEFAULT_LEVEL`, `message_params`, and a `message` that checks the threshold and the initialize state before it renders anything; the executor tick wired to both audiences; the refusal that lists every level; and the suite — all eight names accepted, an unknown one naming them, the default, a level taking effect immediately, silence before initialize, an embedded tick reaching the client with a structured `data` and no path, pid, or duration, and a quiet session staying quiet.

**Corrections.** (1) The level lives on the session's `Live` state rather than inside `logging.rs`, because `5702` already routed `logging/setLevel` there and a second home for the same value is a second answer to "what level is this session at". `message` takes it as a parameter, which is what lets the threshold be checked before the data is built. (2) `data` is built by a closure, so a dropped message never pays for its own contents — the point of checking the threshold first.
