---
id: elicit-event-tool
title: "Elicit Event Tool"
workstream: "0064"
kind: task
depends_on:
  - elicitation-schema-derivation
  - tool-annotations-and-titles
gated: false
touches:
  - crates/fsm-cli/src/mcp/tools/mod.rs
  - crates/fsm-cli/src/mcp/tools/schema_in.rs
  - crates/fsm-cli/src/mcp/tools/schema_out.rs
  - crates/fsm-cli/src/mcp/tools/handlers/instance.rs
  - crates/fsm-cli/src/mcp/descriptions.rs
  - crates/fsm-cli/tests/tools_budget.rs
  - crates/fsm-cli/tests/elicit_tool.rs
status: planned
merged_as: ""
---
# Elicit Event Tool

A workflow at a human gate can now ask — and the journal records the event that arrived, never the conversation that produced it, which is what keeps this compatible with the rule that the server never parses natural language.

**Steps:**

1. Add `instance_elicit(instance_id, event, request_id)` to the registry in `crates/fsm-cli/src/mcp/tools/mod.rs`, with schemas in `schema_in.rs` and `schema_out.rs`, a description in `descriptions.rs`, and the handler in `handlers/instance.rs`.
2. **Add it to `MUTATING_TOOLS`.** It can write, so a read-only server must refuse it with the mode-naming message; its derived annotations then follow from `6201` with no special case.
3. Detect the client's `elicitation` capability from the `initialize` parameters, stored once on the session. When it is absent, refuse with a tool error whose hint names `instance_send` as the direct path — a caller must never be left guessing why the ask did nothing.
4. Confirm the named event is currently **enabled** on the instance before asking, refusing with the ordinary `run/not_enabled` vocabulary otherwise. Asking a person to fill in a form for an event that cannot fire is worse than refusing.
5. Build the schema with `6402`'s derivation and perform the exchange through `6401`'s `request_and_await`.
6. On `action: "accept"`, coerce the content into a typed payload and send it through the **ordinary** `instance_send` path with the caller's `request_id`. There is no elicitation record and no new record kind: what happened to the workflow is that an event arrived.
7. On `action: "decline"` or `"cancel"`, send nothing, journal nothing, **consume no `request_id`**, and return a structured result naming the action so the caller can react rather than guess.
8. On a timeout, a nesting refusal, or a coercion failure, return a tool error naming the cause; in every one of those cases nothing is journaled and the `request_id` stays unclaimed and reusable.
9. **Assert this tool fits under `6201`'s ceiling — do not raise it.** `6201` set one ceiling for the whole sequence with headroom for the six tools still to come, of which this is the first. Measure and confirm `crates/fsm-cli/tests/tools_budget.rs` still passes. If it does not, the answer is to shorten this tool's description rather than to move the number, because a ceiling that only ever goes up is not a budget.

**Tests:**

- `crates/fsm-cli/tests/elicit_tool.rs`: with a client advertising `elicitation`, an accepted exchange sends the event and returns the post-send instance view.
- The journal after an accepted exchange contains exactly one `event_applied` and **no** new record kind.
- A declined exchange journals nothing and returns a structured result naming `decline`; the `request_id` is then reusable for a different call.
- A cancelled exchange behaves the same way.
- A client that did **not** advertise `elicitation` gets a tool error naming `instance_send`, and nothing is written.
- An event that is not currently enabled is refused before any elicitation request is written — assert no `elicitation/create` appears in the output.
- A timeout returns a tool error and leaves the `request_id` unclaimed.
- A response that fails coercion returns a tool error naming the field and journals nothing.
- The tool is in `MUTATING_TOOLS` and is refused by a read-only server.
- Its derived annotations show `readOnlyHint: false` and `idempotentHint: true`, falling out of `6201`'s derivation with no special case.
- Its structured output validates against its declared output schema via `tool_schemas.rs`.

- **Done when:** `cargo test -p fsm-cli --test elicit_tool --test tool_schemas --test read_only` passes every case above, an accepted exchange journals only the event, declines and failures journal nothing and consume no key, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
