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
2. Add `simulate <machine|spec.json> --events <events.json|-> [--context k=v ...] [--on-reject stop|continue]` — inline spec or stored machine, declared-type coercion for context overrides, per-step trace rendering, final configuration and context summary. Simulation is a what-if report: a rejected step is *content*, not a command failure, so the command exits 0 with the rejection (and `stopped_at` under `stop`) rendered.
3. Add `docs [spec]` printing the embedded `docs/SPEC.md` via `include_str!`, and `version` printing `CARGO_PKG_VERSION`.
4. Write the inline test module encoding exactly the inventory under **Tests** (calling the spec `run` functions directly with capture buffers — no store, no binary spawn).

**Tests:**

- Inline in `offline.rs` — `validate`: the `case_review` reference spec → exit 0 with the summary rendered; a spec with an unknown `to` state → exit 1, the `def/unknown_state` finding rendered with its path and hint; input arrives via both a file argument and `-` (stdin).
- `simulate` happy path: the reference spec with `docs_ok` then `suspend` → two per-step traces, final leaf `suspended`, exit 0.
- `simulate` rejection handling: a three-event sequence whose middle event rejects — under `--on-reject continue` all three steps render (the rejection trace visible mid-run) and the final configuration reflects events 1 and 3; under `stop` (the default) rendering ends at the rejection with `stopped_at` reported; both exit 0 (a simulated rejection is a finding, not a failure).
- Declared-type coercion: `--context visits=2` parses as the declared int; a decimal-typed override given `1.5` at declared scale 2 is stored as `1.50`; an over-precision value (`1.505` at scale 2) → the `req/field_scale` error rendered with its hint, exit 1.
- `docs`: stdout equals the embedded `docs/SPEC.md` bytes exactly (byte-compare against `include_str!` in the test); `version`: stdout equals `CARGO_PKG_VERSION` plus one newline.

- **Done when:** inline offline-command tests prove validate exit codes, simulate traces incl. the continue-on-reject path, and typed context coercion, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
