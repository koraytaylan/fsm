---
id: output-multiplexer
title: "Output Multiplexer"
workstream: "0057"
kind: task
depends_on: []
gated: false
touches:
  - crates/fsm-cli/src/mcp/notify.rs
  - crates/fsm-cli/src/mcp/serve.rs
  - crates/fsm-cli/src/mcp/subscribe.rs
  - crates/fsm-cli/src/mcp/watch.rs
  - crates/fsm-cli/src/mcp/logging.rs
  - crates/fsm-cli/src/mcp/progress.rs
  - crates/fsm-cli/src/mcp/cancel.rs
  - crates/fsm-cli/src/mcp/mod.rs
  - crates/fsm-cli/tests/mcp_notifier.rs
  - crates/fsm-cli/src/mcp/jsonrpc.rs
  - crates/fsm-cli/tests/*.rs
status: done
merged_as: ""
---
# Output Multiplexer

`stdout` is the protocol stream and one stray byte inside a line is a protocol error, so before the server can speak from two places it needs exactly one thing that writes: a mutex held across the whole line and the flush.

**Steps:**

1. Create `crates/fsm-cli/src/mcp/notify.rs` and implement `pub struct Notifier { out: Arc<Mutex<Box<dyn Write + Send>>> }` with `new`, `clone_handle`, `send(&self, message: &Value)`, and `notify(&self, method: &str, params: Value)`.
2. **Scaffold every module this plan adds, here.** Create `subscribe.rs`, `watch.rs`, `logging.rs`, `progress.rs`, and `cancel.rs` carrying only the public type and function skeletons the architecture names, with `unimplemented!()` bodies, and declare all six modules in `crates/fsm-cli/src/mcp/mod.rs`. Declaring them once is what lets each later task stay inside its own single file, and it is the pattern plan 0008's `3601-crate-scaffold-and-skeleton` established for exactly this reason. A module cannot be declared without its file existing, so the shells and the declarations land together or neither compiles.
3. Hold the mutex across the **entire** write: canonical bytes, then `\n`, then `flush`, then release. A partial write outside the lock is the one bug this type exists to prevent, and the lock scope is the whole correctness argument — write it as a comment.
4. Keep the existing `debug_assert!` that a serialized message contains no newline, moved here from `send_line`.
5. Recover a poisoned mutex with `into_inner()` rather than panicking. A panicking notifier takes down a server whose protocol state is otherwise fine, and the existing panic hook already aborts on real bugs.
6. Make a write error non-fatal to a background producer: `send` returns `io::Result`, and a caller on a background thread records the failure and stops rather than unwinding. `stdout` closing means the client is gone, and the main loop will discover EOF on its own.
7. Make `Notifier` the **only** writer type in the process: `send_line` becomes its private implementation, and no other code writes to the protocol stream after this task.
8. **Tighten the transport bound and fix the callers.** `serve_session_with` takes `impl Write` today; boxing that writer as `Box<dyn Write + Send>` requires `impl Write + Send + 'static`. `StdoutLock<'static>` satisfies it, and so does a `Vec<u8>`, but every existing test caller that passes a borrowed buffer does not. Widen the signature and update those call sites in this task — discovering the bound three tasks later, in a task that does not own `serve.rs`, is the failure this step prevents.
9. Add `pub fn is_broken(&self) -> bool` so `5703`'s shutdown can tell a closed stream from a live one without attempting another write.

**Tests:**

- `crates/fsm-cli/tests/mcp_notifier.rs`: `send` writes exactly one canonical line terminated by a single `\n`, and flushes.
- **Interleaving:** spawn eight threads each sending 500 distinct messages through cloned handles into a shared buffer; every line in the result parses as a complete JSON-RPC message and the multiset of messages is exactly what was sent, with none truncated or merged.
- `notify` produces a message with `jsonrpc`, `method`, and `params` and **no** `id`, since a notification must not carry one.
- A poisoned mutex — poisoned by panicking a thread while it holds the lock — still allows subsequent sends.
- A writer that returns an error on write makes `send` return `Err` without panicking, and `is_broken` then reports true.
- A message containing a newline inside a string value is escaped by the canonical serializer and still occupies exactly one line.
- Two handles from `clone_handle` write to the same underlying stream.
- The whole existing MCP test suite compiles and passes against the widened `Write + Send + 'static` bound, with call sites updated in this commit.
- All six modules are declared in `mcp/mod.rs` and the crate compiles; `cargo doc --workspace --no-deps` is warning-free under `RUSTDOCFLAGS=-D warnings` with the shells in place.
- Every scaffolded shell carries the public items the architecture names, so a later task adds bodies rather than signatures — checked by referencing each named type or function from the test file.

- **Done when:** `cargo test -p fsm-cli --test mcp_notifier` passes every case above including the eight-thread interleaving property, `Notifier` is the only writer to the protocol stream, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.

**Landed:** `Notifier` with `new`, `clone_handle`, `send`, `notify`, and `is_broken`; the five module shells and their declarations; `jsonrpc::notification`; the widened bound on all four session entry points with `stdout()` taken by value rather than locked; and the suite — one line per send, eight threads × 500 messages arriving intact and exactly once, a notification with no `id`, a poisoned lock still writing, a broken stream reported rather than fatal, an escaped newline still occupying one line, and two handles sharing one stream.

**Corrections.** Step 8 says to fix the callers that pass a borrowed buffer, without saying what they should pass instead: an owned `Write + Send + 'static` that the caller can still read cannot be a `&mut Vec`. `SharedSink`/`SharedWriter` is that thing, and it lives in `notify.rs` beside the type that forced the bound — every test and the embed-acceptance caller now use it.
