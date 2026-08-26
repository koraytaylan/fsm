---
id: capability-negotiation
title: "Capability Negotiation And Routing"
workstream: "0057"
kind: task
depends_on:
  - output-multiplexer
gated: false
touches:
  - crates/fsm-cli/src/mcp/serve.rs
  - crates/fsm-cli/src/mcp/tools/dispatch.rs
  - crates/fsm-cli/tests/mcp_lifecycle.rs
  - crates/fsm-cli/tests/mcp_skeleton.rs
  - crates/fsm-cli/src/mcp/tools/dispatch.rs
  - crates/fsm-cli/src/mcp/tools/mod.rs
  - crates/fsm-cli/src/mcp/subscribe.rs
  - crates/fsm-cli/src/mcp/logging.rs
  - crates/fsm-cli/src/mcp/cancel.rs
  - crates/fsm-cli/tests/mcp_lifecycle.rs
status: done
merged_as: ""
---
# Capability Negotiation And Routing

This is the one task in the plan allowed to move the `initialize` golden, and it moves it once — so every later task can treat that transcript as fixed. It is also the **only** task in the plan that edits `serve.rs`'s method routing, which is what keeps five later tasks from queueing behind one file.

**Steps:**

1. In `crates/fsm-cli/src/mcp/serve.rs`, rewrite `initialize_result`'s capabilities to `{"tools": {"listChanged": false}, "resources": {"subscribe": true, "listChanged": true}, "prompts": {"listChanged": false}, "logging": {}}`.
2. Leave `tools.listChanged` and `prompts.listChanged` at `false` and record why in a comment: the tool and prompt sets are static, a per-machine tool surface would make `tools/list` depend on store contents, and no client is obliged to re-read either list.
3. Route every response in `handle_request` through the `Notifier` from `5701` instead of writing to `output` directly, so there is exactly one writer before any second producer exists.
4. **Wire every method arm this plan adds, here and only here.** `5701` already landed the module shells, so route `resources/subscribe` and `resources/unsubscribe` to `subscribe.rs`, `logging/setLevel` to `logging.rs`, and the `notifications/cancelled` arm to `cancel.rs`'s registry — replacing the stderr line that discards it today. The bodies are `unimplemented!()` until `5901`, `6001`, and `6003` fill them; the **routing** is complete after this task, and no later task in the plan edits `handle_request`'s match. Concentrating the arms here is deliberate: five tasks each adding one arm would serialise the whole plan behind one file for no benefit.
5. **Introduce the tool-call context seam.** `dispatch(store, clock, name, args)` cannot see the request's `params`, so it cannot reach `_meta.progressToken` and cannot consult a cancellation flag. Change its signature to take a `ToolCtx<'_>` carrying the `Notifier`, the request id, and the request's `_meta`, and thread it from `serve.rs`'s `tools/call` arm. Build the struct and pass it through unused at this stage — `6002` and `6003` are the consumers, and neither should have to reshape a signature to do its own job.
6. Do **not** touch the `instructions` string. `6103` owns the sentence describing the live surface, so this plan moves that transcript exactly once rather than twice.
7. Update the byte-compared `initialize` golden in `crates/fsm-cli/tests/mcp_lifecycle.rs` and any capability assertion in `crates/fsm-cli/tests/mcp_skeleton.rs`, in this commit — regenerating the skeleton with `REGEN_SKELETON=1 cargo test -p fsm-cli --test mcp_skeleton` and then **reading the diff line by line**, exactly as `docs/RELEASE.md` prescribes for a version stamp. A later task that finds itself editing a transcript golden it does not own has made a mistake.
8. Confirm version negotiation is untouched: `KNOWN_VERSIONS` still offers `2025-06-18`, `2025-03-26`, and `2024-11-05`, and every capability this plan adds exists in all three, so nothing here is version-gated. Verify that against the specification rather than trusting the architecture document.

**Tests:**

- `crates/fsm-cli/tests/mcp_lifecycle.rs`: the `initialize` result byte-matches the updated golden, with `resources.subscribe` and `resources.listChanged` true and a `logging` object present.
- Negotiation still returns the client's version when it is one of the three known ones and the default otherwise.
- A request before `initialize` still returns `NOT_INITIALIZED`, unchanged.
- `ping` still returns an empty result object, unchanged.
- `notifications/initialized` is still accepted and still produces no response.
- Batch requests are still refused with the existing message.
- Every response in the session is written through the `Notifier`, verified by asserting there is no remaining direct write to `output` in `serve.rs`.
- Every routed arm exists: `resources/subscribe`, `resources/unsubscribe`, and `logging/setLevel` reach their modules rather than returning `METHOD_NOT_FOUND`, and `notifications/cancelled` reaches the registry rather than stderr.
- `handle_request`'s match contains every arm this plan needs — assert by listing the plan's methods and confirming none returns `METHOD_NOT_FOUND`.
- `dispatch` takes a `ToolCtx` carrying the notifier, request id, and `_meta`, and the `tools/call` arm builds one; every existing tool behaves identically through the new signature.
- A session that uses none of the new capabilities produces a byte-identical transcript to the pre-plan build apart from the `initialize` line itself — assert this against a committed fixture, since it is the plan's inertness claim.

- **Done when:** `cargo test -p fsm-cli --test mcp_lifecycle --test mcp_skeleton` passes with the updated `initialize` golden, every method arm this plan adds is routed here, `dispatch` takes a `ToolCtx`, all responses flow through the `Notifier`, the instructions string is unchanged, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.

**Landed:** the four capabilities with their reasons, the four routed arms with a `Live` session state carrying the watch set, the level, and the cancellation registry, `ToolCtx` and `dispatch_with`, the regenerated transcripts, and tests that every plan method is routed, that a missing URI and an unknown level are refused by name, and that a cancellation answers nothing.

**Corrections.** (1) The routed arms call registries that *work*, not `unimplemented!()` bodies: an arm routed at a panic is a landmine rather than routing, and the test the task asks for — "reaches its module rather than returning `METHOD_NOT_FOUND`" — would have had to catch a panic to pass. The registries are small data structures; what each later task adds is the notification it produces, which is the part that needs its own task. (2) `dispatch` keeps its signature and gains `dispatch_with`: the plain form has around a hundred call sites in the CLI and the suites, none of which has anything to say about progress or cancellation, and rewriting them all to pass an empty context would be churn with no reader.
