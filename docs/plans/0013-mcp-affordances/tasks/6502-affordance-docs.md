---
id: affordance-docs
title: "Affordance Docs"
workstream: "0065"
kind: task
depends_on:
  - affordance-goldens
gated: false
touches:
  - crates/fsm-cli/tests/spec_appendix.rs
  - docs/EMBEDDING.md
  - README.md
  - crates/fsm-cli/tests/affordance_doc.rs
status: done
merged_as: ""
---
# Affordance Docs

`idempotentHint: true` is an unusually strong claim and a host operator deciding what to auto-approve deserves to know it is exact rather than aspirational — which is the kind of thing that belongs in prose, not only in a hint.

**Steps:**

1. Add an *Affordances* section to `docs/EMBEDDING.md` documenting what each annotation claims and where it is derived from: `readOnlyHint` from `MUTATING_TOOLS`, `destructiveHint` true only for `instance_cancel`, `openWorldHint` false because no tool call reaches beyond the data directory, and `idempotentHint` true for every mutating tool.
2. Spell out the `idempotentHint` claim precisely, because it is stronger than most servers can make: every mutating tool requires a `request_id`, the store keys on `(request_id, request fingerprint)`, a retry with identical content replays, and a reused key with **different** content is refused rather than replayed. That last clause is the part a host operator must understand before auto-approving retries.
3. Document what is completable and what is deliberately not: resource template `{id}` variables and prompt arguments are, tool arguments are not — the protocol defines completion only for the first two, and a private fourth reference type would be a message no client sends.
4. Document the `event` completion's dependence on the resolved-argument context, and that without `instance_id` it returns empty by design rather than guessing.
5. Document the elicitation path with its three honest limits stated together: the client must advertise the capability, nesting is capped at one outstanding ask, and there is a 300-second timeout. Add the rule that a decline or a timeout journals nothing and consumes no `request_id`.
6. State the compatibility argument explicitly, because it is the reason this feature is allowed to exist here: elicitation carries a schema derived from typed declarations and returns structured data validated through the same path an external payload takes. **The server still never parses natural language**, and an elicitation returning prose for the server to interpret is out of scope permanently, not merely for now.
7. Add one guarantee row to `README.md`: *accurate tool annotations — read-only, destructive, and idempotent hints are derived from the code that enforces them, not declared alongside it*.
8. Create `crates/fsm-cli/tests/affordance_doc.rs` pinning the documentation to the code, in the style of the existing `executor_doc.rs`.

**Tests:**

- `crates/fsm-cli/tests/affordance_doc.rs`: every tool name in the registry appears in the EMBEDDING affordances section's annotation table.
- The set of tools EMBEDDING lists as read-only is exactly the complement of `MUTATING_TOOLS` — asserted against the constant, so the doc cannot drift from the gate.
- EMBEDDING names `instance_cancel` as the only destructive tool, asserted against the same derivation `6201` uses.
- A documentation test asserts EMBEDDING contains the "refused rather than replayed" clause of the idempotency claim.
- A documentation test asserts EMBEDDING contains the never-parses-natural-language statement in the elicitation section.
- A documentation test asserts EMBEDDING names all three elicitation limits — capability, nesting cap, timeout — and the 300-second default.
- `README.md` contains the new guarantee row.
- The banned-vocabulary scan in `crates/fsm-cli/tests/policy.rs` passes over the new prose.

- **Done when:** EMBEDDING documents every annotation's derivation, the exact idempotency claim, what is and is not completable, and the elicitation path with all three limits and the compatibility argument; README carries the guarantee row; `cargo test -p fsm-cli --test affordance_doc --test policy` passes with the doc pinned to `MUTATING_TOOLS`; and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.

**Landed:** An *Affordances* section covering all three: where each hint comes from and what it claims, what completes and what deliberately does not, and the elicitation path with its three limits stated in one place. The idempotency claim is spelled out to its last clause — a reused key with different content is **refused rather than replayed**, as `req/request_id_conflict` — because that is the sentence a host operator needs before auto-approving retries.

The prose is pinned to the code rather than trusted: `affordance_doc.rs` reads the two tool lists back out of the section and compares both against `MUTATING_TOOLS`, so the guide cannot drift from the gate in either direction; it checks that the destructive row names `instance_cancel` and no other tool; and it asserts the documented 300 seconds against `DEFAULT_TIMEOUT_MS`. README gains the annotation-accuracy row.

**Corrections.**

- *The guarantee row count moves again, from 22 to 23.* `spec_appendix` counts the README's rows, so adding one is a two-file change — the test doing its job, as in 6103.
