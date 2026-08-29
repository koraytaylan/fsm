---
id: execute-surface-boundary
title: "Execute Surface Boundary"
workstream: "0088"
kind: task
depends_on: []
gated: false
touches:
  - crates/fsm-execute/tests/public_surface.rs
  - crates/fsm-execute/tests/public_surface/scanner.rs
  - crates/fsm-execute/tests/fixtures/public_surface.txt
  - docs/API-POLICY.md
status: done
merged_as: ""
---
# Execute Surface Boundary

`fsm-execute` is labelled provisional because it has no outside-workspace acceptance check, which is honest — and currently means the label covers whatever the crate happens to expose this release.

**Steps:**

1. Create `crates/fsm-execute/tests/public_surface.rs` comparing the crate's public items against a committed inventory at `crates/fsm-execute/tests/fixtures/public_surface.txt`.
2. Generate the inventory by **scanning the crate's own source files** for `pub` items, in a stable, sorted, deterministic form: module path, item kind, and name for every public item, including public fields and enum variants, since those are what a downstream actually breaks on. (The scanner and the gate together exceed the thousand-line ceiling, so the scanner is a `#[path]` sibling at `tests/public_surface/scanner.rs`; the module doc and the boundary tests stay in the root.)
3. Source scanning is the approach because there is no other one available here. `cargo public-api` is a third-party dependency and the workspace has none; `cargo doc --output-format json` is nightly-only and CI runs stable and the MSRV. Hand-rolling the scanner is the same answer this project already gave for JSON, SHA-256, JSON-RPC, and MCP framing, and it is the only one consistent with the charter.
4. Write down what the scanner cannot see, in its module doc, so nobody mistakes it for a complete public-API tool: items produced by macro expansion, and re-exports that widen visibility from a private module. Both are absent from `fsm-execute` today; the note is what makes their arrival visible rather than silent. A test that overstates its own coverage is worse than one that states its limits.
5. A public item that is not in the inventory **fails the test**. That is the whole mechanism: an addition becomes a decision somebody records rather than one that accumulates between releases.
6. Support regeneration through `FSM_REGEN_FIXTURES=1`, this repository's established idiom, so updating the inventory is a reviewable diff in the same commit as the change that widened the surface.
7. Add one sentence to `docs/API-POLICY.md`'s `fsm-execute` row: the provisional surface is now enumerated, and where the inventory lives. **Do not stabilise the crate.** It still has no outside-workspace acceptance check, and plan 0017 moves the store underneath it; the honest position this release is a bounded provisional surface, not a promise.
8. Do not apply this technique to `fsm-core` or `fsm-store`. Both already have acceptance checks that fail when their surface regresses, which is a stronger guarantee than an inventory, and adding a second mechanism where a better one exists is the speculative generality `CONTRIBUTING.md` warns against. Say so in the test's module doc, so the next reader does not helpfully extend it.
9. Change no code in `crates/fsm-execute/src/`. This task observes the surface; it does not adjust it.

**Tests:**

- `crates/fsm-execute/tests/public_surface.rs` passes against the committed inventory.
- Adding a public item makes the test fail — asserted by a case that feeds the comparator a surface with one extra item, so the mechanism is proved without committing a throwaway public item.
- Removing a public item also fails, since a removal is a break a downstream needs to see too.
- The inventory covers public fields and enum variants, not only types and functions — asserted against a known type in the crate that has both.
- The inventory is deterministic: generating it twice produces identical bytes, and the ordering is stable regardless of source order.
- The scanner ignores `pub` inside comments and string literals, and does not mistake `pub(crate)` or `pub(super)` for public — three cases, since each is a way the inventory could silently overstate or understate the surface.
- The scanner's documented limitations are present in its module doc, asserted by the test so the note cannot be deleted while the test passes.
- `FSM_REGEN_FIXTURES=1` regenerates it, and the regenerated file is byte-identical to the committed one on an unchanged crate.
- `docs/API-POLICY.md` still marks the crate provisional — assert the word is present, so a later edit cannot quietly promote it while this test passes.
- `crates/fsm-execute/src/` is unchanged by this task, asserted by diff.
- `cargo test -p fsm-embed-acceptance` and `cargo test -p fsm-cli --test zero_deps` pass unchanged.

- **Done when:** `cargo test -p fsm-execute --test public_surface` passes, the comparator provably fails on both an added and a removed item, the scanner distinguishes `pub` from `pub(crate)` and ignores comments and strings, its limitations are documented and asserted, the inventory is deterministic and covers fields and variants, regeneration works through the standard idiom, `API-POLICY.md` still says provisional and now says where the boundary is, no source file changed, and `cargo test`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo fmt --check` succeed.
