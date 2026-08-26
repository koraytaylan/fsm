---
id: workspace-scaffold
title: "Workspace Scaffold"
workstream: "0001"
kind: chore
depends_on: []
gated: false
touches:
  - Cargo.toml
  - rust-toolchain.toml
  - "crates/fsm-core/Cargo.toml"
  - "crates/fsm-core/src/**"
  - "crates/fsm-cli/Cargo.toml"
  - crates/fsm-cli/src/lib.rs
  - crates/fsm-cli/src/main.rs
  - "crates/fsm-cli/src/mcp/**"
status: done
merged_as: ""
---
# Workspace Scaffold

The repository has no cargo workspace yet; this task writes every manifest the project will ever have (zero external dependencies makes manifests final) plus compilable module stubs, so no later task edits a `Cargo.toml` or `lib.rs`.

**Steps:**

1. Create root `Cargo.toml` (`[workspace]`, `members = ["crates/fsm-core", "crates/fsm-cli"]`, `resolver = "3"`, `[workspace.package]` with `edition = "2024"`, `rust-version = "1.89"`, an explicit release version, `license = "MIT OR Apache-2.0"`, and `[workspace.lints.rust] unsafe_code = "forbid"`) and `rust-toolchain.toml` (`channel = "1.89.0"`, `components = ["clippy", "rustfmt"]`) exactly as pinned in architecture.
2. Create `crates/fsm-core/Cargo.toml` (no `[dependencies]` table, `[lints] workspace = true`) and `crates/fsm-cli/Cargo.toml` (`[[bin]] name = "fsm"`, sole dependency `fsm-core` by path, `[lints] workspace = true`, `[lints.clippy] print_stdout = "deny"` and `print_stderr = "deny"`).
3. Create `crates/fsm-core/src/lib.rs` with `#![forbid(unsafe_code)]`, the purity-rule crate docs, and declarations `pub mod json; pub mod sha256; pub mod decimal; pub mod canon; pub mod ident; pub mod limits; pub mod error;`, plus the stub files `src/json/{mod,value,parse,write}.rs`, `src/sha256.rs`, `src/decimal/{mod,u256}.rs`, `src/canon.rs`, `src/ident.rs`, `src/limits.rs`, `src/error.rs` (module docs only).
4. Create `crates/fsm-cli/src/lib.rs` (`#![forbid(unsafe_code)]`, declaring `pub mod mcp;` — the crate carries a library target alongside the binary so integration tests can import `fsm_cli::…`; cargo auto-detects it with no manifest change), `src/mcp/{mod,jsonrpc,serve}.rs` stubs, and a thin `src/main.rs` (dispatch `serve` to `fsm_cli::mcp::serve::run()`, a stderr "not yet implemented" stub exiting 2; anything else prints one usage line to stderr and exits 2).

**Tests:**

- No dedicated test file — this task's acceptance is mechanical and fully covered by the commands in the done-when: `cargo metadata --format-version 1` resolves exactly the two members `fsm-core` and `fsm-cli` (no third package anywhere in the graph); `cargo build` produces a binary named `fsm`; `./target/debug/fsm` with no arguments exits 2 and prints the usage line to stderr only (nothing on stdout); `./target/debug/fsm serve` exits 2 with the not-yet-implemented message on stderr.
- The three gates (`cargo test`, `cargo clippy --workspace -- -D warnings`, `cargo fmt --check`) run clean on the empty scaffold — proving the lint tables and stubs are well-formed before any real code exists.

- **Done when:** `cargo metadata --format-version 1` resolves exactly the two workspace members, `./target/debug/fsm` exits 2 with stderr-only usage, and `cargo build`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed with a binary named `fsm` produced.
