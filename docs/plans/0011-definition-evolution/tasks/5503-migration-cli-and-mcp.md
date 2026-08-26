---
id: migration-cli-and-mcp
title: "Migration CLI And MCP"
workstream: "0055"
kind: task
depends_on:
  - instance-migrate-operation
gated: false
touches:
  - crates/fsm-cli/src/args.rs
  - crates/fsm-cli/src/cli/instance.rs
  - crates/fsm-cli/src/mcp/tools/mod.rs
  - crates/fsm-cli/src/mcp/tools/handlers/instance.rs
  - crates/fsm-cli/src/mcp/tools/schema_in.rs
  - crates/fsm-cli/src/mcp/tools/schema_out.rs
  - crates/fsm-cli/src/mcp/descriptions.rs
  - crates/fsm-cli/tests/migration_tools.rs
status: planned
merged_as: ""
---
# Migration CLI And MCP

The dry run is the important half of this surface: an operator or a model should be able to ask what a migration would do from a read-only server, and only then decide to write.

**Steps:**

1. Add `fsm instance migrate <id> --to <machine> [--dry-run] [--request-id <id>]` in `crates/fsm-cli/src/args.rs` and `crates/fsm-cli/src/cli/instance.rs`, following the existing mutating-subcommand shape.
2. Add the MCP tool `instance_migrate` in `crates/fsm-cli/src/mcp/tools/mod.rs`, with `instance_id`, `to_machine`, optional `dry_run`, and `request_id` in `schema_in.rs`, and an output schema in `schema_out.rs` carrying either the preview or the post-migration instance view.
3. **Add it to `MUTATING_TOOLS`** so a read-only server refuses the writing form — but make `dry_run: true` work on a read-only server, exactly as `machine_create`'s dry run does. Route the dry-run branch **before** the mutating-tool gate, and pin that ordering with a test, because it is the one place in the tool surface where a mutating tool has a legal read-only path.
4. Make `dry_run: true` require no `request_id`: a preview claims no idempotency key because it changes nothing. Requiring one would teach a caller to burn keys on questions.
5. Write the description in the existing voice: what it does, that the target must declare `supersedes` for this instance's machine, that timers are rescheduled from now, and that the caller should dry-run first. The rescheduling consequence belongs in the tool description, not only in SPEC — it is the surprise an operator will otherwise meet in production.
6. Surface the preview's grouped refusal codes in the structured output so a model can act on them without parsing prose.
7. Add `machine_history: [{machine_id, from_seq}]` to `instance_get`'s output and schema, so a reader sees that an instance has changed definitions without paging its journal.
8. Keep `--json` output byte-identical to the MCP structured result, which `review_regressions/cli_mcp_parity.rs` enforces workspace-wide.

**Tests:**

- `crates/fsm-cli/tests/migration_tools.rs`: `instance_migrate` performs the migration and returns the post-migration view; `dry_run: true` returns the preview and writes nothing.
- `dry_run: true` **succeeds** on a read-only server; the writing form is refused there with the mode-naming message.
- `dry_run: true` without a `request_id` succeeds; the writing form without one reports the existing argument error.
- The tool is present in `MUTATING_TOOLS`.
- A refusal — settled instance, unmapped leaf, or a non-superseding target — is returned as a structured tool error carrying the code, not as a transport error.
- `instance_get` reports `machine_history` with one entry before migration and two after, each with its `from_seq`.
- CLI/MCP parity: `fsm instance migrate --json` matches `instance_migrate`'s `structuredContent` for both the real and dry-run forms.
- Both output shapes validate against the declared output schema via `tool_schemas.rs`.
- `tools/list` stays under the response budget `tools_budget.rs` enforces, and the tool-count assertions in the MCP golden suites are updated in this commit.

- **Done when:** `cargo test -p fsm-cli --test migration_tools --test tool_schemas --test read_only` passes, the dry run works on a read-only server while the writing form is refused, `machine_history` is exposed, parity holds, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
