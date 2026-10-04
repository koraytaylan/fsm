---
id: autonomous-stdio-transport
title: "Autonomous Stdio Transport"
workstream: "0090"
kind: task
depends_on:
  - bounded-session-channels
gated: false
touches:
  - crates/fsm-cli/src/mcp/serve.rs
  - crates/fsm-cli/src/mcp/executor.rs
  - crates/fsm-cli/src/mcp/prompts.rs
  - crates/fsm-cli/src/mcp/descriptions.rs
  - crates/fsm-cli/src/mcp/host/
  - crates/fsm-cli/src/main.rs
  - crates/fsm-cli/tests/autonomous_stdio.rs
  - crates/fsm-cli/tests/mcp_execute_workflow.rs
  - crates/fsm-cli/tests/embedded_read_only.rs
  - crates/fsm-cli/tests/serve_modes.rs
  - crates/fsm-cli/tests/mcp_executor.rs
  - crates/fsm-cli/tests/fixtures/mcp_live/
  - crates/fsm-cli/tests/fixtures/mcp_affordance/
  - crates/fsm-cli/tests/fixtures/transcripts/
  - crates/fsm-cli/tests/fixtures/audit/session.expected
  - docs/SPEC.md
  - docs/EMBEDDING.md
  - docs/API-POLICY.md
  - docs/RELEASE.md
  - README.md
status: planned
merged_as: ""
---
# Autonomous Stdio Transport

Connect the real stdio server to the execution host so an open but quiet
client no longer pauses its workflow.

**Steps:**

1. Start one host for the stdio process and use bounded input/output
   adapters; replace request-following `drive_executor` with host scheduling.
2. Preserve initialization negotiation, parser limits, busy responses,
   resource subscription framing, and deterministic transcript helpers;
   document any unsupported CLI helper API change rather than implying a
   supported library breaking change.
3. Treat open idle stdin as a live session; treat EOF, broken output, and
   egress saturation as stop-admission events followed by plan 0022's
   supervised shutdown, with unfinished durable work recoverable on reopen.
4. Keep read-only contention fallback and degraded diagnosis behavior,
   including refresh and the defensive no-execution guard in direct helpers.
5. Publish the architecture's `fsm.executor/2` autonomous metadata together
   with this behavior, updating the normative stdio lifetime contract,
   initialization/prompt/tool instructions, README's embedded guidance,
   affected goldens (including the audit session), API policy, and release
   notes in the same commit; final cross-transport documentation reconciliation
   belongs to `autonomous-execution-contract`.

**Tests:**

- `cargo test -p fsm-cli --test autonomous_stdio`: launch the real binary,
  initialize and submit a workflow, keep stdin open, and send no further
  requests while success, retry, deadline, compensation, and restored
  interrupted advance each reach the expected durable state.
- A subscribed session receives asynchronous updates without ping; a
  separate case with no subscription still completes, observed through a
  read-only journal inspection that cannot tick the host.
- A barrier-held process/MCP handler allows an independent ping or read to
  finish before the handler is released; stdout remains valid JSON-RPC.
- EOF during work and broken output invoke supervised stop, release the
  writer within the lifecycle bound, and permit verified restart recovery.
- A real competing writer and a damaged journal each prevent all fixture
  starts while preserving the expected read-only or diagnostic surface.

- **Done when:** every real-binary `autonomous_stdio` case passes without polling requests driving execution, existing mode/stdio tests pass, and the stable host gate records the lifecycle integration used.
