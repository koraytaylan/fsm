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

1. Write the inline test module first (the type is crate-internal, so all tests live in `u256.rs`), encoding exactly the inventory under **Tests**.
2. Implement `U256 { hi: u128, lo: u128 }` with `from_u128`, `cmp` (hi then lo), `checked_mul_pow10(u32) -> Option<U256>` (a loop of the ×10 limb routine given verbatim in architecture), and `div_rem_u128(self, d: u128) -> (U256, u128)` (the 256-iteration restoring loop given verbatim in architecture, including the `hi_bit`/`wrapping_sub` rule and its stated invariant proof, reproduced as the module's doc comment).

**Tests:**

- Native cross-check sweep (seeded xorshift, fixed seed, ≥10,000 cases, all operands chosen to fit u128): `cmp` agrees with u128 ordering; `checked_mul_pow10(k)` agrees with `u128::checked_mul(10^k)` whenever the native product exists; `div_rem_u128` agrees with native `/` and `%`.
- 2¹²⁸-crossing, hard-coded expected `{hi, lo}` values: `from_u128(u128::MAX).checked_mul_pow10(1)`; a division whose dividend has `hi != 0` and whose quotient still exceeds u128; a division whose quotient lands back inside u128 (checked against a precomputed value).
- Division edges: divide by 1 returns `(self, 0)`; `from_u128(u128::MAX).div_rem_u128(u128::MAX)` → `(1, 0)`; dividend zero → `(0, 0)`; remainder is always `< d` asserted across the sweep.
- Overflow boundary: the exact smallest `k` for which `checked_mul_pow10` on a hard-coded near-max value returns `None`, and `k−1` on the same value returns `Some` — both pinned.
- The architecture's worked division example asserted digit for digit.

- **Done when:** the u256 inline tests pass — the native sweep, both 2¹²⁸-crossing directions, the division edges, and the overflow boundary — under `cargo test -p fsm-core u256`, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
