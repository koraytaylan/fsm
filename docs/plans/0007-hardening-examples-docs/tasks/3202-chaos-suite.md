---
id: chaos-suite
title: "Chaos Suite"
workstream: "0032"
kind: task
depends_on: []
gated: false
touches:
  - crates/fsm-cli/tests/chaos.rs
status: done
merged_as: ""
---
# Chaos Suite

Curated fixtures cannot find the interactions a random storm can: seeded sequences of valid and invalid operations — defines, sends, acks, cancels, mid-sequence reopen — must leave the journal verifiable and the process alive after every run, on stable Rust with zero dependencies.

**Steps:**

1. Implement `crates/fsm-cli/tests/chaos.rs` with a self-contained xorshift64* generator (the ~30-line duplication with the workstream-0033 generator is deliberate — test crates cannot share `tests/` helpers across crates — and both files say so).
2. Implement the storm driver and the per-iteration invariants exactly as inventoried under **Tests**.

**Tests:**

- The storm, in `chaos.rs`: 200 seeded iterations (fixed base seed, per-iteration seed derived and printed on any failure; a `CHAOS_SEED` env var replays exactly one seed), each on a fresh temp data dir with 30–80 operations drawn from the mix — valid and deliberately malformed defines, instance creates, sends (valid payloads, wrong-typed payloads, unknown events, duplicate `request_id`s, stale `expect_seq`), `effect_ack`s (pending and unknown ids), cancels, and a mid-sequence store close-and-reopen.
- Mix accounting: an operation-kind counter asserts every kind occurred across the run and that at least half the iterations exercised the close-and-reopen — the storm cannot silently degenerate into a happy-path loop.
- Post-sequence invariants, every iteration: no panic (reaching the assertions is the proof); full journal verification green; a fresh refold contains every operation that received a success response (tracked by the driver); the live store's per-instance state hashes equal the refold's.
- Error-path sanity, asserted opportunistically across the storm: every rejected operation returned a structured error with a non-empty `hint` — the storm doubles as a fuzz of the error paths.
- Storm determinism: re-running one fixed seed reproduces the identical operation log byte-for-byte (guards the generator against wall-clock or ordering leaks).

- **Done when:** `cargo test -p fsm-cli --test chaos` completes 200 seeded sequences with every per-iteration invariant green and seed-replay wired, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
