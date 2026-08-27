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
  - crates/fsm-store/src/store/view.rs
  - crates/fsm-store/src/store/mod.rs
  - crates/fsm-cli/tests/fixtures/transcripts/
status: done
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

**Landed:** Every listed resource carries a `title`; every template already did. A machine is `name`d by its identifier and titled by the name somebody wrote. An instance is titled `machine — leaf`, from the two map lookups the listing already performs — a regional configuration has no single leaf, so it says `machine — regions` rather than naming one region and implying it is the whole story. The prompt and its `goal` argument are titled for a client rendering them as a form.

The no-extra-render claim is checked rather than asserted in prose: `fsm_store` counts rendered views, listing sixty instances renders none, and reading one instance renders exactly one — the control that keeps the counter honest.

**Corrections.**

- *Step 4 and step 1 disagree about a machine entry's `name`, and step 1 is right.* The `name` was the spec name, so "its `name` stays the identifier" required changing it to the `machine_id`; the test step 6 asks for — `name` and `title` differ — cannot pass otherwise. It is also the better answer since plan 0011: a superseded machine and its replacement share a spec name, so a listing keyed on that name is ambiguous exactly where an operator is looking. Every other `name` — both documentation resources, every instance, the prompt — is byte-identical.
- *Counting views needs one file more than `touches` names.* The counter lives beside the render it counts, in `fsm-store`, and is re-exported by the store facade. A test that measured time instead would be a flake generator, and one that read the source for a call would break on the first refactor.
- *Every test in `mcp_resources.rs` now takes one mutex.* The counter is per-process, and thirteen sibling tests rendering views beside the one counting them is a race, not a measurement.
- *Only the three `mcp_full` transcripts moved.* Their listings hold the two documentation resources, so each gained exactly one `title` per entry — verified field by field rather than by eye.
