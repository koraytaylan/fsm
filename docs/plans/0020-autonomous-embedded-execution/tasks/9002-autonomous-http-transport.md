---
id: autonomous-http-transport
title: "Autonomous HTTP Transport"
workstream: "0090"
kind: task
depends_on:
  - autonomous-stdio-transport
gated: false
touches:
  - crates/fsm-cli/tests/mcp_execute_workflow.rs
  - crates/fsm-cli/tests/workflow_http/mod.rs
  - crates/fsm-cli/tests/workflow_stdio/mod.rs
  - crates/fsm-execute/src/containment/workflow_native_tests.rs
  - crates/fsm-execute/tests/lifecycle_platform/workflow_probe.py
  - crates/fsm-cli/src/http/mod.rs
  - crates/fsm-cli/src/http/startup.rs
  - crates/fsm-cli/src/mcp/serve.rs
  - crates/fsm-cli/src/mcp/serve/native_stdio.rs
  - crates/fsm-cli/src/mcp/mod.rs
  - crates/fsm-cli/src/mcp/http_host.rs
  - crates/fsm-cli/src/mcp/http_host/native.rs
  - crates/fsm-cli/src/mcp/executor.rs
  - crates/fsm-cli/src/mcp/prompts.rs
  - crates/fsm-cli/src/mcp/descriptions.rs
  - crates/fsm-cli/tests/fixtures/transcripts/full_2024-11-05.out.jsonl
  - crates/fsm-cli/tests/fixtures/transcripts/full_2025-03-26.out.jsonl
  - crates/fsm-cli/tests/fixtures/transcripts/full_2025-06-18.out.jsonl
  - crates/fsm-cli/tests/fixtures/transcripts/skeleton.out.jsonl
  - crates/fsm-cli/tests/fixtures/transcripts/skeleton_echo.out.jsonl
  - crates/fsm-cli/tests/fixtures/mcp_live/session.expected
  - crates/fsm-cli/tests/fixtures/mcp_live/quiet.expected
  - crates/fsm-cli/tests/fixtures/mcp_affordance/session.expected
  - crates/fsm-cli/tests/fixtures/audit/session.expected
  - crates/fsm-cli/src/http/endpoint.rs
  - crates/fsm-cli/src/http/endpoint/streaming.rs
  - crates/fsm-cli/src/http/endpoint/retirement_tests.rs
  - crates/fsm-cli/src/http/writer.rs
  - crates/fsm-cli/src/http/session.rs
  - crates/fsm-cli/src/mcp/serve/session_store.rs
  - crates/fsm-cli/src/mcp/notify.rs
  - crates/fsm-cli/src/http/sse.rs
  - crates/fsm-cli/src/http/server.rs
  - crates/fsm-cli/src/mcp/host/
  - crates/fsm-cli/tests/autonomous_http.rs
  - crates/fsm-cli/tests/http_multi_client.rs
  - crates/fsm-cli/tests/http_resume.rs
  - crates/fsm-cli/tests/http_limits.rs
  - crates/fsm-cli/tests/http_session.rs
  - crates/fsm-cli/tests/http_server.rs
  - docs/SPEC.md
  - docs/EMBEDDING.md
  - docs/API-POLICY.md
  - docs/RELEASE.md
status: in_progress
merged_as: ""
---
# Autonomous HTTP Transport

HTTP must retain its embedded executor and share exactly one execution host
across every session.

**Steps:**

1. Construct the shared host from the complete `ServeMode` in `run_http`,
   retaining its executor and handler table; replace endpoint-owned writer
   access with bounded host requests and immutable responses.
2. Apply the same startup mode selection as stdio, including read-only
   contention fallback and degraded diagnostics; no HTTP session creates
   its own executor or mutates a read-only handle.
3. Preserve authentication, origin checks, protocol negotiation, existing
   connection/session caps, and SSE replay bounds before host admission;
   return 503 for requests refused before admission.
4. Keep execution alive through DELETE, session expiry, SSE disconnect, and
   zero attached clients; only host stop invokes supervised executor shutdown.
