---
id: dec-division
title: "Dec Division"
workstream: "0004"
kind: task
depends_on:
  - dec-rounding
gated: false
touches:
  - crates/fsm-core/src/decimal/mod.rs
  - crates/fsm-core/tests/fixtures/decimal/div_vectors.jsonl
status: planned
merged_as: ""
---
# Dec Division

Correctly-rounded division of the exact rational at the target scale — never double-rounded: widen the numerator into u256 and long-divide, or on the negative-k path fold powers of ten into the divisor, with the architecture's proven small-quotient rule covering the one case where that fold would overflow u128.

**Steps:**

1. Author `crates/fsm-core/tests/fixtures/decimal/div_vectors.jsonl` first: the architecture's worked `1/3` and `2/3` cases, repeating expansions at scales 0 and 12, exact divisions, ties in every mode and both signs, division by zero, the `k < 0` fold path, at least three fold-overflow cases (large-mantissa divisor with `k < 0`) asserting the `q = 0`-with-`bump` rule per mode, and a result exceeding the mantissa bound (`overflow`); the existing `decimal_golden` test picks the file up automatically.
2. Implement `div(a: Dec, b: Dec, scale: u8, mode: RoundMode) -> Result<Dec, DecError>` in `crates/fsm-core/src/decimal/mod.rs` exactly per the architecture procedure: `b` zero → `DivZero`; compute `k = scale − a.scale + b.scale`; for `k ≥ 0` widen `|a.mant|` into u256, `checked_mul_pow10(k)`, `div_rem_u128(|b.mant|)`; for `k < 0` fold `10^|k|` into the divisor, applying the stated overflow rule (fold exceeds u128 ⟹ `q = 0`, `r = |a.mant|`, and `2r < d` is guaranteed by the bound argument quoted in architecture) when it fires; decide the final digit with the shared `bump` from the rounding task using `(2r) cmp d`; re-apply the combined sign; bound-check the quotient.

- **Done when:** every line of `div_vectors.jsonl` passes — including the fold-overflow cases — under `cargo test -p fsm-core --test decimal_golden`, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
