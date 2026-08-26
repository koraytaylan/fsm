---
id: resource-links-in-tool-results
title: "Resource Links In Tool Results"
workstream: "0058"
kind: task
depends_on:
  - instance-resources
  - capability-negotiation
gated: false
touches:
  - crates/fsm-cli/src/mcp/tools/dispatch.rs
  - crates/fsm-cli/tests/mcp_full.rs
  - crates/fsm-cli/tests/mcp_structured_parity.rs
  - crates/fsm-cli/src/mcp/serve.rs
  - crates/fsm-cli/src/mcp/tools/mod.rs
  - crates/fsm-cli/tests/mcp_full.rs
status: done
merged_as: ""
---
# Resource Links In Tool Results

A model that creates a workflow should get a handle to it, not a string it has to reassemble into a URI — and the link is what makes the resource it can then subscribe to discoverable at the moment it becomes relevant.

**Steps:**

1. In `crates/fsm-cli/src/mcp/tools/dispatch.rs`, after a successful tool call that produces or touches exactly one instance, append a third content element to the result: `{"type": "resource_link", "uri": "fsm://instance/<id>", "name": "<id>", "mimeType": "application/json"}`.
2. Apply it to exactly these tools: `instance_create`, `instance_send`, `deadline_poll`, `effect_ack`, `instance_cancel`, and `instance_get`. Not to `instance_list` — a list result would carry N links and bury the text — and not to any machine or simulate tool. Name the set as a constant beside `MUTATING_TOOLS` so the membership is one list rather than six match arms.
3. Take the instance id from the tool's own structured result rather than from its arguments, so a tool that resolved or defaulted an id links to what it actually acted on.
4. **Do not touch `structuredContent`.** It is what `crates/fsm-cli/tests/mcp_structured_parity.rs` and `review_regressions/cli_mcp_parity.rs` compare against the CLI's `--json` output, and changing it would break parity for a cosmetic addition to `content`.
5. Do not append a link to an error result. `tool_error` keeps its exact current shape — a failed call has no instance worth linking to, and a link there would invite a read that returns `-32002`.
6. Update the byte-compared transcript goldens in `crates/fsm-cli/tests/mcp_full.rs` for the six affected tools, in this commit, via `REGEN_MCP_FULL=1 cargo test -p fsm-cli --test mcp_full` followed by a line-by-line read of the diff. Confirm the diff contains **only** added `resource_link` elements for those six tools and nothing else — that check is the whole value of regenerating rather than hand-editing. This is the second and last golden move in the plan, and both are owned by the tasks that cause them.

**Tests:**

- `crates/fsm-cli/tests/mcp_full.rs`: each of the six tools returns three content elements — text, then the structured object, then the resource link — matching the updated goldens.
- The link's URI is readable: a `resources/read` of the returned URI in the same session succeeds.
- `instance_list`, `machine_list`, `machine_get`, `machine_analyze`, `machine_diagram`, and `simulate` results are unchanged.
- An error result from any of the six carries no link and byte-matches its existing golden.
- `structuredContent` is byte-identical to the pre-change value for every one of the six.
- `crates/fsm-cli/tests/mcp_structured_parity.rs` and `review_regressions/cli_mcp_parity.rs` pass unchanged.
- The linked id equals the id in the structured result, including for a call where the id was resolved rather than supplied verbatim.
- `tools_budget.rs` still passes — the added element does not push any result past the response budget.

- **Done when:** `cargo test -p fsm-cli --test mcp_full --test mcp_structured_parity --test tools_budget` passes with updated goldens for exactly six tools, `structuredContent` and every error result are unchanged, CLI parity holds, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.

**Landed:** `LINKED_TOOLS`, the link appended in `tool_ok` from the result's own `instance_id`, the regenerated transcripts, and tests that the six tools link, that the link resolves through `resources/read` in the same session, that a listing and a failure carry none, and that the linked id is the one in the structured result rather than the one in the arguments.

**Corrections.** The task describes the result as three content elements — "text, then the structured object, then the resource link" — but the structured object has never been a content element here: it is the sibling `structuredContent` field, which step 4 forbids touching. A successful linked call therefore carries **two** content elements, text and the link, beside that field. Adding the structured object to `content` as well would change what every existing client parses, for no reader.
