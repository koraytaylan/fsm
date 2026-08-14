---
id: decimal-differential-harness
title: "Decimal Differential Harness"
workstream: "0004"
kind: task
depends_on:
  - decimal-core
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

1. Write `tools/gen_decimal_vectors.py` (Python 3 stdlib only): exact rational results via integer quotient/remainder with per-mode rounding implemented in integer space, a fixed seed and no wall-clock input, covering boundary mantissas, all mode-by-tie combinations, and ~5,000 seeded random cases across scales 0–12.
2. Run it to produce `crates/fsm-core/tests/fixtures/decimal/generated_vectors.jsonl`, sorted and byte-stable, and commit the file (the existing `decimal_golden` test picks up every `*.jsonl` automatically).
3. Document at the top of the script how to regenerate and that a diff on regeneration is a release blocker.

- **Done when:** running `python3 tools/gen_decimal_vectors.py` twice produces byte-identical output matching the committed file, all generated vectors pass under `cargo test -p fsm-core --test decimal_golden`, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
