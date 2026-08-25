---
id: crate-scaffold-and-skeleton
title: "Crate Scaffold And Skeleton"
workstream: "0036"
kind: task
depends_on: []
gated: false
touches:
  - Cargo.toml
  - Cargo.lock
  - crates/fsm-execute/Cargo.toml
  - crates/fsm-execute/src/lib.rs
  - crates/fsm-execute/src/config.rs
  - crates/fsm-execute/src/effect.rs
  - crates/fsm-execute/src/rid.rs
  - crates/fsm-execute/src/watch.rs
  - crates/fsm-execute/src/sched.rs
  - crates/fsm-execute/src/run.rs
  - crates/fsm-execute/src/service.rs
  - crates/fsm-execute/src/error.rs
  - crates/fsm-cli/tests/zero_deps.rs
status: planned
merged_as: ""
---
# Crate Scaffold And Skeleton

`fsm-execute` is a new library crate (not folded into `fsm-cli`) honouring the workspace's zero-dependency, `forbid(unsafe_code)`, blocking-only posture; this task lands the crate, its module layout, the `exec/*` error type, and the two registration points a fifth workspace crate must update or the suite goes red.

**Steps:**

1. Add `"crates/fsm-execute"` to the workspace `members` in the root `Cargo.toml`, and commit the regenerated `Cargo.lock` in the same change — `zero_deps.rs` shells `cargo metadata --locked`, so a stale lockfile fails the graph check.
2. Extend `WORKSPACE_CRATES` in `crates/fsm-cli/tests/zero_deps.rs` with `"fsm-execute"`. That constant is an exact set: the test panics on any package the resolved graph adds *or* drops, and CI runs it as its own `zero-deps` job.
3. Author `crates/fsm-execute/Cargo.toml` with `edition.workspace = true`, `rust-version.workspace = true`, `version.workspace = true`, `license.workspace = true`, `repository.workspace = true`, `[lints] workspace = true`, and path dependencies on `fsm-core` and `fsm-store` only (no third-party crates).
4. In `crates/fsm-execute/src/lib.rs`, write the crate doc comment (single-node, at-least-once-at-the-process-boundary, effects-are-an-outbox thesis), `#![forbid(unsafe_code)]`, and `pub mod config; pub mod effect; pub mod rid; pub mod watch; pub mod sched; pub mod run; pub mod service; pub mod error;`.
5. Create the eight module files. `config`, `effect`, `rid`, `watch`, `sched`, `run`, and `service` carry only the public type and function skeletons named in the architecture (`HandlerTable`/`HandlerSpec`/`Advance`, `PendingEffect`/`resolve`, `ack_rid`/`event_rid`/`poll_rid`, `Watcher`/`Observation`, `Scheduler`/`Directive`, `Runner`/`RunOutcome`, `tick`) with `unimplemented!()` bodies; `error.rs` lands in full. Declaring every module here is what lets the later tasks stay inside their own single file.
6. Implement `error.rs`: `pub struct ExecError { pub code: &'static str, pub message: String, pub hint: Option<String>, pub details: Option<Value> }` (using `fsm_core::json::Value`), constructors `new`/`hint`/`details`, and `pub const ALL_CODES: &[&str]` listing exactly the plan's eight codes (`exec/config`, `exec/effect_unresolved`, `exec/unhandled_effect`, `exec/spawn`, `exec/timeout`, `exec/cancelled`, `exec/store`, `exec/mode`). Task `4101`'s doc test asserts every entry here appears in `docs/EMBEDDING.md`, so the list is a commitment, not a scratch pad.

**Tests:**

- `cargo metadata` / build: the workspace resolves with `fsm-execute` as a member and the crate compiles; `cargo doc --workspace --no-deps` is warning-free under `RUSTDOCFLAGS=-D warnings`.
- `cargo test -p fsm-cli --test zero_deps` passes with the extended crate set and the committed lockfile.
- No new third-party dependency: `cargo tree -p fsm-execute --depth 1` lists only `fsm-core` and `fsm-store` as path deps.
- Lints: `cargo clippy -p fsm-execute -- -D warnings` is clean under the workspace lints (confirms `unsafe_code = forbid` and print denies are inherited); `scripts/oversized-files.sh` stays green.
- `error.rs`: `ALL_CODES` entries are unique, non-empty, and every one starts with the `exec/` prefix; `ExecError::new("exec/timeout", "...")` chains `.hint(...).details(...)` and retains all fields.

- **Done when:** the crate is a workspace member that compiles and lints clean, both registration points (`Cargo.lock`, `zero_deps.rs`) are updated, `error.rs` is fully implemented and unit-verified, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
