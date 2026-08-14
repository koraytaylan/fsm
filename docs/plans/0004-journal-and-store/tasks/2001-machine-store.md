---
id: machine-store
title: "Machine Store"
workstream: "0020"
kind: task
depends_on:
  - open-and-verify
gated: false
touches:
  - crates/fsm-cli/src/store.rs
status: planned
merged_as: ""
---
# Machine Store

Machine definitions are immutable, content-addressed, and journaled: an identical spec re-created is a success with `created: false` and no new record, and references resolve by full id, unique hash prefix, or unambiguous bare name.

**Steps:**

1. Implement `Store { open, define_machine, resolve_machine }` in `crates/fsm-cli/src/store.rs` per architecture: open via `journal_io::open` with the history-index `RecordSink`, data-dir layout `{VERSION, journal/, snapshots/}` created on first run and version-checked afterward.
2. `define_machine`: canonicalize, validate through the plan-0003 spec pipeline, compute `machine_id = name@sha256:<hex>`; identical id → `{created: false}` with no append; new content → `machine_defined` appended; `dry_run` validates without appending; `if_exists_error` turns the idempotent case into an error for callers that want strictness.
3. `resolve_machine`: full id, unique hash prefix ≥ 12 hex characters, or bare name iff exactly one version — ambiguity returns `req/machine_ambiguous` listing every stored version in details.
4. Add inline unit tests for idempotent define, version accretion under one name, dry-run non-persistence, and every resolution path including the ambiguity listing.

- **Done when:** inline store tests prove idempotent content-addressed define and all four resolution behaviors, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
