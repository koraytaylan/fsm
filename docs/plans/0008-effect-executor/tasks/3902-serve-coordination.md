---
id: serve-coordination
title: "Serve Coordination"
workstream: "0039"
kind: task
depends_on:
  - execute-subcommand
gated: false
touches:
  - crates/fsm-cli/src/args.rs
  - crates/fsm-cli/src/mcp/serve.rs
  - crates/fsm-cli/src/mcp/tools/mod.rs
  - crates/fsm-cli/src/mcp/tools/dispatch.rs
  - crates/fsm-cli/src/cli/execute.rs
  - crates/fsm-cli/tests/serve_modes.rs
status: done
merged_as: ""
---
# Serve Coordination

Resolves the plan's only real fork — who may write while the executor runs — by shipping the three modes from architecture §0039 with **paired as the default**: a read-only `fsm serve` lets the LLM watch the executor's journaled progress live while only the executor holds the writer lock; `embedded` is the opt-in simple-but-blocking path; `exclusive` is executor-alone.

**Steps:**

1. In `args.rs`, add `--read-only` and `--execute` switches plus a `--handlers` flag to the existing `SERVE` `CmdSpec`.
2. `serve --read-only` opens `Store::open_read_only` instead of `Store::open`. Declare the gated set once as `pub const MUTATING_TOOLS: &[&str]` next to the tool registry and have `dispatch` consult it, so the read-only gate is a table rather than six match arms and task `4101`'s doc test can import the same constant instead of restating the list. Thread the read-only fact through `serve` so that **all six** `ensure_writable()`-gated tools — `machine_create`, `instance_create`, `instance_send`, `deadline_poll`, `effect_ack`, `instance_cancel` — return a clean tool error (mapping `io/write`) whose hint says this serve is read-only and the executor owns writes. Count them from the code, not from memory: `machine_create` is the authoring path and the easy one to miss, though a `dry_run` create still validates without writing. The eight read tools (`machine_list`, `machine_get`, `machine_analyze`, `machine_diagram`, `instance_get`, `instance_list`, `instance_history`, `simulate`) work unchanged.
3. `serve --execute --handlers <file>` (embedded mode) calls `fsm_execute::service::tick_with` once per serve-loop iteration, passing serve's own writer handle — the lent-writer entry point exists precisely for this, since `tick`'s own `Store::open` would collide with the lock serve already holds. Document both limits in the code where they bite: a long-running handler blocks the protocol, and because `serve_session` blocks in `read_capped_line` until a client line arrives, **a tick happens only when the client speaks**, so embedded mode advances a workflow during a conversation, never overnight.
4. `fsm execute` (task `3901`) gains explicit mode selection: default `paired` (expects a read-only serve as the only other reader; acquires the writer per tick) and `--exclusive`, which additionally asserts at startup that it can take the writer lock and exits with `exec/mode` if something else already holds it instead of backing off. Both log the mode at startup.
5. Record the mode in one startup log line per process and in `serve`'s `instructions` adjunct, so an operator reading a transcript can tell which mode ran. That line is not part of any tick trace.

**Tests:**

- Read-only serve: `tools/call` for each name in `MUTATING_TOOLS` returns a clean tool error whose text/hint names read-only mode and that the executor owns writes; `machine_create` with `dry_run` still validates; `instance_get` and `instance_history` return normal results against a data dir the executor is concurrently writing (separate handles from the same test process).
- Read-only serve coexists with a writer: open serve `--read-only` and a writer `Store` in the same test → no `store/lock`.
- Non-read-only serve against a data dir the executor already holds the writer on → the second opener gets `store/lock` and renders it as the existing lock error (regression guard, proving single-writer still holds even within one process).
- Embedded mode: serve initialized with a machine whose instance emits an effect; after driving the input lines that advance into the effect, the serve process itself journals the `effect_acked` using the test-binary stub handler — no external executor involved. One further line drives the tick that sends the advance event, demonstrating the tick-on-traffic limit rather than hiding it.
- `--exclusive` against a data dir whose lock is already held exits with `exec/mode`; plain `paired` in the same situation logs `exec/store` and retries on a later tick rather than exiting.
- Mode visibility: each of the three startups logs its mode line, and the line is absent from `tick`'s action lines.

- **Done when:** `cargo test -p fsm-cli --test serve_modes` passes the `MUTATING_TOOLS` read-only, embedded, exclusive-assertion, contention, and mode-visibility rows, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
