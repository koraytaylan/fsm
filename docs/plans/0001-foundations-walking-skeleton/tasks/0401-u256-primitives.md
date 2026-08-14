---
id: u256-primitives
title: "U256 Primitives"
workstream: "0004"
kind: task
depends_on:
  - workspace-scaffold
gated: false
touches:
  - crates/fsm-core/src/decimal/u256.rs
status: planned
merged_as: ""
---
# U256 Primitives

Decimal comparison and division need integers wider than i128; this task lands the four u256 operations as verbatim transcriptions of the architecture's given algorithms — multiply-by-ten with explicit limb carries, and restoring bit-by-bit division with the high-bit wrapping rule — each cross-checked against native u128 arithmetic wherever operands and results fit.

**Steps:**

1. Implement `U256 { hi: u128, lo: u128 }` with `from_u128`, `cmp` (hi then lo), `checked_mul_pow10(u32) -> Option<U256>` (a loop of the ×10 limb routine given verbatim in architecture), and `div_rem_u128(self, d: u128) -> (U256, u128)` (the 256-iteration restoring loop given verbatim in architecture, including the `hi_bit`/`wrapping_sub` rule and its stated invariant proof) in `crates/fsm-core/src/decimal/u256.rs`.
2. Add inline unit tests: for a seeded sweep of u128-representable operands, every operation agrees with native u128 arithmetic; boundary cases crossing 2¹²⁸ (`from_u128(u128::MAX).checked_mul_pow10(1)`, division whose quotient exceeds u128); division by 1 and by `u128::MAX`; `checked_mul_pow10` returning `None` exactly when the true value exceeds 2²⁵⁶−1; and the worked division example from architecture asserted digit for digit.

- **Done when:** the u256 unit tests pass, including the u128 cross-check sweep and both 2¹²⁸-crossing directions, under `cargo test -p fsm-core u256`, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
