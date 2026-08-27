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
  - crates/fsm-core/src/error.rs
  - docs/SPEC.md
  - docs/EMBEDDING.md
  - crates/fsm-cli/tests/naive_caller/one_step_elicit.rs
  - crates/fsm-cli/tests/naive_caller/one_step_every_non_infra_code.rs
  - crates/fsm-cli/tests/naive_caller/infra_support.rs
  - crates/fsm-cli/tests/naive_caller/tool_outcomes.rs
  - crates/fsm-cli/tests/fixtures/transcripts/skeleton.out.jsonl
  - crates/fsm-cli/src/mcp/tools/mod.rs
  - crates/fsm-cli/src/mcp/tools/schema_in.rs
  - crates/fsm-cli/src/mcp/tools/schema_out.rs
  - crates/fsm-cli/src/mcp/tools/handlers/instance.rs
  - crates/fsm-cli/src/mcp/descriptions.rs
  - crates/fsm-cli/tests/tools_budget.rs
  - crates/fsm-cli/tests/elicit_tool.rs
status: done
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

**Landed:** `instance_elicit(instance_id, event, request_id)` is the nineteenth tool and the first mutating one this plan adds. It refuses in order: no client able to answer, then an event that cannot fire — asking a person to fill in a form for an event the machine would reject is worse than refusing before the form is written. Then it derives the schema, asks through 6401's exchange, and on `accept` coerces the answer and sends it down the **ordinary** `instance_send` path with the caller's key. The journal shows one `event_applied` and no new record kind: what happened to the workflow is that an event arrived.

`decline` and `cancel` return a structured result naming the action, journal nothing, and leave the key unclaimed — asserted by reusing it immediately afterwards. So do a timeout, a nesting refusal and a coercion failure.

**The ceiling holds without moving.** Nineteen tools measure **30 188** bytes against 38 000. `instance_elicit` cost 2 556, more than the 1 700 projected — its description was shortened by 89 bytes rather than the number being raised — leaving **7 812** for plan 0014's five tools, or 1 562 each.

All four elicitation codes now reach a caller, so both every-code gates lost their allowlist entries and gained real driven outcomes and one-step rows: no client to ask, a client that never answers, an ask inside an ask, and an answer that is an error. Each recovery is the same call again or the direct path the hint names, because none of them claims a key.

**Corrections.**

- *The refusal a read-only server gives is `io/write`, not a code of this task's own.* Every mutating tool gets the same mode-naming sentence from one place; a second vocabulary for the same refusal would be worse than the shared one.
- *The tool needs a fifth registry shape.* `ToolSpec::run` cannot carry the session, so `instance_elicit` takes the branch `simulate` and `instance_history` already take: validated in `dispatch_with`, then run with the context. Its registry `run` is the CLI path and says there is nobody to ask.
- *The one-step rows moved to their own file.* Adding four rows put `one_step_every_non_infra_code.rs` over the thousand-line ceiling; the four that need a scripted client on the other end of the wire are a seam, so they live in `one_step_elicit.rs`.
- *Two suites serialise their id predictions.* Server request ids are monotonic per process, so a test that scripts an answer for the next one holds a turn while it learns the id and uses it — as does any test that consumes one by writing a question.
- *EMBEDDING's read-only list moved from ten tools to eleven.* A documentation test checks that every gated tool is named there, which is the test doing its job.
