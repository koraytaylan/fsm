---
id: resource-template-completion
title: "Resource Template Completion"
workstream: "0063"
kind: task
depends_on:
  - completion-capability
  - resource-and-prompt-titles
gated: false
touches:
  - crates/fsm-store/src/store/lifecycle.rs
  - crates/fsm-store/src/store/mod.rs
  - crates/fsm-store/src/store/commit.rs
  - crates/fsm-cli/src/mcp/complete.rs
  - crates/fsm-cli/src/mcp/resources.rs
  - crates/fsm-cli/tests/mcp_completion_resources.rs
status: done
merged_as: ""
---
# Resource Template Completion

The server holds every machine id and every instance id; a caller assembling a URI should be offered them rather than reconstructing a hash from a previous response.

**Steps:**

1. In `crates/fsm-cli/src/mcp/complete.rs`, implement the `ref/resource` supplier for the three templates: `fsm://machine/{id}`, `fsm://instance/{id}`, and `fsm://instance/{id}/history`.
2. Complete `{id}` for the machine template from the machine catalogue, most-recent-first by the seq of the defining record — the same ordering `resources/list` already uses.
3. Complete `{id}` for both instance templates from the instance listing, most-recent-first by `created_seq` — the same field `5801`'s listing orders by. Never scan for an `instance_created` record: a child instance has none, and completing without children would hide exactly the instances composition creates.
4. In `crates/fsm-cli/src/mcp/resources.rs`, expose the ordered id enumeration the supplier needs as a small `pub(crate)` function rather than letting `complete.rs` re-derive an ordering. One ordering rule, one implementation, so a listing and its completions can never disagree.
5. Read ids through the folded state rather than by scanning records at completion time: a completion request is interactive and must not pay a journal walk.
6. Match on the raw id string only. Do **not** try to complete by machine *name* under an id argument — a name is not an id, and offering one would produce a URI that fails to read.
7. Return an empty completion for a template variable this task does not serve, per `6301`'s ruling, rather than an error.

**Tests:**

- `crates/fsm-cli/tests/mcp_completion_resources.rs`: completing `{id}` for `fsm://machine/{id}` with an empty prefix returns machine ids, most-recent-first.
- A prefix matching two of five machines returns exactly those two, with `total: 2`.
- Completing `{id}` for `fsm://instance/{id}` and for `fsm://instance/{id}/history` both return instance ids in most-recent-first order.
- A child instance is offered as a completion, ordered by `created_seq` among its peers.
- A prefix matching nothing returns an empty completion with `total: 0` and no error.
- With 250 instances, the response carries 100 values, `total: 250`, and `hasMore: true`.
- A completed value composes into a URI that `resources/read` then resolves successfully — assert the round trip, since a completion that yields an unreadable URI is worse than none.
- Machine **names** are never offered under an id argument.
- The completion's ordering matches `resources/list`'s ordering for the same store — assert against the listing directly.
- A completion request performs no journal walk — assert via a counter or by confirming the cost does not scale with journal length.

- **Done when:** `cargo test -p fsm-cli --test mcp_completion_resources` passes every case above including the completion-to-read round trip and the shared-ordering assertion, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.

**Landed:** `machine_ids` and `instance_ids` are `pub(crate)` in `resources.rs` and are what both the listing and the completion read, so an ordering rule has one implementation. Instances order by `created_seq`, which reads the folded history — a child's first record is its `instance_invoked`, so children are offered like any other instance, which a scan for `instance_created` would have hidden. Only `{id}` completes, and only as an id: a machine *name* under an id argument composes into a URI that fails to read.

The no-journal-walk claim is proved by taking the journal away — `records.clear()`, then the same completions come back identical. That is only true because machine ordering stopped being a record scan.

**Corrections.**

- *Ordering machines by age needed a store index, not a scan.* `resources/list` was finding each machine's defining record by walking `records` — per machine, on every call — which is exactly what step 5 forbids for completion. `Store::machine_seqs` now records when each definition first arrived, built on open beside `history` and `parents` and extended in `note_record`, so "newest first" is a map lookup. The listing gets the same improvement for free, and the store had the precedent: two derived indexes already lived there for the same reason.
- *The child test starts the invocation.* Entering an invoking state arms the slot; `invoke_child` is the record that brings the child into existence, and without it there is no child to offer.
