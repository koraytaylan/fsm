---
id: chaos-suite
title: "Chaos Suite"
workstream: "0032"
kind: task
depends_on: []
gated: false
touches:
  - crates/fsm-cli/tests/chaos.rs
status: planned
merged_as: ""
---
# Chaos Suite

Curated fixtures cannot find the interactions a random storm can: seeded sequences of valid and invalid operations — defines, sends, acks, cancels, mid-sequence reopen — must leave the journal verifiable and the process alive after every run, on stable Rust with zero dependencies.

**Steps:**

1. Implement `crates/fsm-cli/tests/chaos.rs` with a self-contained xorshift64* generator (the ~30-line duplication with the workstream-0033 generator is deliberate — test crates cannot share `tests/` helpers across crates — and both files say so).
2. Drive 200 seeded iterations: fresh temp data dir, then 30–80 random operations mixing valid and invalid defines, creates, sends (wrong types, unknown events, duplicate `request_id`s, stale `expect_seq`), `effect_ack`s (pending and unknown), cancels, and a mid-sequence store close-and-reopen.
3. After each sequence assert: no panic occurred, full journal verification passes, and every operation that received a success response is present in a fresh refold; print the failing seed and honor a `CHAOS_SEED` env var for replay.

- **Done when:** `cargo test -p fsm-cli --test chaos` completes 200 seeded sequences with post-sequence verification green and seed-replay wired, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
