---
id: instance-tree-legibility
title: "Instance Tree Legibility"
workstream: "0051"
kind: task
depends_on:
  - cli-and-mcp-composition-tools
gated: false
touches:
  - crates/fsm-store/src/store/view.rs
  - crates/fsm-core/src/diagram.rs
  - crates/fsm-core/src/analyze.rs
  - crates/fsm-cli/src/mcp/tools/handlers/instance.rs
  - crates/fsm-cli/src/mcp/tools/schema_out.rs
  - crates/fsm-cli/tests/instance_tree.rs
  - crates/fsm-store/src/store/lifecycle.rs
  - crates/fsm-store/src/store/reconstruct.rs
  - crates/fsm-store/src/store/instance/invoke.rs
  - crates/fsm-store/src/store/instance/cancel.rs
  - crates/fsm-core/src/analyze/invoke.rs
  - crates/fsm-cli/tests/instance_tree.rs
  - docs/SPEC.md
status: done
merged_as: ""
---
# Instance Tree Legibility

A store where every listing is flat is a store nobody can navigate: once instances have parents, the surface has to show the edges, or composition is a feature you can only see in the journal.

**Steps:**

1. In `crates/fsm-store/src/store/view.rs`, add to `instance_view`: `parent: {instance_id, slot} | null` and `children: [{slot, child_instance_id, child_machine_id, status}]`, both derived from the folded state rather than by scanning records at read time.
2. **Add `created_seq` to the folded instance state and expose it on the view.** A child has **no** `instance_created` record — `4901` derives it from `instance_invoked` — so "when did this instance appear" has no answer a record scan can give uniformly, and every ordering built on `instance_created` would silently omit children. Record the seq of whichever record brought the instance into existence, `instance_created` or `instance_invoked`, at fold time. This also removes a per-entry record scan from the listing path, which the existing machine listing pays and which would otherwise double.
3. Add a `parent` filter and a `roots_only` boolean to the instance listing, so a caller can ask for one tree or for every root. Keep the existing cursor pagination contract exactly — filters compose with the cursor rather than replacing it.
4. In `crates/fsm-cli/src/mcp/tools/handlers/instance.rs` and `schema_out.rs`, surface both fields on `instance_get` and both filters on `instance_list`. The fields are additive, so an existing caller's parse is unaffected.
5. In `crates/fsm-core/src/diagram.rs`, draw an invoke slot as a labelled sub-box on its invoking state — Mermaid a nested `subgraph`-style node, DOT a `shape=box3d` child node — labelled with the slot id and the first eight hex of the child machine id, so a reader can tell two slots apart without opening the definition.
6. In `crates/fsm-core/src/analyze.rs`, report two composition smells: a slot whose `$done.invoke.<slot>` no transition handles, and a slot on a state with no other exit, which will wait forever if the child never settles. Both are **warnings** — they are modelling choices, not errors — reported through the existing `Finding` vocabulary.
7. Keep every non-composing machine's diagram and analysis output byte-identical, on the same additive-and-optional discipline the rest of this plan uses.

**Tests:**

- `crates/fsm-cli/tests/instance_tree.rs`: a child's `instance_get` reports its parent and slot; the parent's reports its children with their statuses.
- A root instance reports `parent: null` and an empty `children` array.
- `created_seq` is present on every instance view, is the seq of the `instance_created` record for a root and of the `instance_invoked` record for a child, and is stable across a re-fold.
- Ordering instances by `created_seq` includes children — assert a listing over a parent and its child returns both.
- `instance_list --parent <id>` returns exactly that instance's children; `roots_only` excludes every child; both compose correctly with a cursor across two pages.
- A depth-3 tree is navigable from the root to every leaf using only `instance_get`.
- Diagram: an invoking state renders its slot label with the truncated child machine id in both formats, and two slots on one state are visually distinct.
- Analysis: a slot with no handling transition reports the unhandled-result warning; a slot on a state with no other exit reports the wait-forever warning; a well-formed slot reports neither.
- Non-composing machines produce byte-identical diagram and analysis output to the committed goldens.
- `instance_get`'s structured output validates against its declared output schema.

- **Done when:** `cargo test -p fsm-cli --test instance_tree --test tool_schemas` passes every case above, a depth-3 tree is fully navigable from the tool surface, non-composing goldens are unchanged, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.

**Landed:** `parent`, `children`, and `created_seq` on the instance view and on every reconstructed one (a replayed response must be the response, so `view_at` grew the same three fields and takes the creation seq from the caller that holds the index); `parent` and `roots_only` filters applied before the cursor so they compose with pagination; the slot rendering in both diagram formats; `invoke_findings` with its two warnings, wired into `analyze_all` and into the analyze half of the every-code gate; and the `parents` index beside `history`, built from the complete record vector at open and extended by `invoke_child` so a live store agrees with what its own reopen would say.

**Corrections.** (1) Step 2 says to put `created_seq` on the folded instance state. `InstanceState` is the *hashed* state, so a field there either moves the `fsm.state/3` payload — a format bump this plan already spent, and one this task must not spend again — or sits inside a hashed struct unhashed, which is a trap for the next reader. `StoreState` would need the snapshot to carry it, bumping that format instead. The history index already holds exactly this fact at its first entry, for roots and children alike, and reading it is the O(1) lookup the step asks for. (2) Step 5 says Mermaid should draw a slot as a nested subgraph. `stateDiagram-v2` has no subgraph, and a composite state means something else in this renderer; the annotation form it already uses for `<<final>>` and `<<deep-history>>` says the same thing in the vocabulary a reader knows. DOT does get the `shape=box3d` child node the step names. (3) The two analysis codes are warnings, so they belong in the every-code gate's analyze table rather than its refusal table — `machine_create` accepts these machines, which is the point of a warning.
