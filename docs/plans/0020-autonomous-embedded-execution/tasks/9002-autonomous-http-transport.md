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
  - crates/fsm-cli/tests/workflow_race/mod.rs
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
  - crates/fsm-cli/src/http/endpoint/buffer.rs
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
status: done
merged_as: "32dc871170b3402c091a62274cfa88efe1cbc05e"
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
  Frozen native candidate 3c1e490b failed: the fixture read history after DELETE
  without reinitializing its session. Verified failed-report digest in the task
  cache is bfd123562d069169d6110433f619e01d8aa1ccf44941c5b664a2cc3c4bc53f7f.
  The corrected adapter reinitializes only after zero-session observation;
  its socket regression reproduces HTTP 400 when that guard is removed and
  passes after restoration, with focused clippy and format/size/diff checks.
  The provisioned group must still pass before native acceptance can count;
  no installed handler has been executed locally.
- Frozen native candidate 340e76ba reached all seven successful handler
  settlements but failed on an empty HTTP diagnostic response: the adapter used
  ordinary output where journal_verify requires hosted output. HTTP now exposes
  the existing bounded transport queue as hosted output, and buffered JSON waits
  for observed drainage before reading its sink; admission closes on unwinding.
  A real-binary journal_verify regression returns health Ok, retains the physical
  writer and continues discovery. Removing only hosted HTTP output mode
  reproduces the empty response; restoration passes. Twenty-one focused HTTP
  cases, CLI all-target clippy and format/size/diff checks pass.
  Frozen native group at e6cf6e5c passes all three success/retry/compensation
  scenarios, including zero-session observation, actual original-owner exit,
  journal reopening and authority/staging retirement. Source cleanliness, exact
  case/count, exit status and transcript/log digests are verified; task-cache
  report digest is 31184c2fc31faecfea492ab3be9c9072b2f486c23d3eb48c8d5bc62cd19fce67.
- Buffered HTTP responses now independently bound aggregate Vec capacity to
  8 MiB across completed frames, discard overflow and refuse later publication,
  then transfer and seal fully observed bytes without cloning the whole buffer.
  Three focused cases cover exact capacity, limit-plus-one, suffix refusal and
  the real protocol notifier writer; relaxing only the buffer limit fails the
  independent producer case and restoration passes. Twenty-one HTTP cases,
  CLI all-target clippy and format/size/diff checks pass. Hosted dispatch/output
  errors retire the original HTTP session without stopping the shared owner.
  Full concurrent admission and active-handler expiry/DELETE acceptance still
  require closure; plan-end gates remain due and this task stays in progress.
- Real-binary writer-mode HTTP acceptance now fills all 32 transport sessions,
  observes HTTP 503 without a session ID on the thirty-third initialization,
  verifies an existing session remains usable, and reclaims capacity after
  DELETE while the retired ID returns HTTP 404. The original writer stays held,
  the journal does not advance and verification remains healthy. The focused
  case, CLI all-target clippy and format/size/diff checks pass. This establishes
  transport session-count admission, not simultaneous host command-count
  saturation; the remaining acceptance and plan-end gates still apply.
- A focused provisioned HTTP case now holds the first original handler and its
  descendant behind an explicit release barrier before deleting the only session.
  It requires unchanged process identities and journal records, retained writer
  ownership, then releases that same tree and uses the existing seven-call,
  settlement, resource, zero-session completion and original-owner exit checks.
  Fixture compilation, CLI all-target clippy, ten producer checks and
  format/size/diff checks pass; native execution must pass before this case can
  establish active-handler DELETE acceptance.
  Frozen candidate e590422f failed before handler entry because its provisioned
  catalogue added a retry policy absent from the CLI fixture; native binding
  correctly refused the changed fingerprint and retained the uncertain claim.
  Verified failed-report digest in the task cache is
  918dda7ac7ab595fee6d281f86bf419d560f65934ab0d017809842b641355209.
  Both fixture tables now retain their original 30-second timeout and default
  single attempt, and entry failures include bounded executor diagnostics;
  focused compilation, CLI clippy and format/size/diff checks pass.
  Corrected frozen candidate 80c1c34a passes genuine native acceptance: DELETE
  preserves both original live process identities and the unresolved claim,
  then zero-session observation reaches completion with exactly seven original
  handler invocations and acknowledgements. Reinitialization observes verified
  history; original-owner drain exits successfully and the physical store
  reopens with no unresolved claims or handoffs. Source cleanliness, exact case,
  successful exit, log/transcript digests and authority/staging retirement are
  verified; report digest is
  e3aa5143844db1753ddeb9b4aa1cf9ebd3a11e7750c123c94213f55e1b44602c.
  The integrated focused HTTP milestone passes 90 integration and twelve unit
  cases, CLI all-target clippy, format/size and frozen-range diff checks; the
  session-ID subprocess entry remains intentionally ignored in its parent run.
  This closes active-handler DELETE acceptance, not concurrent host command
  saturation or the plan-end full/platform gates; this task stays in progress.
- Ordinary hosted tool calls now use a request-local protocol view with the
  original cancellation registry and no reverse-mailbox reader, reaching host
  admission while an elicitation in the same session remains unanswered.
  The real question case now reads that same session before answering; disabling
  only the independent-tool path fails its bounded socket read and restoration
  passes. Ten real HTTP cases, CLI all-target clippy and format/size/diff checks
  pass. Fixture startup serializes its reserve-to-bind gap after a parallel
  connection-refusal failure; request execution remains concurrent. The exact
  concurrent command saturation case and plan-end gates remain outstanding.
- Real writer-mode HTTP now retains exactly 32 admitted unanswered requests
  across original sessions and refuses a thirty-third mutation with HTTP 503.
  Every original request cancels with typed req/cancelled, journal sequence
  remains unchanged through refusal and drain, and retrying the refused request
  ID commits as nonduplicate with a verified journal and retained writer.
  Raising only the original host count to 33 fails this named case by admitting
  the mutation; restoration passes. This closes the concurrent global command
  count acceptance alongside the existing real HTTP byte-pressure refusal.
  The integrated focused checkpoint passes 103 HTTP cases, CLI all-target
  clippy and format/size/diff checks; native acceptance must be revalidated
  against the final same-session dispatch repair before focused task closure.
- Frozen `6cf1df2b..32dc8711` closes the original focused HTTP inventory:
  all four provisioned success/retry/compensation/active-handler DELETE scenarios
  pass on the final dispatch source, with verified clean-source reports,
  log/transcript digests, actual original-owner exit, writer reopening and
  authority/staging retirement. The 103 focused HTTP cases cover deadline
  progress, expiry, modes, security, replay, live questions, cancellation and
  exact global count refusal with unchanged journal/request-ID state.
  Self-review found no further task-scope defect; task-cache verdict digest:
  `7af50075bf68db37fc2c6c48f90649876e2022533702a708b625c9dc73652ee8`.
  Native scope is Linux/systemd stable; no independent reviewer, executable-byte
  comparison or fresh full/platform gate is claimed. Full integration gates
  remain due at plan completion; task 9003 owns final public-contract alignment.
