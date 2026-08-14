---
id: replay-determinism
title: "Replay Determinism"
workstream: "0021"
kind: task
depends_on:
  - instance-store
gated: false
touches:
  - crates/fsm-cli/tests/replay_determinism.rs
status: planned
merged_as: ""
---
# Replay Determinism

The determinism claim is a test: journals produced through the real append path must refold to bit-identical state hashes with snapshots, without snapshots, and from a byte-copied directory — and verification must return the exact typed health for every committed corruption fixture.

**Steps:**

1. Implement `crates/fsm-cli/tests/replay_determinism.rs`: drive a real `Store` through the scripted mixed session in a temp data dir under `FSM_CLOCK_MS`, then assert exactly the inventory under **Tests**.

**Tests:**

- The scripted session covers **all ten record kinds** — genesis, two defines, a create, applied events with effects, a rejected event, an ignored event (a machine with `on_unhandled: "ignore"`), an effect ack, a request rejection (ack of an unknown effect id), a cancel, and an annotation — asserted by collecting the journal's kind set and comparing it to the full `RecordKind` list, so the refold exercises every re-application arm.
- Four-way hash identity, all bit-identical: the live in-memory state hashes at session end; a full refold of the journal ignoring snapshots; a reopen through a forced snapshot; and an open of a byte-copied journal directory at a different path.
- Snapshot equivalence detail: the forced snapshot's reload-and-verify passes, and the snapshot-fast-path open and the full-refold open produce the same `StoreState` (machines, instances, dedup — compared structurally, not just by hash).
- Clock determinism end-to-end: running the identical session a second time in a fresh temp dir under the same `FSM_CLOCK_MS` produces a byte-identical journal (segment bytes compared), proving no wall-clock or ordering leak anywhere in the append path.
- Fixture classification sweep: `journal_io::verify` returns each committed `fixtures/journals/*` directory's exact `JournalHealth` variant — the typed six-way classification the plan-0005 CLI maps to exit codes 0/2/3/4/5/6.

- **Done when:** `cargo test -p fsm-cli --test replay_determinism` proves the ten-kind coverage, all four hash-identity paths, byte-identical session reruns, and the fixture-exact health classifications, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
