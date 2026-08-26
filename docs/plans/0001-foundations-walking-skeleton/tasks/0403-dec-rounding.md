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
status: done
merged_as: ""
---
# Dec Rounding

All rounding in the system — `round` here and division in the next task — flows through one shared decision function transcribed from the architecture's seven-row mode table, so a mode can never behave differently in two places.

**Steps:**

1. Author `crates/fsm-core/tests/fixtures/decimal/round_vectors.jsonl` first, encoding exactly the inventory under **Tests**; the existing `decimal_golden` test picks the file up automatically.
2. Implement `RoundMode` and the single decision function `fn bump(mode: RoundMode, negative: bool, twice_rem_vs_divisor: Ordering, q_is_even: bool) -> bool` in `crates/fsm-core/src/decimal/mod.rs` as a verbatim transcription of the architecture table (`r == 0` never reaches `bump`).
3. Implement `round(self, scale: u8, mode: RoundMode) -> Result<Dec, DecError>`: upscale = `rescale_up`; downscale divides the magnitude by 10^Δ producing `(q, r)`, calls `bump` with `(2r) cmp 10^Δ` and `q`'s parity, then re-applies the sign — exactly the architecture's worked walk-through.

**Tests:**

- `round_vectors.jsonl` — the exact-tie block: `2.345` to scale 2 in all seven modes (per the architecture table: `down`/`floor`/`half_down`/`half_even` → `2.34`, `up`/`ceiling`/`half_up` → `2.35`) and `-2.345` in all seven (`floor` → `-2.35`, `ceiling` → `-2.34`, `half_even` → `-2.34`, the rest mirroring the magnitude rule).
- Parity pair for `half_even`: `2.345` → `2.34` (even quotient stays) versus `2.355` → `2.36` (odd quotient bumps).
- Non-ties both sides: `2.344` and `2.346` to scale 2 in every mode (only the directional modes differ from truncation on `2.344`; all half modes agree with nearest).
- Upscale: exact widen `1.5` → scale 4 = `1.5000` in any mode; widening `(10³⁸−1)` at scale 0 to scale 1 → `overflow`.
- Edges: `0` and `-0.00` to any scale → `0` at the target scale; a scale-0 target (`2.5` → all seven modes, the classic integer-rounding row); a multi-digit drop (`2.9999` → scale 0) where `2r` vs `10^Δ` uses the full remainder, not the first dropped digit.
- Inline unit test: `bump` itself, all seven modes × {`Less`, `Equal`, `Greater`} × parity × sign — the full 84-cell truth table asserted against the architecture table (cheap, and makes any transcription slip a named cell).

- **Done when:** every line of `round_vectors.jsonl` and the 84-cell `bump` truth table pass under `cargo test -p fsm-core --test decimal_golden` and `cargo test -p fsm-core bump`, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
