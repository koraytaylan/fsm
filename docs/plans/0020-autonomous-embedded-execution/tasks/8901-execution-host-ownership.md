---
id: execution-host-ownership
title: "Execution Host Ownership"
workstream: "0089"
kind: task
depends_on: []
gated: false
touches:
  - crates/fsm-cli/src/args.rs
  - crates/fsm-cli/src/cli/execute.rs
  - crates/fsm-cli/src/mcp/mod.rs
  - crates/fsm-cli/src/mcp/host/
  - crates/fsm-cli/src/mcp/tools/dispatch.rs
  - crates/fsm-cli/src/mcp/tools/mod.rs
  - crates/fsm-cli/src/mcp/tools/elicitation.rs
  - crates/fsm-cli/src/mcp/elicit.rs
  - crates/fsm-cli/src/mcp/notify.rs
  - crates/fsm-cli/src/mcp/notify/output.rs
  - crates/fsm-cli/src/mcp/notify/output/
  - crates/fsm-cli/src/mcp/notify/encoded.rs
  - crates/fsm-cli/src/mcp/notify/pending_input.rs
  - crates/fsm-cli/src/mcp/resources.rs
  - crates/fsm-cli/src/mcp/executor.rs
  - crates/fsm-cli/src/mcp/prompts.rs
  - crates/fsm-cli/src/mcp/descriptions.rs
  - crates/fsm-cli/tests/autonomous_stdio.rs
  - crates/fsm-cli/tests/fixtures/
  - README.md
  - crates/fsm-cli/src/mcp/methods.rs
  - crates/fsm-cli/src/mcp/methods/
  - crates/fsm-cli/src/mcp/serve.rs
  - crates/fsm-cli/src/mcp/serve/
  - crates/fsm-cli/src/mcp/notify/diagnostic_output.rs
  - docs/SPEC.md
  - docs/API-POLICY.md
  - docs/EMBEDDING.md
  - docs/RELEASE.md
status: in_progress
merged_as: ""
---
# Execution Host Ownership

Establish a runnable host whose writer never leaves its owner and whose
request queue has explicit count and byte bounds.

**Steps:**

1. Add the private host module described in ARCHITECTURE, accepting an owned
   store, injected clock, and owned typed commands; add a private `cfg(test)`
   `execution_host` integration harness under `mcp/host/tests/` that submits
   the same command envelopes production adapters will use, without adding
   a public CLI-library API merely to expose private internals to tests.
2. Extract the store-dependent dispatch boundary so the owner applies one
   complete operation at a time and returns immutable response data; keep
   session I/O and user interaction outside that boundary.
3. Implement host and per-session count/byte admission, session generation,
   per-session FIFO, and separately bounded stop/cancel/close controls;
   account for every retained payload allocation rather than raw frames only.
4. Reject over-capacity work before store dispatch with the specified busy
   result; reject admission after stop without creating a journal record.
5. Return the committed sequence with each mutation's publication marker;
   keep failed operations atomic and preserve original request IDs and
   fingerprints through the envelope.
6. Document the serialization, admission, and unchanged persistence
   contracts in SPEC and classify their API effect in API-POLICY.

**Tests:**

- `cargo test -p fsm-cli --lib execution_host`: mixed reads and writes from
  concurrent session handles observe complete states in admission order;
  no handle opens another writer while the owner is alive.
- The exact count and byte limits accept work; each limit-plus-one is busy,
  calls no store operation, and leaves the verified journal unchanged.
- A full application queue still accepts the reserved stop control and
  drains or rejects admitted commands according to the specified stop state.
- Replaying a committed request after losing its response returns the
  original result once; reusing it with different content still conflicts.
- An expired session generation cannot receive a newer session's response.

- **Done when:** the production command boundary passes every `execution_host` case above with exactly one store owner, bounded admission, and unchanged journal/idempotency behavior under the stable host gate.

The coordinator adopts these shared elicitation and framing boundaries serially
under task 8901 to complete the private stdio vertical path; task 8904 remains
planned and its scheduling dependency is not released by this integration.

The staged owned-input response wait now services cancellation, ping and EOF
while the writer is occupied, retaining at most eight raw deferred frames and
16 MiB of their allocation capacity; other requests remain in wire order for
the ordinary dispatcher, and cancellation suppresses only known pending IDs.
New fixtures cover EOF and cancellation before an owner turn, deferred
cancellation, the eighth/ninth frame boundary, and capacity release.
Verification is queued behind an independently running Cargo process; these
fixtures have not yet been accepted as passing evidence, production stdio
selection is unchanged, and this task remains in progress.

The hosted adapter unwind boundary now requests original-control shutdown
and retains the initiating failure instead of detaching its native owner; a
fixture injects an adapter panic after original owner startup and asserts
original control retirement and writer reopening, with verification pending
in the same queued milestone run and no additional task completion claim.

The coordinator adopts initial production stdio wiring and versioned discovery
serially within this working-path integration: Linux CLI stdio now selects the
owned host, and actual hosted executor discovery publishes v2/autonomous in
the same change. Initialization guidance, prompts, tool descriptions, README,
embedding guidance and affected instruction goldens follow the observed
progress field; HTTP and borrowed helpers retain their legacy contract.
A real-binary fixture leaves stdin open after creation and observes deadline
completion solely through read-only inspection, then checks EOF writer release.
The stable real-binary fixture passes after correcting its invalid empty handler
table to a declared manual-effect table; broader verification remains pending
and task 9001 stays planned
until its scheduling dependency and full scenario inventory are accepted.

The first working production path is extended with actual-binary quiet
subscription output and broken-stdout cleanup while stdin remains open; both
new scenarios are pending serial stable/MSRV verification, and the complete
9001/native scenario inventory and all task dependencies remain open.

The working-path integration now adopts embedded stdio poll-interval wiring
from 8903: the actual CLI validates the finite interval before loading a table
or opening a writer, passes it to the original native owner, and keeps native
stop observation independent; 8903 remains planned pending its complete
scheduling/fairness acceptance inventory, and verification is pending.
