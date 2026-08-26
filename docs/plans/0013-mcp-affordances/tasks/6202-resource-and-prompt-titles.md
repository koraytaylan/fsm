---
id: resource-and-prompt-titles
title: "Resource And Prompt Titles"
workstream: "0062"
kind: task
depends_on: []
gated: false
touches:
  - crates/fsm-cli/src/mcp/resources.rs
  - crates/fsm-cli/src/mcp/prompts.rs
  - crates/fsm-cli/tests/mcp_resources.rs
  - crates/fsm-cli/tests/mcp_prompts.rs
status: planned
merged_as: ""
---
# Resource And Prompt Titles

`name` is the identifier and `title` is what a person reads; the two have been the same string everywhere, which reads badly in a client and loses information that costs nothing to add.

**Steps:**

1. In `crates/fsm-cli/src/mcp/resources.rs`, add `title` to every entry `resources/list` returns: the documentation resources get human titles, a machine resource's `title` is the machine's `name` while its `name` stays the identifier, and an instance resource's `title` names its machine and current state so a listing is readable at a glance.
2. Add `title` to every entry in `resources/templates/list`, describing what the template addresses rather than restating the URI.
3. In `crates/fsm-cli/src/mcp/prompts.rs`, add `title` to the prompt in `prompts/list` and to each of its arguments, so a client rendering a form shows readable labels.
4. Keep every `name` byte-identical. `name` is an identifier that clients and goldens key on; this task adds a field and changes none.
5. Keep an instance resource's `title` derived from data the listing already loads — the machine name and the configuration it already reads — rather than triggering an extra view render per entry. A listing that costs an `enabled_events` scan per instance is a listing that gets slow exactly when a store gets interesting.
6. Update the `resources/list`, `resources/templates/list`, and `prompts/list` goldens in this commit. With `6201`, these are the only golden moves in the plan.

**Tests:**

- `crates/fsm-cli/tests/mcp_resources.rs`: every entry in `resources/list` and `resources/templates/list` carries a non-empty `title`, and every `name` is unchanged from its committed value.
- A machine resource's `title` is the machine's `name` and its `name` is the identifier — assert they differ for a machine whose name is not its hash.
- An instance resource's `title` names its machine and current state.
- `crates/fsm-cli/tests/mcp_prompts.rs`: the prompt and each argument carry a `title`, and the prompt's `name` is unchanged.
- Listing 60 instances performs no per-instance view render — assert via a counter or by measuring that the call does not scale with an `enabled_events` cost.
- All three list goldens byte-match their updated fixtures.
- `resources/read` results are unchanged: this task touches listings only.

- **Done when:** `cargo test -p fsm-cli --test mcp_resources --test mcp_prompts` passes with titles present everywhere and every `name` unchanged, listings do no extra per-entry render, the three goldens are updated, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
