---
id: fuzz-side-crate
title: "Fuzz Side Crate"
workstream: "0032"
kind: chore
depends_on: []
gated: false
touches:
  - "fuzz/**"
status: planned
merged_as: ""
---
# Fuzz Side Crate

The hand-rolled parsers deserve hostile bytes: this task adds the out-of-workspace cargo-fuzz crate — the one documented exception to zero dependencies, excluded from the shipped binary's graph by an empty `[workspace]` table — with six targets over JSON, expressions, decimals, canonicalization, journal records, and the serve loop.

**Steps:**

1. Create `fuzz/Cargo.toml` per architecture (`fsm-fuzz`, empty `[workspace]` table, `libfuzzer-sys`, path dependencies on `fsm-core` and `fsm-cli`, one `[[bin]]` per target) plus `fuzz/.gitignore` for `corpus/` and `artifacts/`.
2. Implement the six targets in `fuzz/fuzz_targets/` — `json_parse.rs`, `expr_parse.rs`, `decimal_parse.rs`, `canon_roundtrip.rs`, `record_line.rs`, `jsonrpc_loop.rs` — each with the no-panic and consistency assertions named in architecture.
3. Write `fuzz/README.md`: nightly usage (`cargo +nightly fuzz run <target>`), corpus seeding from committed fixtures, and the crash-triage rule (minimize, commit as a regression fixture in the owning module's corpus).

- **Done when:** `cargo metadata --manifest-path fuzz/Cargo.toml --format-version 1` resolves `libfuzzer-sys` and lists all six bin targets while the workspace `zero_deps` guard still passes, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed for the workspace (the fuzz crate itself builds under its own nightly toolchain, outside these gates).
