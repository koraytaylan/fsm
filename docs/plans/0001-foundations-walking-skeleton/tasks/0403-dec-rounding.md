---
id: dec-rounding
title: "Dec Rounding"
workstream: "0004"
kind: task
depends_on:
  - dec-representation-and-alignment
gated: false
touches:
  - crates/fsm-core/src/decimal/mod.rs
  - crates/fsm-core/tests/fixtures/decimal/round_vectors.jsonl
status: planned
merged_as: ""
---
# Dec Rounding

All rounding in the system — `round` here and division in the next task — flows through one shared decision function transcribed from the architecture's seven-row mode table, so a mode can never behave differently in two places.

**Steps:**

1. Author `crates/fsm-core/tests/fixtures/decimal/round_vectors.jsonl` first: the architecture's worked `2.345` set across all seven modes in both signs, non-tie cases both sides of the halfway point, upscale (exact widen) cases including one overflowing at the mantissa bound, zero and `-0` inputs, and scale-0 targets; the existing `decimal_golden` test picks the file up automatically.
2. Implement `RoundMode` and the single decision function `fn bump(mode: RoundMode, negative: bool, twice_rem_vs_divisor: Ordering, q_is_even: bool) -> bool` in `crates/fsm-core/src/decimal/mod.rs` as a verbatim transcription of the architecture table (`r == 0` never reaches `bump`).
3. Implement `round(self, scale: u8, mode: RoundMode) -> Result<Dec, DecError>`: upscale = `rescale_up`; downscale divides the magnitude by 10^Δ producing `(q, r)`, calls `bump` with `(2r) cmp 10^Δ` and `q`'s parity, then re-applies the sign — exactly the architecture's worked walk-through.

- **Done when:** every line of `round_vectors.jsonl` passes under `cargo test -p fsm-core --test decimal_golden`, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
