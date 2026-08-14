---
id: dec-representation-and-alignment
title: "Dec Representation And Alignment"
workstream: "0004"
kind: task
depends_on:
  - u256-primitives
gated: false
touches:
  - crates/fsm-core/src/decimal/mod.rs
  - crates/fsm-core/tests/decimal_golden.rs
  - crates/fsm-core/tests/fixtures/decimal/align_vectors.jsonl
status: planned
merged_as: ""
---
# Dec Representation And Alignment

The decimal type itself: an i128 mantissa with a semantic (never-normalized) scale, exact parsing and canonical formatting, exact alignment for add/sub, scale-adding multiply, and value comparison across scales through u256 widening — everything except rounding and division, which follow as their own tasks.

**Steps:**

1. Author `crates/fsm-core/tests/fixtures/decimal/align_vectors.jsonl` (line schema per architecture) and `crates/fsm-core/tests/decimal_golden.rs` first, encoding exactly the inventory under **Tests**; the test runs every line of every `*.jsonl` file in the fixtures directory so the rounding, division, and generated files of later tasks are picked up without edits, and fails on an unparseable vector line rather than skipping it.
2. Implement `Dec { mant: i128, scale: u8 }`, the `MAX_SCALE`/`MAX_MANT` constants, `DecError`, `parse`, `format`, `rescale_up` (checked ×10^Δ), `checked_add`, `checked_sub`, `checked_mul`, and u256-widened total `cmp` in `crates/fsm-core/src/decimal/mod.rs` exactly per architecture.

**Tests:**

- `align_vectors.jsonl` — parse/format family: `"1.5"` at scale 2 → formats `"1.50"` (exact widening); `"1.505"` at scale 2 → `parse` error (never rounded); `"-0.00"` → `"0.00"` (sign-dropped zero); `"00.1"`, `"1."`, `".5"`, `"+1"`, `"1e5"` → `parse` errors; the maximum 38-digit mantissa at scale 12 round-trips; format emits exactly `scale` fraction digits at scales 0 and 12.
- Add/sub family: `1.5 + 0.25 = 1.75` (cross-scale alignment); `(10³⁸−1)` at scale 0 plus `1` → `overflow` (true result unrepresentable); subtraction crossing zero; alignment where the *smaller-scale* operand's rescale itself overflows → `overflow`.
- Mul family: `1.5 × 0.25 = 0.375` (scale 3 = 1+2); scale 7 × scale 6 → `scale_cap`; a mantissa-bound-exceeding product → `overflow`.
- Cmp family: `1.5 == 1.50` across scales; a strict ordering chain across three scales; the architecture's i128-overflow pair — `(10³⁸−1)` at scale 0 vs `0.000000000001` at scale 12 → `Greater`, and reversed → `Less`; negative comparisons mirror.
- Inline unit tests: every `DecError` variant reachable in this task is constructed at least once.

- **Done when:** every line of `align_vectors.jsonl` passes under `cargo test -p fsm-core --test decimal_golden`, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
