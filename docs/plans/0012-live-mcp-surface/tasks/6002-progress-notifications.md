---
id: progress-notifications
title: "Progress Notifications"
workstream: "0060"
kind: task
depends_on:
  - logging-capability
  - resource-links-in-tool-results
gated: false
touches:
  - crates/fsm-cli/src/mcp/progress.rs
  - crates/fsm-cli/src/mcp/tools/dispatch.rs
  - crates/fsm-cli/src/mcp/tools/handlers/simulate.rs
  - crates/fsm-cli/src/mcp/tools/handlers/instance.rs
  - crates/fsm-cli/tests/mcp_progress.rs
status: planned
merged_as: ""
---
# Progress Notifications

A call that takes a while and says nothing is indistinguishable from a hung server, and this plan's real beneficiary is plan 0014's journal verify — so the reporter is built now, with the two honest consumers that exist today.

**Steps:**

1. Create `crates/fsm-cli/src/mcp/progress.rs` with `pub struct ProgressReporter { token: Option<Value>, notifier: Notifier, last_ms: Cell<i64>, clock: ... }` and `pub fn report(&self, progress: u64, total: Option<u64>, message: Option<&str>)`.
2. In `crates/fsm-cli/src/mcp/tools/dispatch.rs`, read `_meta.progressToken` from the `ToolCtx` that `5702` threaded in, building a live reporter when it is present and a **discarding** reporter when it is absent, so every call site reports unconditionally and no handler needs an `if`.
3. Emit `notifications/progress` with `{progressToken, progress, total?, message?}`. Include `total` whenever the work has a known size, since a progress bar without a denominator is barely better than silence.
4. Rate-limit to at most one report per 100 ms of wall time **and always emit the final one**, so a fast call produces one notification rather than a thousand and a slow one still ends cleanly. Read time through the injected clock, never `Instant::now()`, so tests are deterministic.
5. Wire the two consumers that genuinely take time today: `simulate` reports once per event in `crates/fsm-cli/src/mcp/tools/handlers/simulate.rs`, and `instance_history` reports once per chunk in `crates/fsm-cli/src/mcp/tools/handlers/instance.rs`. Wire no others — a report on a call that returns in a microsecond is noise, and this plan does not add reports it cannot justify.
6. Leave the reporter's constructor public so plan 0014's `journal_verify` can use it without restructuring anything.
7. Emit nothing when the token is absent: the discarding reporter must produce **zero** notifications, which is what keeps every existing golden byte-identical.

**Tests:**

- `crates/fsm-cli/tests/mcp_progress.rs`: a `simulate` call carrying a `progressToken` emits progress notifications carrying that exact token, with a final report whose `progress` equals `total`.
- The same call **without** a token emits none, and its transcript byte-matches the pre-plan golden.
- Rate limiting: a 50-event simulate under a clock advancing 1 ms per event emits far fewer than 50 reports, and the last one is always present.
- A single-event simulate emits exactly one report.
- `total` is present for both consumers, since both know their size up front.
- The token is echoed verbatim, including a string token and a numeric one, since the specification permits both.
- `instance_history` with a large page reports per chunk and finishes with `progress == total`.
- No other tool emits progress — assert across a session exercising every tool with a token supplied.

- **Done when:** `cargo test -p fsm-cli --test mcp_progress` passes every case above, a call without a token emits nothing and keeps its golden, timing comes from the injected clock, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
