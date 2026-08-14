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

1. Implement `crates/fsm-cli/tests/replay_determinism.rs`: drive a real `Store` through the scripted mixed session from architecture (defines, creates, applied events with effects, a rejection, an ack, a cancel, an annotation) in a temp data dir under `FSM_CLOCK_MS`.
2. Assert bit-identical final state hashes across: live state, a full refold ignoring snapshots, a reopen through a forced snapshot, and an open of a byte-copied journal directory at a different path.
3. Assert `journal_io::verify` returns each committed `fixtures/journals/*` directory's exact `JournalHealth` variant, exercising the classification the plan-0005 CLI will map to exit codes 0/2/3/4/5/6.

- **Done when:** `cargo test -p fsm-cli --test replay_determinism` proves all four hash-identity paths and the fixture-exact health classifications, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
