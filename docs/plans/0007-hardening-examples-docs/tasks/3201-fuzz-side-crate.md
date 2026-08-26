---
id: fuzz-side-crate
title: "Fuzz Side Crate"
workstream: "0032"
kind: chore
depends_on: []
gated: false
touches:
  - "fuzz/**"
status: done
merged_as: ""
---
# Fuzz Side Crate

The hand-rolled parsers deserve hostile bytes: this task adds the out-of-workspace cargo-fuzz crate — the one documented exception to zero dependencies, excluded from the shipped binary's graph by an empty `[workspace]` table — with six targets over JSON, expressions, decimals, canonicalization, journal records, and the serve loop.

**Steps:**

1. Create `fuzz/Cargo.toml` per architecture (`fsm-fuzz`, empty `[workspace]` table, `libfuzzer-sys`, path dependencies on `fsm-core` and `fsm-cli`, one `[[bin]]` per target) plus `fuzz/.gitignore` for `corpus/` and `artifacts/`.
2. Implement the six targets in `fuzz/fuzz_targets/`, each body asserting exactly the invariants under **Tests**.
3. Write `fuzz/README.md`: nightly usage (`cargo +nightly fuzz run <target>`), the corpus-seeding mapping from committed fixtures, and the crash-triage rule (minimize, commit as a regression fixture in the owning module's corpus).

**Tests:**

- Build acceptance (needs the nightly toolchain, outside the workspace gates): `cargo +nightly fuzz build` compiles all six targets; `cargo metadata --manifest-path fuzz/Cargo.toml --format-version 1` resolves `libfuzzer-sys` and lists the six bins; the workspace `zero_deps` guard still passes (the fuzz crate is invisible to the workspace graph).
- Per-target invariants, encoded in each target body (a violating input is a crash, i.e. a finding): `json_parse` — `fsm_core::json::parse` never panics, and on `Ok(v)` the canonical bytes re-parse to an equal value; `expr_parse` — lex+parse of UTF-8-lossy input never panics and every error span is within input bounds; `decimal_parse` — `Dec::parse` never panics, and on `Ok` `format ∘ parse` is identity; `canon_roundtrip` — accepted input canonicalizes twice byte-identically; `record_line` — the record parser/verifier never panics and never accepts a line whose recomputed hash mismatches; `jsonrpc_loop` — `serve` over the fuzz input (in-memory store, fixed clock) never panics and every output line is valid single-line JSON.
- Corpus seeding: each target's README-listed seed mapping points at the owning module's committed fixture directory (spot-checked as a manual review item — sustained fuzz runs are manual/CI, not part of the workspace gates).

- **Done when:** `cargo metadata --manifest-path fuzz/Cargo.toml --format-version 1` resolves `libfuzzer-sys` and lists all six bin targets while the workspace `zero_deps` guard still passes, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed for the workspace (the fuzz crate itself builds under its own nightly toolchain, outside these gates).
