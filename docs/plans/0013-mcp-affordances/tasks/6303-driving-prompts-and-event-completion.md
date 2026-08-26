---
id: driving-prompts-and-event-completion
title: "Driving Prompts And Event Completion"
workstream: "0063"
kind: task
depends_on:
  - resource-template-completion
gated: false
touches:
  - crates/fsm-cli/src/mcp/prompts.rs
  - crates/fsm-cli/src/mcp/complete.rs
  - crates/fsm-cli/tests/mcp_completion_prompts.rs
status: planned
merged_as: ""
---
# Driving Prompts And Event Completion

This is where completion earns its place: an `event` argument completed from the named instance's own `enabled_events` offers exactly the events that can actually fire, at the moment somebody is deciding what to send.

**Steps:**

1. In `crates/fsm-cli/src/mcp/prompts.rs`, add two prompts beside the existing `author_machine`: `drive_instance` with a required `instance_id` and an optional `event`, and `diagnose_instance` with a required `instance_id`. Both follow the existing prompt's shape — a `messages` array with one user message — and both teach the workflow rather than restating the tool list.
2. `drive_instance`'s body should point at `instance_get`, `enabled_events`, `deadlines_pending`, and the subscription path plan 0012 added, so the prompt is a route through the surface rather than a paragraph.
3. `diagnose_instance`'s body should point at `instance_history --trace` and `explain`, which is the diagnosis path an operator actually takes.
4. In `crates/fsm-cli/src/mcp/complete.rs`, implement the `ref/prompt` supplier. `instance_id` on both prompts completes from the instance listing, reusing `6302`'s enumeration rather than a second one.
5. Complete `event` on `drive_instance` using the **resolved-argument context**: when `context.arguments.instance_id` is present, return that instance's `enabled_events`, filtered to genuinely enabled ones. Confirm the context field's exact shape against the specification while implementing.
6. Return an **empty** completion for `event` when the context argument is absent. Guessing from the whole store would suggest events that cannot fire against this instance, which is worse than offering nothing.
7. Exclude internal and `$`-prefixed generated events. After plan 0009 this falls out of `enabled_events` already excluding them; assert it anyway, so the two stay connected if either changes.
8. Complete nothing for `author_machine`'s `goal` argument — a free-text goal has no candidate set, and returning an empty completion is the honest answer.

**Tests:**

- `crates/fsm-cli/tests/mcp_completion_prompts.rs`: `prompts/list` includes all three prompts with their arguments and required flags.
- `prompts/get` for each of the two new prompts returns a `messages` array with the documented content.
- `instance_id` completes from the instance listing on both new prompts, most-recent-first.
- `event` with `context.arguments.instance_id` present returns exactly that instance's enabled events — assert against `instance_get`'s `enabled_events` for the same instance.
- `event` **without** the context argument returns an empty completion.
- `event` for an instance whose enabled set is empty returns an empty completion.
- An internal event and a `$`-prefixed generated event never appear in an `event` completion.
- `goal` on `author_machine` returns an empty completion.
- A prefix filter applies to `event` completions as it does elsewhere.
- The `prompts/list` golden byte-matches its updated fixture.

- **Done when:** `cargo test -p fsm-cli --test mcp_completion_prompts` passes every case above, `event` completion agrees with `instance_get`'s `enabled_events` and returns empty without context, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
