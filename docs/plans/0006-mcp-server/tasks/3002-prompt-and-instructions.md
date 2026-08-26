---
id: prompt-and-instructions
title: "Prompt And Instructions"
workstream: "0030"
kind: task
depends_on:
  - negotiation-and-lifecycle
gated: false
touches:
  - crates/fsm-cli/src/mcp/prompts.rs
  - crates/fsm-cli/tests/mcp_prompts.rs
status: done
merged_as: ""
---
# Prompt And Instructions

The `author_machine` prompt and the `initialize.instructions` text are the onboarding surface: they teach the golden loop once, in prompt real estate most hosts inject directly, so every conversation starts already knowing the workflow.

**Steps:**

1. Author `crates/fsm-cli/tests/mcp_prompts.rs` first, encoding exactly the inventory under **Tests**.
2. Implement `prompts/list` and `prompts/get` in `crates/fsm-cli/src/mcp/prompts.rs` with the architecture's prompt template, interpolating `goal`.
3. Replace the empty `INSTRUCTIONS` const with the architecture's ~120-word text, verbatim.

**Tests:**

- `prompts/list`: exactly one prompt, `author_machine`, declaring the single required argument `goal`.
- `prompts/get` with `goal: "track a mediation case"`: one user-role message whose text contains the goal string and, in this order (ordered substring search), the five flow stages — read `fsm://docs/spec`; `machine_create` with `dry_run` until clean; `machine_create`; `simulate` a happy path and a rejection path; `instance_create` and drive with `instance_send`.
- `prompts/get` without `goal` → the args-invalid error naming `goal` in `details`.
- `prompts/get` for an unknown prompt name → the not-found error listing `author_machine` as the valid name.
- Instructions: the initialize result's `instructions` field is present, is at most 130 words, and contains each of the key phrases `enabled_events`, `dry_run`, `effect_ack`, `request_id`, and `JSON strings`.
- Capabilities coherence: the advertised prompts capability is `{listChanged: false}`.

- **Done when:** `cargo test -p fsm-cli --test mcp_prompts` passes all listed assertions, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
