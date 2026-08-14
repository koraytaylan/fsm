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
status: planned
merged_as: ""
---
# Prompt And Instructions

The `author_machine` prompt and the `initialize.instructions` text are the onboarding surface: they teach the golden loop once, in prompt real estate most hosts inject directly, so every conversation starts already knowing the workflow.

**Steps:**

1. Author `crates/fsm-cli/tests/mcp_prompts.rs` first: assert `prompts/list` returns exactly `author_machine` with its required `goal` argument; `prompts/get` with a sample goal returns one user message containing the goal and the five flow stages (read spec, dry-run until clean, create, simulate happy and rejection paths, create and drive an instance); and the initialize result's `instructions` field contains the key phrases `enabled_events`, `dry_run`, `effect_ack`, and `request_id`.
2. Implement `prompts/list` and `prompts/get` in `crates/fsm-cli/src/mcp/prompts.rs` with the architecture's prompt template, interpolating `goal`.
3. Replace the empty `INSTRUCTIONS` const with the architecture's ~120-word text, verbatim.

- **Done when:** `cargo test -p fsm-cli --test mcp_prompts` passes all three assertions, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
