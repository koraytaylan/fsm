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
  - crates/fsm-cli/src/mcp/watch.rs
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

Acceptance is frozen at the steps, tests and Done when criterion above; sibling
scheduling, channel and transport tasks retain their own acceptance inventories.
Historical integration checkpoints are archived outside the repository by digest:
`0b8e7256a62ea4a6408844397ba59539c97b28ceb6a7f2985a088241c391788e`.
