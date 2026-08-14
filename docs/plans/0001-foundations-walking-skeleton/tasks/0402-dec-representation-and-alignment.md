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

1. Author `crates/fsm-core/tests/fixtures/decimal/align_vectors.jsonl` first (line schema per architecture) covering: parse widening (`"1.5"` at scale 2 → `1.50`) and over-precision rejection (`"1.505"` at scale 2), `-0.00` normalization, format round-trips at scales 0 and 12, add/sub alignment with overflow exactly at the mantissa bound, mul scale addition and `scale_cap`, and cmp pairs whose naive rescale would overflow i128 (the architecture's worked example included); plus `crates/fsm-core/tests/decimal_golden.rs`, which runs every line of every `*.jsonl` file in the fixtures directory so the rounding, division, and generated files of later tasks are picked up without edits.
2. Implement `Dec { mant: i128, scale: u8 }`, the `MAX_SCALE`/`MAX_MANT` constants, `DecError`, `parse`, `format`, `rescale_up` (checked ×10^Δ), `checked_add`, `checked_sub`, `checked_mul`, and u256-widened total `cmp` in `crates/fsm-core/src/decimal/mod.rs` exactly per architecture.
3. Add inline unit tests for each `DecError` path reachable in this task.

- **Done when:** every line of `align_vectors.jsonl` passes under `cargo test -p fsm-core --test decimal_golden`, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
