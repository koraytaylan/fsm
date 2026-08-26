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
status: done
merged_as: ""
---
# Machine Store

Machine definitions are immutable, content-addressed, and journaled: an identical spec re-created is a success with `created: false` and no new record, and references resolve by full id, unique hash prefix, or unambiguous bare name.

**Steps:**

1. Implement `Store { open, define_machine, resolve_machine }` in `crates/fsm-cli/src/store.rs` per architecture: open via `journal_io::open` with the history-index `RecordSink`, data-dir layout `{VERSION, journal/, snapshots/}` created on first run and version-checked afterward.
2. `define_machine`: canonicalize, validate through the plan-0003 spec pipeline, compute `machine_id = name@sha256:<hex>`; identical id → `{created: false}` with no append; new content → `machine_defined` appended; `dry_run` validates without appending; `if_exists_error` turns the idempotent case into an error for callers that want strictness.
3. `resolve_machine`: full id, unique hash prefix ≥ 12 hex characters, or bare name iff exactly one version — ambiguity returns `req/machine_ambiguous` listing every stored version in details.
4. Write the inline test module encoding exactly the inventory under **Tests**.

**Tests:**

- Inline in `store.rs` — idempotent define: defining the `case_review` spec twice yields `{created: true}` then `{created: false}` with the same `machine_id`, and the journal record count is unchanged by the second call.
- Version accretion: the same name with different content appends a second `machine_defined`; both versions resolve by full id afterward.
- `dry_run`: a valid spec returns the report with no append; an invalid spec returns its findings with no append — journal length identical before and after both.
- Strict mode: `if_exists_error` turns the identical-spec case into an error naming the existing id.
- Resolution paths: full id resolves; a unique ≥ 12-hex prefix (`name@sha256:<first 12>`) resolves; a bare name with exactly one version resolves; a bare name with two versions → `req/machine_ambiguous` whose details list *both* full version ids; an unknown reference → the not-found error naming the reference.
- Data-dir lifecycle: first open creates `{VERSION, journal/, snapshots/}`; reopening with a tampered `VERSION` value fails with the version-mismatch error before touching the journal.

- **Done when:** inline store tests prove idempotent content-addressed define and all four resolution behaviors, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
