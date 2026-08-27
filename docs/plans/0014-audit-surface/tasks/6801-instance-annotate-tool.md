---
id: instance-annotate-tool
title: "Instance Annotate Tool"
workstream: "0068"
kind: task
depends_on:
  - store-doctor-tool
gated: false
touches:
  - crates/fsm-cli/tests/tool_schemas.rs
  - crates/fsm-cli/tests/mcp_full.rs
  - crates/fsm-cli/tests/mcp_regions_deadlines.rs
  - crates/fsm-cli/tests/naive_caller/core_tests.rs
  - crates/fsm-cli/tests/review_regressions/output_schema_and_wire_format.rs
  - crates/fsm-cli/tests/mcp_affordance_golden.rs
  - crates/fsm-cli/tests/degraded_gating.rs
  - crates/fsm-cli/tests/serve_modes.rs
  - crates/fsm-cli/tests/fixtures/
  - docs/EMBEDDING.md
  - crates/fsm-cli/src/mcp/tools/handlers/instance.rs
  - crates/fsm-cli/src/mcp/tools/mod.rs
  - crates/fsm-cli/src/mcp/tools/schema_in.rs
  - crates/fsm-cli/src/mcp/tools/schema_out.rs
  - crates/fsm-cli/src/mcp/descriptions.rs
  - crates/fsm-cli/tests/tools_budget.rs
  - crates/fsm-cli/tests/audit_annotate.rs
status: done
merged_as: ""
---
# Instance Annotate Tool

The `annotated` record kind is in SPEC and in the CLI, and nothing in the MCP surface can write one — so a model producing an audit trail cannot leave a note in it saying why it did what it did.

**Steps:**

1. Add `instance_annotate(instance_id, note, request_id)` to the registry, wrapping the existing `Store::annotate`, with schemas and a description alongside.
2. **Add it to `MUTATING_TOOLS`.** It writes a record, so a read-only server must refuse it and plan 0013's derived annotations then give it `readOnlyHint: false`, `destructiveHint: false`, and `idempotentHint: true` with no special case.
3. Do not add a second size rule. The note is bounded by `MAX_PAYLOAD_BYTES` like every other journaled payload, and an oversized note is `req/payload_too_large` — unjournaled, key not consumed, correct-and-resend. That behaviour already exists in the store and must simply surface.
4. State in the description that an annotation **changes no logical state**: it claims a `request_id`, it appears in `instance_history`, and it moves nothing. "Annotate" reads like it might do more, and a caller that believes it advances a workflow will be confused at the worst possible moment.
5. Return the instance view unchanged alongside the record's seq, so a caller can confirm the note landed without a second call.
6. Confirm annotating a completed or cancelled instance is legal, since a note about why something ended is exactly the note somebody wants to leave. Pin it — a reader may otherwise assume the lifecycle gate applies here as it does to `send`.
7. **Assert this plan's five tools fit under plan 0013's ceiling — do not raise it.** `6201` set one `tools/list` ceiling for the whole sequence with headroom sized for exactly these five plus `instance_elicit`. This task is last in the plan's tool sequence, so it is where the headroom is finally spent: measure with all five present and confirm `crates/fsm-cli/tests/tools_budget.rs` passes. If it does not, shorten descriptions rather than moving the number, and say in the commit message which ones and why.

**Tests:**

- `crates/fsm-cli/tests/audit_annotate.rs`: annotating writes one `annotated` record and returns the instance view with an unchanged configuration, context, and status.
- The note appears in `instance_history` at the returned seq.
- Idempotency: the same `request_id` replays with `duplicate: true`; the same key with a different note is refused, not replayed.
- An oversized note is `req/payload_too_large`, journals nothing, and leaves the key unconsumed — a corrected retry under the same key then succeeds.
- Annotating a completed instance and a cancelled instance both succeed.
- Annotating an unknown instance is a structured tool error.
- The tool is in `MUTATING_TOOLS`, refused by a read-only server, and refused in degraded mode.
- Its derived annotations report `readOnlyHint: false` and `idempotentHint: true`.
- CLI/MCP parity: `fsm instance annotate --json` matches the tool's `structuredContent`.
- Its structured output validates against its declared output schema.

- **Done when:** `cargo test -p fsm-cli --test audit_annotate --test tool_schemas --test read_only` passes every case above, an annotation changes no logical state, size limiting reuses the existing store rule, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.

**Landed:** `instance_annotate(instance_id, note, request_id)` wraps `Store::annotate` and returns the note, the seq it landed at, whether it was a replay, and the instance view **unchanged** — so one call confirms both that the note landed and that nothing moved. The description says so too, because "annotate" reads like it might do more and a caller who believed it advanced a workflow would find out at the worst possible moment. Annotating a completed or a cancelled instance is legal and pinned: a note about why something ended is exactly the note somebody wants to leave.

No second size rule: an oversized note is `req/payload_too_large` from the store's own check, journals nothing, and leaves the key unconsumed — asserted by correcting and resending under the same key.

**The ceiling held without moving. Twenty-four tools measure 36 256 bytes against 38 000.** 6201 projected 37 832 for exactly these six; the six landed 1 576 bytes under that projection, and 1 744 bytes of headroom remain.

**Corrections.**

- *`Store::annotate` returns neither a seq nor a duplicate flag, so the tool derives both.* The seq comes from the idempotency slot, which holds the original record's seq on a replay — the record a caller would actually go and read. "Duplicate" is `journal.last_seq` unchanged across the call, which is what a replay *is*.
- *Eight suites and two fixtures count the tools or enumerate arguments for them.* Every one is a gate noticing a new tool, which is what they are for.
