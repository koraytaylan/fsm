---
id: instance-resources
title: "Instance Resources"
workstream: "0058"
kind: task
depends_on: []
gated: false
touches:
  - crates/fsm-cli/src/mcp/resources.rs
  - crates/fsm-cli/tests/mcp_resources.rs
  - crates/fsm-store/src/store/view.rs
  - crates/fsm-cli/src/mcp/tools/handlers/instance.rs
  - crates/fsm-cli/tests/mcp_resources.rs
status: done
merged_as: ""
---
# Instance Resources

The live objects — the running workflows this whole system is about — have no URI, so there is nothing to subscribe to, nothing to link to from a tool result, and nothing a user can attach to a conversation. Two URIs fix that.

**Steps:**

1. In `crates/fsm-cli/src/mcp/resources.rs`, serve `fsm://instance/{id}` returning the `instance_view` structured object as `application/json`, and `fsm://instance/{id}/history` returning the first history page as `application/json`.
2. Add both to `resources/templates/list` beside the existing `fsm://machine/{id}` template, each with `uriTemplate`, `name`, `title`, and `mimeType`.
3. Extend `resources/list` to include the most recent instances up to the existing cap of 50, ordered most-recent-first by the `created_seq` plan 0010's `5103` added to the folded instance state. Do **not** order by scanning for an `instance_created` record: a child instance has none — `4901` derives it from `instance_invoked` — so that scan would silently omit every child, and it would add a second per-entry record walk to a listing that already pays one.
4. Serve the history URI as a **page**, using the same default limit `instance_history` uses, and say so in the resource description with a pointer to the tool for paging. A resource that could return an unbounded journal will one day return an unbounded journal.
5. Return the existing `-32002` "Resource not found" for an unknown instance id, a malformed URI, a trailing path this task does not serve, and a read attempted with no store — matching what the current code already does for unknown URIs rather than adding a second error shape.
6. Keep the two documentation resources and the machine resource byte-identical, so an existing client's `resources/list` parse and the committed goldens for them do not move beyond the added entries.
7. Make every instance read go through the same `instance_view` the tool uses, so a resource and a tool can never disagree about what an instance looks like.

**Tests:**

- `crates/fsm-cli/tests/mcp_resources.rs`: reading `fsm://instance/{id}` returns the same JSON body as the `instance_get` tool's `structuredContent` for that instance — assert equality directly, since divergence between the two surfaces is the failure this step prevents.
- Reading `fsm://instance/{id}/history` returns a bounded page matching `instance_history`'s default-limit result.
- `resources/templates/list` includes both new templates with their mime types.
- `resources/list` includes recent instances, ordered most-recent-first by `created_seq`, capped at 50 with 60 instances present.
- **A child instance appears in the listing** and is ordered correctly among roots — the case an `instance_created` scan would have dropped.
- Unknown instance id, malformed URI, unserved trailing path, and no-store each return `-32002`.
- The documentation and machine resources are byte-identical to their committed goldens.
- A store with no instances lists only the documentation and machine resources.
- Reading an instance that was cancelled or completed works and reflects the settled state.

- **Done when:** `cargo test -p fsm-cli --test mcp_resources` passes every case above including the resource/tool equality assertion, existing resource goldens are unchanged, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.

**Landed:** both instance URIs, both templates with titles and descriptions, the instance listing ordered by `created_seq` and capped at fifty, and the suite — resource/tool equality, the bounded history page, the templates, the ordering with sixty instances present, a child appearing in the listing and reading correctly, five not-found shapes, the untouched documentation and machine resources, an empty store, and a settled instance.

**Corrections.** Step 7 says every instance read goes through the same `instance_view` the tool uses, but the tool adds the instance's history bindings on top of that view — so a resource calling `instance_view` alone would differ from the tool's structured content, which the task's own test forbids. Both now call `Store::instance_report`, which is the view plus that addition: one function, two callers, and no way for the two surfaces to drift as the view grows.
