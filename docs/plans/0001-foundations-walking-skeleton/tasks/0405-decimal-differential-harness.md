---
id: decimal-differential-harness
title: "Decimal Differential Harness"
workstream: "0004"
kind: task
depends_on:
  - dec-division
gated: false
touches:
  - tools/gen_decimal_vectors.py
  - crates/fsm-core/tests/fixtures/decimal/generated_vectors.jsonl
status: planned
merged_as: ""
---
# Decimal Differential Harness

An independent oracle — Python integer arithmetic with explicit remainder-based rounding, deliberately not `decimal.quantize` — generates a large deterministic vector file that the decimal module must match, so our implementation and the reference cannot share a bug.

**Steps:**

1. Write `tools/gen_decimal_vectors.py` (Python 3 stdlib only): exact rational results via integer quotient/remainder with per-mode rounding implemented in integer space (the same seven-row decision table, independently transcribed), a fixed seed and no wall-clock input, with the built-in self-checks listed under **Tests**.
2. Run it to produce `crates/fsm-core/tests/fixtures/decimal/generated_vectors.jsonl`, sorted and byte-stable, and commit the file (the existing `decimal_golden` test picks up every `*.jsonl` automatically).
3. Document at the top of the script how to regenerate and that a diff on regeneration is a release blocker.

**Tests:**

- The committed `generated_vectors.jsonl` passes in full under the existing `decimal_golden` runner — this is the differential assertion: every Rust result equals the Python integer-oracle result.
- Generator self-checks (the script exits nonzero if any fails, so a bad vector file cannot be produced): coverage counters — ≥5,000 random cases, every op × every mode represented, ≥1 exact-tie case per mode per op with a remainder, ≥3 fold-overflow division rows, boundary mantissas (±(10³⁸−1)) present in each op family; the integer oracle and a second reference (`decimal` with a 60-digit context) agree wherever the second applies, and any disagreement aborts generation naming the case.
- Determinism check (in the done-when): two consecutive runs produce byte-identical files matching the committed one — proving no wall-clock, hash-order, or set-iteration leak in the generator.
- Sanity rows pinned by hand inside the script's self-test: the worked `2.345` rounding set and the `1/3`, `2/3` division cases must appear in the output with exactly the architecture's values (guarding against a transcription slip in the *oracle* itself).

- **Done when:** running `python3 tools/gen_decimal_vectors.py` twice produces byte-identical output matching the committed file with all self-checks green, all generated vectors pass under `cargo test -p fsm-core --test decimal_golden`, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
