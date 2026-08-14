---
id: policy-gates
title: "Policy Gates"
workstream: "0001"
kind: chore
depends_on:
  - workspace-scaffold
  - json-value-and-parser
gated: false
touches:
  - crates/fsm-core/clippy.toml
  - crates/fsm-cli/tests/policy.rs
  - crates/fsm-cli/tests/zero_deps.rs
status: planned
merged_as: ""
---
# Policy Gates

The project's core rules — no unsafe, no third-party dependencies, no floats or clocks or hash-order nondeterminism in `fsm-core` — must be machine-checked from the start so they cannot decay into convention; this task adds the three gates.

**Steps:**

1. Create `crates/fsm-core/clippy.toml` with `disallowed-types` for `std::collections::HashMap`, `std::collections::HashSet`, `std::time::SystemTime`, `std::time::Instant`, each with a reason string pointing at the purity rule.
2. Create `crates/fsm-cli/tests/policy.rs`: walk `../fsm-core/src`, scan every `.rs` line outside `//` comments for the banned tokens (`f32`, `f64`, `SystemTime`, `Instant`, `HashMap`, `HashSet`, `std::fs`, `std::net`, `std::process`, `rand`, `unsafe`), failing with file and line; support a `POLICY_ALLOW:` justification-comment escape hatch that itself fails when used without a justification suffix.
3. Create `crates/fsm-cli/tests/zero_deps.rs`: run `cargo metadata --format-version 1 --locked`, parse the output with `fsm_core::json::parse`, and assert the resolved package set is exactly `fsm-core` and `fsm-cli`.

- **Done when:** `cargo test -p fsm-cli --test policy --test zero_deps` passes, and temporarily inserting `use std::collections::HashMap;` into `crates/fsm-core/src/lib.rs` makes `cargo clippy -p fsm-core -- -D warnings` fail (change reverted), with `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` green.
