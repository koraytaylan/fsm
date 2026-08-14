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

1. Author `crates/fsm-core/tests/fixtures/sha256/vectors.txt` and `crates/fsm-core/tests/sha256_golden.rs` first, encoding exactly the inventory under **Tests** (digests taken from the published NIST byte-oriented vector files, never computed by our own code).
2. Implement `Sha256 { new, update, finalize }`, `sha256(&[u8])`, `to_hex`, and `from_hex` (lowercase-only) in `crates/fsm-core/src/sha256.rs` per FIPS 180-4 — pure safe Rust, no lookup shortcuts beyond the standard constant tables.

**Tests:**

- `vectors.txt` message→digest lines from the NIST byte-oriented set, asserted by `sha256_golden.rs`: the empty message; `abc`; the 448-bit two-block message (`abcdbcdecdefdefg…`); the 896-bit message; messages of exactly 55, 56, 63, 64, and 65 bytes (the padding boundaries); and the 1,000,000 × `a` message.
- Incremental-equals-one-shot: for a fixed seeded 10 KiB buffer and for the million-`a` message, feeding `update` in chunk sizes 1, 3, 64, 65, and 4096 produces the same digest as the one-shot function.
- Hex helpers, inline: `to_hex ∘ from_hex` identity on a digest; `from_hex` rejects uppercase input, odd length, and non-hex characters with `None`.

- **Done when:** every NIST vector passes one-shot and in all listed chunk sizes under `cargo test -p fsm-core --test sha256_golden`, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
