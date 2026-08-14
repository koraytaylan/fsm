---
id: decimal-core
title: "Decimal Core"
workstream: "0004"
kind: task
depends_on:
  - workspace-scaffold
gated: false
touches:
  - crates/fsm-core/src/decimal/mod.rs
  - crates/fsm-core/src/decimal/u256.rs
  - crates/fsm-core/tests/decimal_golden.rs
  - crates/fsm-core/tests/fixtures/decimal/starter_vectors.jsonl
status: planned
merged_as: ""
---
# Decimal Core

All arithmetic in the engine is exact fixed-point decimal on an i128 mantissa — no float exists anywhere — with comparison and division widened through a hand-rolled u256 so that representable values always compare and divide correctly; a hand-authored tie-and-boundary vector file lands before the implementation.

**Steps:**

1. Author `crates/fsm-core/tests/fixtures/decimal/starter_vectors.jsonl` first (~100 lines, schema per architecture) covering every rounding mode at exact ties in both signs, mantissa bounds at ±(10³⁸−1), alignment overflow at the boundary, comparisons whose naive rescale would overflow i128, repeating divisions (1/3, 1/7) at scales 0 and 12, the negative-`k` division path, and `-0.00` normalization; plus `crates/fsm-core/tests/decimal_golden.rs`, which runs every line of every `*.jsonl` file in the fixtures directory.
2. Implement `U256 { from_u128, checked_mul_pow10, cmp, div_rem_u128 }` in `crates/fsm-core/src/decimal/u256.rs` with exhaustive unit tests against u128-representable cases.
3. Implement `Dec`, `RoundMode` (down, up, floor, ceiling, half_up, half_down, half_even), `DecError`, and the operations in `crates/fsm-core/src/decimal/mod.rs` exactly per architecture: checked add/sub with exact alignment, mul with scale addition, u256-widened total `cmp`, `round` with per-mode remainder logic and half-even parity, and `div` computing the correctly-rounded exact rational (never double-rounded), plus `parse`/`format` in the canonical string form.
4. Add inline unit tests for each `DecError` path.

- **Done when:** every line of `starter_vectors.jsonl` passes under `cargo test -p fsm-core --test decimal_golden`, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
