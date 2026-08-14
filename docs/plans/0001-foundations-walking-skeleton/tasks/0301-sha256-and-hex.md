---
id: sha256-and-hex
title: "Sha256 And Hex"
workstream: "0003"
kind: task
depends_on:
  - workspace-scaffold
gated: false
touches:
  - crates/fsm-core/src/sha256.rs
  - crates/fsm-core/tests/sha256_golden.rs
  - "crates/fsm-core/tests/fixtures/sha256/**"
status: planned
merged_as: ""
---
# Sha256 And Hex

The tamper-evidence story (machine identity, journal chain, state hashes) rests on this hand-rolled SHA-256, so it lands against the official FIPS 180-4 vectors committed before the implementation.

**Steps:**

1. Author `crates/fsm-core/tests/fixtures/sha256/vectors.txt` first with the NIST byte-oriented vectors named in architecture (empty message, `abc`, the two-block message, 448/896-bit boundaries, the million-`a` case) and `crates/fsm-core/tests/sha256_golden.rs` asserting one-shot and chunked-incremental digests agree with every vector.
2. Implement `Sha256 { new, update, finalize }`, `sha256(&[u8])`, `to_hex`, and `from_hex` (lowercase-only) in `crates/fsm-core/src/sha256.rs` per FIPS 180-4 — pure safe Rust, no lookup shortcuts beyond the standard constant tables.
3. Add inline unit tests for padding boundaries (55/56/63/64/65-byte messages) and hex round-trips.

- **Done when:** every NIST vector passes one-shot and incrementally (varied chunk sizes) under `cargo test -p fsm-core --test sha256_golden`, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
