---
id: policy-gates
title: "Policy Gates"
workstream: "0001"
kind: chore
depends_on:
  - workspace-scaffold
  - json-structural-parser
gated: false
touches:
  - crates/fsm-core/clippy.toml
  - crates/fsm-cli/tests/policy.rs
  - crates/fsm-cli/tests/zero_deps.rs
status: done
merged_as: ""
---
# Policy Gates

The project's core rules — no unsafe, no third-party dependencies, no floats or clocks or hash-order nondeterminism in `fsm-core` — must be machine-checked from the start so they cannot decay into convention; this task adds the three gates, with the token scanner built as a pure function so the gate itself is unit-testable.

**Steps:**

1. Create `crates/fsm-core/clippy.toml` with `disallowed-types` for `std::collections::HashMap`, `std::collections::HashSet`, `std::time::SystemTime`, `std::time::Instant`, each with a reason string pointing at the purity rule.
2. Create `crates/fsm-cli/tests/policy.rs`: a pure `fn scan(source: &str) -> Vec<Violation>` implementing the banned-token rules (tokens: `f32`, `f64`, `SystemTime`, `Instant`, `HashMap`, `HashSet`, `std::fs`, `std::net`, `std::process`, `rand`, `unsafe`; `//` comments exempt; `POLICY_ALLOW:` with a justification suffix exempts a line, bare `POLICY_ALLOW` is itself a violation), plus a `#[test]` that walks `../fsm-core/src` and fails naming file and line on any violation.
3. Create `crates/fsm-cli/tests/zero_deps.rs`: run `cargo metadata --format-version 1 --locked`, parse the output with `fsm_core::json::parse`, and assert the resolved package set is exactly `fsm-core` and `fsm-cli`.

**Tests:**

- `policy.rs` inline unit tests over the pure scanner (no filesystem needed): a banned token on a code line → one violation with the right line number; the same token inside a `//` comment → clean; a token in a string literal on a code line → violation (crude by design, documented); `POLICY_ALLOW: reason text` on the offending line → clean; bare `POLICY_ALLOW` with no justification → violation; two violations in one sample → both reported.
- `policy.rs` tree test: scanning the real `../fsm-core/src` yields zero violations.
- `zero_deps.rs`: the metadata package set equals `{fsm-core, fsm-cli}` exactly — asserted by name; any extra package fails with its name in the message.
- Falsification checks (manual, named in the done-when): temporarily adding `use std::collections::HashMap;` to `crates/fsm-core/src/lib.rs` must fail `cargo clippy -p fsm-core -- -D warnings` (clippy.toml) *and* the policy tree test (scanner) — both nets, independently; change reverted before commit.

- **Done when:** `cargo test -p fsm-cli --test policy --test zero_deps` passes, the temporary-`HashMap` falsification fails both clippy and the policy test (change reverted), and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
