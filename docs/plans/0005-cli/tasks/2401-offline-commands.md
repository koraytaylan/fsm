---
id: offline-commands
title: "Offline Commands"
workstream: "0024"
kind: task
depends_on:
  - output-frame
gated: false
touches:
  - crates/fsm-cli/src/cli/offline.rs
status: planned
merged_as: ""
---
# Offline Commands

Validation, simulation, the embedded normative docs, and the version banner work without any store: pure core calls surfaced through the output frame, with `--context` values coerced by the machine's declared types rather than shell guessing.

**Steps:**

1. Fill `crates/fsm-cli/src/cli/offline.rs::SPECS` with `validate <spec.json|->` — pure definition validation, findings rendered with severities, exit 0/1.
2. Add `simulate <machine|spec.json> --events <events.json|-> [--context k=v ...] [--on-reject stop|continue]` — inline spec or stored machine, declared-type coercion for context overrides, per-step trace rendering, final configuration and context summary.
3. Add `docs [spec]` printing the embedded `docs/SPEC.md` via `include_str!`, and `version` printing `CARGO_PKG_VERSION`.
4. Add inline unit tests: a valid and an invalid spec through `validate` (exit codes 0/1), a two-event simulation with a rejection under `--on-reject continue`, and declared-scale coercion of a context override.

- **Done when:** inline offline-command tests prove validate exit codes, simulate traces incl. the continue-on-reject path, and typed context coercion, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