5. Deliver ordered asynchronous messages through each session's existing
   bounded SSE mechanism; isolate slow or saturated sessions and preserve
   reconnect/replay behavior without keeping a response's temporary sink alive.
6. Update the HTTP lifetime, embedded mode, and transport parity contracts
   in SPEC, EMBEDDING, and API-POLICY.

**Tests:**

- `cargo test -p fsm-cli --test autonomous_http`: the real `serve --http
  --execute` process completes the same success/retry/deadline/compensation
  fixtures as stdio after the submitting client stops making requests.
- DELETE or expire the only session while a handler is active; a fresh
  session later observes the completed workflow with one handler invocation
  and one host, including an interval with zero connected clients.
- A blocked SSE consumer or unanswered elicitation leaves a second
  client's reads, cancellation, and workflow progress responsive.
- Saturate count/byte admission through real HTTP requests, assert 503 and
  no journal mutation for refused work, and verify the journal after drain.
- Contention and corruption follow stdio's mode contract and start no
  handlers; native HTTP conformance, security, resume, and limit tests pass.

- **Done when:** real HTTP tests prove autonomous execution and session-independent host lifetime with one writer and bounded isolated sessions, while the stable host and existing HTTP gates pass.

Focused startup acceptance:

- Real `serve --http --execute` configured-sentinel cases prove contended
  read-only refresh, refusal of mutation and automatic upgrade, and degraded
  diagnosis from the original directory without handler starts or altered
  corrupt bytes. Direct sentinel execution verifies its marker behavior.
  Neutralizing only read-only refresh fails the named real-client case on
  stale state; restoration passes. Twenty-six focused fallback/conformance/
  multi-client/resume/limit cases pass; source and test-target clippy pass.
- Startup acceptance alone establishes no autonomous capability or task
  landing; full gates remain due at plan completion.
- Writer-only production HTTP now selects the existing bounded command owner
  and immutable replies; per-session protocol locks replace the global Live
  lock, cancellation/DELETE bypass those waits, and pre-admission busy replies
  map to HTTP 503. A real-binary two-session fixture verifies the shared writer,
  duplicate-result contract, unchanged replay sequence and a verified journal.
  Twenty-seven focused HTTP cases, source/test-target clippy and format/size/
  diff checks pass. Review repaired empty hosted replies to preserve cancelled
  HTTP 202 and retired-admission HTTP 503 without fabricating a JSON-RPC result.
  Genuine native workflows, exact HTTP saturation, client-wait responsiveness
  and asynchronous SSE framing still require their original acceptance proof.
- Server stop now wakes admitted socket I/O before joining connection workers;
  finished workers also end sockets retained for shutdown. Twenty focused
  production/fallback, server and limit cases pass, with focused clippy and
  format/size/diff checks. Disabling only the stop wakeup fails the named silent/
  partial-request case; restoration passes. This removes a transport retirement
  delay without claiming application interruptibility or native host closure.
- Supported Linux embedded HTTP now consumes the complete executor into one
  native owner and publishes its original physical-store control endpoint;
  binding precedes execution. The real-binary manual-effect/deadline fixture
  deletes the only session, observes committed completion without HTTP calls,
  reconnects to the same writer and exits successfully through original drain
  control with a verified journal. Delaying only HTTP scheduling fails the
  named zero-session deadline case; restoration passes. A retained actual
  owner test proves first-deadline preservation, writer retention and unknown
  worker/output facts on timeout. Thirty-eight focused HTTP cases, thirty-three
  guidance goldens, that uncertainty case, CLI all-target clippy and format/
  size/diff checks pass. Guidance and fixtures now state the transport lifetime.
  No installed handler was run locally; genuine success/retry/compensation,
  expiry retirement, exact HTTP saturation and bounded asynchronous SSE/
  unanswered-elicitation isolation still require acceptance before task closure.
