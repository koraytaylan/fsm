---
id: crate-scaffold-and-skeleton
title: "Crate Scaffold And Skeleton"
workstream: "0036"
kind: task
depends_on: []
gated: false
touches:
  - Cargo.toml
  - crates/fsm-execute/Cargo.toml
  - crates/fsm-execute/src/lib.rs
  - crates/fsm-execute/src/config.rs
  - crates/fsm-execute/src/watch.rs
  - crates/fsm-execute/src/sched.rs
  - crates/fsm-execute/src/run.rs
  - crates/fsm-execute/src/service.rs
  - crates/fsm-execute/src/error.rs
status: planned
merged_as: ""
---
# Crate Scaffold And Skeleton

`fsm-execute` is a new library crate (not folded into `fsm-cli`) honouring the workspace's zero-dependency, `forbid(unsafe_code)`, blocking-only posture; this task lands the crate, its module layout, and the `exec/*` error type so later tasks fill modules in parallel.

**Steps:**

1. Add `"crates/fsm-execute"` to the workspace `members` in the root `Cargo.toml`.
2. Author `crates/fsm-execute/Cargo.toml` with `edition.workspace = true`, `rust-version.workspace = true`, `license.workspace = true`, `repository.workspace = true`, `[lints] workspace = true`, and path dependencies on `fsm-core` and `fsm-store` only (no third-party crates).
3. In `crates/fsm-execute/src/lib.rs`, write the crate doc comment (single-node, at-least-once-at-the-process-boundary, effects-are-an-outbox thesis), `#![forbid(unsafe_code)]`, and `pub mod config; pub mod watch; pub mod sched; pub mod run; pub mod service; pub mod error;`.
4. Create the seven module files. `config`, `watch`, `sched`, `run`, and `service` carry only the public type skeletons named in the architecture (`HandlerTable`/`HandlerSpec`, `Watcher`/`Observation`, `Scheduler`/`Directive`, `Runner`/`RunOutcome`, `tick`) with `unimplemented!()` bodies; `error.rs` lands in full.
5. Implement `error.rs`: `pub struct ExecError { pub code: &'static str, pub message: String, pub hint: Option<String>, pub details: Option<Value> }` (using `fsm_core::json::Value`), constructors `new`/`hint`/`details`, and `pub const ALL_CODES: &[&str]` listing the plan's codes (`exec/config`, `exec/spawn`, `exec/timeout`, `exec/store`, `exec/mode`, `exec/unhandled_effect`).

**Tests:**

- `cargo metadata` / build: the workspace resolves with `fsm-execute` as a member and the crate compiles.
- No new third-party dependency: `cargo tree -p fsm-execute --depth 1` lists only `fsm-core` and `fsm-store` as path deps.
- Lints: `cargo clippy -p fsm-execute -- -D warnings` is clean under the workspace lints (confirms `unsafe_code = forbid` and print denies are inherited).
- `error.rs`: `ALL_CODES` entries are unique, non-empty, and every one starts with the `exec/` prefix; `ExecError::new("exec/timeout", "...")` chains `.hint(...).details(...)` and retains all fields.

- **Done when:** the crate is a workspace member that compiles and lints clean, `error.rs` is fully implemented and unit-verified, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
