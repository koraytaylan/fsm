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
  - crates/fsm-cli/src/main.rs
  - crates/fsm-cli/src/mcp/serve.rs
  - crates/fsm-cli/src/cli/execute.rs
  - crates/fsm-cli/tests/serve_modes.rs
status: planned
merged_as: ""
---
# Serve Coordination

Resolves the plan's only real fork — who may write while the executor runs — by shipping the three modes from architecture §0039 with **paired as the default**: a read-only `fsm serve` lets the LLM watch the executor's journaled progress live while only the executor holds the writer lock; `embedded` is the opt-in simple-but-blocking path; `exclusive` is executor-alone.

**Steps:**

1. In `args.rs`, add `fsm serve --read-only` and `fsm serve --execute --handlers <file>`.
2. `serve --read-only` opens `Store::open_read_only` instead of `Store::open`. Thread a `read_only: bool` (or the store variant) through `serve` so the four mutating tools and `deadline_poll` — whose `run` fns call `ensure_writable()`-gated mutators — return a clean tool error (mapping `io/write` → a `req/`-style object) whose `hint` reads that this serve is read-only and the executor owns writes. Read tools (`instance_get`, `instance_history`, `instance_list`, `simulate`, `machine_*`) work unchanged.
3. `serve --execute --handlers <file>` (embedded mode) runs the *same* `fsm_execute` scheduler/runner/pipeline on the serve thread between input reads: it starts effects inline and documents that a long-running handler blocks the protocol. It reuses 100% of the library — only the driver differs (an embedded tick invoked per serve-loop iteration against serve's own writer handle, which it already holds while initialized).
4. `fsm execute` (from task 3901) gains explicit mode selection: default `paired` (expects a read-only serve to be the only concurrent reader; acquires writer per-tick) and `--exclusive` (asserts it is the sole process; still uses per-tick writer acquisition). Both log the mode at startup; `paired` + `exclusive` differ only in the startup assertion and log line.
5. Record the mode in one startup log line per process and in `serve`'s `serverInfo`/`instructions` adjunct so the proof transcript can assert which mode ran.

**Tests:**

- Read-only serve: `tools/call instance_send` → a clean tool error whose text/hint names read-only mode and that the executor owns writes; `tools/call instance_get` and `instance_history` return normal results against a data dir the executor is concurrently writing (opened from the same test process on separate handles).
- Read-only serve coexists with a writer: open serve `--read-only` and a writer `Store` in the same test → no `store/lock`.
- Non-read-only serve against a data dir the executor already holds the writer on → the second opener gets `store/lock` and renders it as the existing lock error (regression guard, proving single-writer still holds).
- Embedded mode: serve initialized with a machine whose instance emits an effect; after driving the input lines that advance into the effect, the serve process itself journals the `effect_acked` using the fixture stub handler — no external executor involved.
- Mode visibility: each of the three startups logs its mode line; the proof fixture strings are present.
- `store/lock` contention inside `fsm execute` (another writer holds it at tick time) is logged as `exec/store` and retried on a later tick, not a fatal exit.

- **Done when:** `cargo test -p fsm-cli --test serve_modes` passes the read-only, embedded, exclusive, contention, and mode-visibility rows, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