- Lazy expiry and DELETE now retire the original host session, mailbox, replay
  allocation and feed stop signal outside the session registry and without
  waiting for its protocol lock; lookup cannot recreate retired IDs. Four
  focused retirement cases cover held protocol state, repeated full capacity
  reclamation and real TCP native deadline progress after forced idle expiry,
  followed by original-owner shutdown and journal verification. Disabling only
  expiry callbacks fails the named retirement case; restoration passes.
  Thirty-four existing HTTP cases and CLI all-target clippy pass; one fixture
  subprocess entry remains intentionally ignored. Genuine installed-handler,
  saturation and asynchronous SSE/elicitation acceptance remain outstanding;
  task 9002 stays in progress and full gates remain due at plan completion.
- SSE replay now enforces the byte ceiling even for one oversized event,
  evicts before cloning new payload, and reports empty-buffer gaps. Fragmented
  line writers accept exactly 1 MiB and refuse the first excess byte, releasing
  partial storage and refusing suffix publication. Live GET closes on gaps and
  captures its delivery cursor before headers instead of skipping newly arriving
  events. Eighteen focused replay/SSE cases and the production-facing overflow
  case pass; neutralizing each oversized-record, partial-frame and live-gap
  guard fails its named case, and restoring it passes. CLI all-target clippy
  and format/size/diff checks pass; asynchronous POST and the remaining native/
  admission acceptance are still required before this task can land.
- Production streamed POST now gives the existing bounded complete-frame
  queue an owned socket, delivering elicitation and progress during dispatch;
  borrowed endpoint calls keep their original synchronous clocks and output.
  A real writer-mode binary emits the question ID to the actual client, accepts
  its later answer on another connection, and lets a second session read and
  advance the workflow while the question remains unanswered. Events retain
  ordered IDs and EOF delimits the final answer; the journal verifies. Disabling
  only early output makes this case fail on its bounded read; restoration passes.
  Forty-six focused HTTP and seven output-queue cases, CLI all-target clippy and
  format/size/diff checks pass. Queue allocation includes in-flight frames;
  failure/unwind closes admission and wakes the socket without joining I/O.
  Original-session failure retirement does not stop the shared executor.
  Genuine native success/retry/compensation, cancellation during unanswered
  elicitation, and real HTTP count/byte saturation still require acceptance;
  this task stays in progress and plan-end gates remain due.
- Real-binary writer-mode HTTP now has a named unanswered-elicitation
  cancellation case: another session's identical request ID cannot cancel it;
  the original session receives typed req/cancelled within the bounded wait,
  with no elicited event or journal advance. The same physical writer remains
  held and a second session subsequently commits an event with a verified
  journal. Disabling only HTTP cancellation routing fails the case on its
  bounded read; restoration passes. Focused CLI clippy and format/size/diff
  checks pass. This closes the cancellation-specific transport acceptance,
  while genuine native handler workflows and real HTTP count/byte saturation
  still prevent task completion; plan-end gates remain due.
- A real writer-mode HTTP byte-pressure case now verifies that a synthetic
  argument allocation above the original session budget receives HTTP 503
  before tool validation, journal advance or request-ID claim. Its smaller
  shape reaches semantic validation through HTTP 200; retrying the refused ID
  with valid arguments commits as nonduplicate and the journal verifies.
  Disabling only the original session-byte guard fails the named HTTP case;
  restoration passes. This is a retained-allocation refusal case, not an exact
  byte-boundary or
  concurrent count-saturation verdict. Focused CLI clippy passes; genuine
  native workflows and the remaining exact/concurrent admission acceptance
  still prevent task completion.
- The provisioned native workflow runner now registers one HTTP group containing
  original success, quiet retry and compensating-failure scenarios. It reuses
  actual discovered contracts, handler fixtures and full journal/call-order/
  resource assertions, deletes the only HTTP session after submission, observes
  through read-only storage and retires through original local drain control.
  Fixture and root-runner compilation, CLI all-target clippy and ten mocked
  evidence-producer checks pass; these checks do not establish native acceptance.
  The focused provisioned group must pass before its workflows can count toward
  task completion; no installed handler has been executed locally.
