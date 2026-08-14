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

1. Author `crates/fsm-core/tests/fixtures/decimal/div_vectors.jsonl` first, encoding exactly the inventory under **Tests**; the existing `decimal_golden` test picks the file up automatically.
2. Implement `div(a: Dec, b: Dec, scale: u8, mode: RoundMode) -> Result<Dec, DecError>` in `crates/fsm-core/src/decimal/mod.rs` exactly per the architecture procedure: `b` zero → `DivZero`; `k = scale − a.scale + b.scale`; `k ≥ 0` widens `|a.mant|` into u256, `checked_mul_pow10(k)`, `div_rem_u128(|b.mant|)`; `k < 0` folds `10^|k|` into the divisor, applying the stated overflow rule (`q = 0`, `r = |a.mant|`, `2r < d` guaranteed) when the fold exceeds u128; final digit via the shared `bump` with `(2r) cmp d`; combined sign; quotient bound-checked.

**Tests:**

- `div_vectors.jsonl` — worked block: `div(1, 3, 4, half_even)` → `0.3333` and `div(2, 3, 4, half_even)` → `0.6667` (the architecture's traced cases), plus both with negated operands in each position (sign combinations).
- Exact divisions: `1 / 4` at scale 2 → `0.25`; `10 / 2` at scale 0 → `5`; remainders of zero never consult the mode (same result in all seven).
- Tie rows with parity: `1 / 8` at scale 2 (q = 12, tie: `half_up` → `0.13`, `half_down` → `0.12`, `half_even` → `0.12` even) and `3 / 8` at scale 2 (q = 37, tie: `half_even` → `0.38` odd) — every mode pinned on both rows.
- Repeating expansions at scale 12 (`1/3`, `1/7`) against independently computed digits.
- Negative-k fold path: `5.000000000000` (scale 12) ÷ `2` at scale 0 — k = −12, folded divisor `2·10¹²`, an exact tie resolved per mode.
- Fold-overflow rows (≥3): a divisor mantissa near `10³⁸` with k = −12, asserting the `q = 0` rule per mode — half modes and `down` → `0`, `up` → one ulp away from zero, `floor`/`ceiling` by sign — for positive and negative dividends.
- Errors: division by zero → `div_zero`; `(10³⁸−1)` at scale 0 ÷ `0.000000000001` at scale 12 at scale 0 → `overflow` (quotient exceeds the mantissa bound).

- **Done when:** every line of `div_vectors.jsonl` passes — including the fold-overflow and tie-parity rows — under `cargo test -p fsm-core --test decimal_golden`, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
