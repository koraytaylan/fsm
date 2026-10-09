---
id: autonomous-http-transport
title: "Autonomous HTTP Transport"
workstream: "0090"
kind: task
depends_on:
  - autonomous-stdio-transport
gated: false
touches:
  - crates/fsm-cli/src/http/mod.rs
  - crates/fsm-cli/src/http/startup.rs
  - crates/fsm-cli/src/mcp/serve.rs
  - crates/fsm-cli/src/mcp/mod.rs
  - crates/fsm-cli/src/mcp/http_host.rs
  - crates/fsm-cli/src/http/endpoint.rs
  - crates/fsm-cli/src/http/writer.rs
  - crates/fsm-cli/src/http/session.rs
  - crates/fsm-cli/src/http/sse.rs
  - crates/fsm-cli/src/http/server.rs
  - crates/fsm-cli/src/mcp/host/
  - crates/fsm-cli/tests/autonomous_http.rs
  - crates/fsm-cli/tests/http_multi_client.rs
  - crates/fsm-cli/tests/http_resume.rs
  - crates/fsm-cli/tests/http_limits.rs
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
- Shared host construction, independent native execution, per-session bounded
  admission, asynchronous SSE delivery and zero-client lifetime acceptance
  remain incomplete; this startup repair establishes no autonomous capability
  or task landing, and full gates remain due at plan completion.
- Writer-only production HTTP now selects the existing bounded command owner
  and immutable replies; per-session protocol locks replace the global Live
  lock, cancellation/DELETE bypass those waits, and pre-admission busy replies
  map to HTTP 503. A real-binary two-session fixture verifies the shared writer,
  duplicate-result contract, unchanged replay sequence and a verified journal.
  Twenty-seven focused HTTP cases, source/test-target clippy and format/size/
  diff checks pass. Review repaired empty hosted replies to preserve cancelled
  HTTP 202 and retired-admission HTTP 503 without fabricating a JSON-RPC result.
  Native owner scheduling, exact HTTP saturation, client-wait responsiveness
  and asynchronous SSE framing still require their original acceptance proof.
