---
id: bounded-session-channels
title: "Bounded Session Channels"
workstream: "0089"
kind: task
depends_on:
  - autonomous-host-scheduling
gated: false
touches:
  - crates/fsm-cli/src/mcp/host/
  - crates/fsm-cli/src/mcp/notify.rs
  - crates/fsm-cli/src/mcp/methods.rs
  - crates/fsm-cli/src/mcp/elicit.rs
  - crates/fsm-cli/src/mcp/cancel.rs
  - crates/fsm-cli/src/mcp/progress.rs
  - crates/fsm-cli/src/mcp/watch.rs
  - crates/fsm-cli/src/mcp/tools/handlers/
  - crates/fsm-cli/tests/elicit_tool.rs
  - crates/fsm-cli/tests/mcp_cancel.rs
  - docs/SPEC.md
  - docs/API-POLICY.md
  - docs/EMBEDDING.md
status: in_progress
merged_as: ""
---
# Bounded Session Channels

Waiting for a client reply or writable output must not park the execution
host or allocate an unbounded backlog.

**Steps:**

1. Route immutable host messages into count- and byte-bounded per-session
   output queues; implement exact accounting and encoded-message limits
   from ARCHITECTURE without constructing unbounded intermediate copies.
2. Publish durable changes after commit, preserve commit order, and enqueue
   the initiating mutation's response before its own change notifications;
   keep existing complete-frame encoding and permissible URI coalescing.
3. Move client elicitation and long read-only diagnostics out of the store
   owner into session continuations; resume committing operations with
   sequence revalidation when autonomous work has changed the instance.
4. Route cancellation and client responses without waiting for an ordinary
   application slot, preserving request/session identity and existing
   cancellation semantics for an already committed workflow.
5. Detach a saturated output session instead of blocking the owner or
   silently dropping its responses; preserve committed idempotent results
   for a reconnecting client and invoke the transport-specific close hook.
6. Define the over-capacity, stale continuation, notification, and disconnect
   contracts in SPEC, API-POLICY, and EMBEDDING with limits in their actual
   accounting units.

**Tests:**

- `cargo test -p fsm-cli --lib session_channels`: private tests under
  `mcp/host/tests/` use the existing `cfg(test)` harness to prove that a stalled
  output sink and an unanswered elicitation do not prevent another session's
  requests or autonomous retries/deadlines from advancing, without a public
  test-access API.
- Each exact egress count/byte/frame limit accepts; each limit-plus-one
  closes only the offending session and never silently discards a response.
- Pause at the commit boundary and prove no invalidation is visible before
  durability; release it and prove initiating-response and later commit
  ordering, including simultaneous background completion.
- Replying to an elicitation after executor progress revalidates sequence
  and never overwrites intervening state; cancellation reaches a waiting
  continuation even at full application capacity.
- Reconnect after response delivery fails and replay the same request ID:
  the committed mutation exists once and the verified journal is intact.

- **Done when:** the `session_channels` inventory and existing elicitation/cancellation tests pass with bounded egress and no client wait on the writer owner under the stable host gate.


Focused channel review:

- Twenty-nine focused `session_channels` cases pass, including original native
  deadline progress and a second session response while actual output remains
  blocked, and replay through a replacement session after actual delivery
  failure without another journal append. Affected store/CLI test-target clippy and
  diff checks pass; expensive integration gates remain due at plan completion.
- Long progress tools now select session-owned read-only workers rather than
  writer dispatch, retaining original admission and cancellation controls.
  A held diagnostic clock permits another session's owner request to complete;
  disabling only diagnostic routing fails the named regression at exit 101.
  The adapter supplies injected clock samples and continues the existing
  owned-input control wait. Adapter clock unwinding cancels original controls,
  preserves another session's same-ID control, and releases the worker's
  reservation while retaining an independently held original reservation.
  Neutralizing only the cancellation guard fails this regression at exit 101;
  restoring it passes. A held production diagnostic retains the thirty-second
  host slot while all other application slots remain occupied; original-session
  cancellation reaches it without admitting another application command,
  returns `req/cancelled`, and releases exactly one slot after actual return.
  Another session cannot cancel that request. Actual owned-input ping and
  cancellation reach a paused original diagnostic at full application capacity;
  EOF retires its adapter and cancels the original work before the worker
  returns. The barrier is session-scoped and test-only, before read-only store
  opening, and does not replace the worker outcome. Disabling only diagnostic
  owned-input routing fails both named lifetime cases at exit 101; restoring
  it passes all channel cases.
- The six egress boundary and failure-callback cases and
  all eighteen existing elicitation/cancellation integration cases pass.
- Publication review found that a guard on one output stream could not
  establish another session's durable prefix, and an adapter without borrowed
  input lacked the originating response guard. Hosted feeds now share the
  original writer's read-only committed-prefix observer and retain response
  ordering across durable and later commits. A real append is paused between
  write and fsync to prove the observer stays unchanged; read-only journals
  cannot create that observer, failed appends cannot advance it, and it survives
  writer retirement. The hosted feed rejects valid visible but uncommitted
  fixture bytes, while another session may publish an already durable commit
  during the originating response wait. Premature observer advancement,
  disabling the feed ceiling, and disabling either response guard for shared
  or separate streams each fail their named regression at exit 101;
  restoration passes. Both focused committed-prefix cases and all seven
  existing subscription integration cases pass.
- Hosted egress now invokes one nonblocking retirement action outside its
  queue mutex, including actual write failure and registration after failure.
  The production adapter closes only the original saturated session immediately,
  while another session completes with the failed writer still blocked.
  Neutralizing overflow retirement or actual write-failure retirement fails
  its named regression at exit 101; restoration passes. Stdio registers its
  original session and host shutdown action before request handling; all five
  owned-stdio cases pass, including actual broken output with quiet input and
  verified writer release. Physical HTTP connection retirement remains part
  of task 9002's transport composition, not a claim of HTTP completion here.
- Inventory review adds production session-isolation proof for retained-byte
  and encoded-frame overflow, supplementing their exact boundary cases;
  disabling only overflow retirement fails both regressions at exit 101.
  Review also found that elicitation settlement guarded its response stream
  but omitted a separate feed stream. The hosted adapter now binds that feed
  to the original request's I/O scope, acquiring both guards only on settlement
  and retaining them through response enqueue. The separate-feed case permits
  already committed notifications while unanswered, defers settlement updates,
  and verifies the committed event once. Removing only that feed binding fails
  the named regression at exit 101; restoring it passes all twenty-nine channel
  cases and the eighteen elicitation/cancellation and seven subscription cases.
- Completion still requires consolidated frozen review of the original
  inventory; no landing OID is assigned and full gates remain due at plan end.
