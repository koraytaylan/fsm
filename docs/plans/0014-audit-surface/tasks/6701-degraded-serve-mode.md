---
id: degraded-serve-mode
title: "Degraded Serve Mode"
workstream: "0067"
kind: task
depends_on: []
gated: false
touches:
  - crates/fsm-cli/src/mcp/serve.rs
  - crates/fsm-cli/tests/degraded_serve.rs
status: done
merged_as: ""
---
# Degraded Serve Mode

Today an unhealthy store makes the server vanish before the client ever connects, with one line on a stderr nobody is reading — at exactly the moment somebody needs to find out what is wrong.

**Steps:**

1. In `crates/fsm-cli/src/mcp/serve.rs`, introduce `enum StoreSlot { Open(Store), Degraded { health: JournalHealth, detail: String } }` and stop returning `Err` when the open fails. The session **starts**: `initialize` succeeds, capabilities are unchanged, `tools/list` is unchanged.
2. Keep the existing stderr line, and — after plan 0012 — emit the same message as a `notifications/message` at `error` level, so a client sees the problem rather than only a terminal.
3. Add a degraded note to `instructions` in the same style as the existing read-only and embedded notes: one sentence naming the state and pointing at `store_doctor`. That sentence is how a model discovers what to do next, and it must name the tool.
4. Make the mode **reported, never selected**. There is no `--degraded` flag. It is what happens when the store cannot be opened, which keeps the deployment surface unchanged and means no operator can accidentally run a server that refuses to work.
5. Preserve the existing behaviour for the three healthy modes exactly: `Writer`, `ReadOnly`, and `Embedded` are untouched, and `mode_name` gains a fourth arm only for reporting.
6. In degraded mode, spawn no executor even when `--execute` was requested: there is nothing to write to. Report that in the startup line rather than failing.
7. Keep `resources/read` of the two documentation resources working — they are `include_str!` constants and need no store — while instance and machine resources return `-32002`.

**Tests:**

- `crates/fsm-cli/tests/degraded_serve.rs`: a server pointed at a torn-tail store **starts**, completes `initialize`, and returns the unchanged tool list.
- `instructions` carries the degraded note naming `store_doctor`, and the three healthy modes' instructions are byte-identical to their existing goldens.
- An error-level `notifications/message` carries the health and detail.
- The startup stderr line still appears.
- `resources/read` of `fsm://docs/spec` and `fsm://docs/examples` succeeds; an instance resource returns `-32002`.
- `serve --execute` against a degraded store starts without an executor and says so in the startup line.
- A healthy store produces a byte-identical transcript to the pre-change build — assert against a committed fixture, since this task must be inert for every working deployment.
- A chain-broken store and a store whose `VERSION` is unreadable both degrade rather than exiting.
- The process exit code is 0 on a clean client disconnect from a degraded session — a degraded server that ran correctly did not fail.

- **Done when:** `cargo test -p fsm-cli --test degraded_serve` passes every case above, an unhealthy store starts a server instead of killing one, healthy transcripts are byte-identical, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.

**Landed:** A store that will not open no longer kills the server. The session starts, `initialize` succeeds with unchanged capabilities, `tools/list` is unchanged, and the documentation resources still read — they are constants in the binary. An instance resource is `-32002`, which is what "nobody can read this" already means here.

The client hears about it once, at `error` level, as soon as it is allowed to hear anything: the health, the detail, and the sentence "call store_doctor for the diagnosis". `instructions` gains a degraded note in the style of the read-only and embedded ones, naming `store_doctor` — that sentence is how a model finds out what to do next. The stderr line stays and gains `mode=degraded`, plus `executor=none` when `--execute` was asked for, since there is nothing to write to.

The mode is **reported, never selected**: there is no flag, because it is not a way to run a server, it is what happened to one. A healthy session is untouched — same instructions, no notifications, byte-identical transcript — which is the claim that matters for every working deployment, and it is asserted directly.

**Corrections.**

- *`StoreSlot` is not the shape that fits.* `serve_session_with` already takes `Option<&mut Store>`, so "no store" is already representable; what was missing was *why*. `serve_session_degraded` takes the detail string and `Live` carries it. An enum would have meant rewriting every store access in the loop to say the same thing the `Option` already says.
- *The health travels as text, not as a `JournalHealth`.* The open path already renders its refusal as an `ErrorObj` with a code and a message; re-deriving a health enum here would be a second classification beside the one `store_doctor` performs, and the two could disagree.
