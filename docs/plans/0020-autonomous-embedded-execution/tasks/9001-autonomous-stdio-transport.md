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
  - crates/fsm-cli/tests/workflow_stdio/
  - crates/fsm-execute/src/containment/workflow_native_tests.rs
  - crates/fsm-execute/tests/lifecycle_platform/workflow_probe.py
  - crates/fsm-execute/tests/lifecycle_platform/test_workflow_producer.py
  - .github/workflows/focused-workflow.yml
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
status: in_progress
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

Focused acceptance review:

- Eight real-binary `autonomous_stdio` cases and its portable sentinel fixture
  pass: quiet deadlines with and without subscription, broken stdout with open
  stdin, poll-interval refusals and exact accepted boundaries, installed
  protocol panic-hook cleanup, competing-writer refresh and degraded diagnosis.
- The production contention and damaged-journal cases configure an executable
  sentinel handler with pending work and observe no marker or execution claim;
  direct fixture execution verifies its marker behavior. Contention refreshes
  an external committed event and cannot upgrade the original session when
  its writer releases. Degraded mode reports an unreadable unhealthy journal,
  serves diagnosis, refuses instance access and preserves its original bytes.
  Focused clippy, formatting and diff checks pass; no containment helper was
  invoked for this local acceptance slice.
- The provisioned original workflow inventory now includes a quiet transient
  failure retry: the first prerequisite exits 7, the next succeeds, and the
  production server completes without further client requests. Original native
  claims and closure proofs are checked for eight runs, one attempted failure,
  seven acknowledgements and matching second-attempt identity. The portable
  subprocess fixture passes; CLI/provisioner test-target clippy and eight
  mocked producer retirement tests pass. Genuine native execution of this
  new case remains unexecuted, not a passed skip.
- Manual focused CI accepts one exact native workflow case, forwarding the
  original runner filter and retaining its source-bound report and log;
  ten mocked producer checks pass, including bounded single-case execution
  and refusal of a missing original case marker. The full gate is unchanged.
- Success, transient retry, compensation, interrupted advance and a held real
  process/MCP handler with responsive reads remain to be matched to this
  original inventory; no landing OID or full-gate claim is assigned, and full
  gates remain due at plan completion.
