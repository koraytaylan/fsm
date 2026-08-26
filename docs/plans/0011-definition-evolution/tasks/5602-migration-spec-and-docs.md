---
id: migration-spec-and-docs
title: "Migration Spec And Docs"
workstream: "0056"
kind: task
depends_on:
  - migration-properties-and-chaos
gated: false
touches:
  - docs/SPEC.md
  - docs/RELEASE.md
  - docs/EMBEDDING.md
  - docs/API-POLICY.md
  - README.md
  - crates/fsm-cli/tests/spec_appendix.rs
status: planned
merged_as: ""
---
# Migration Spec And Docs

Every rule in this plan is a refusal or a ruling, and a ruling that only exists in code is one a future reader will "fix" — so SPEC states all of them, including the two that will surprise people.

**Steps:**

1. Add a `## Evolution` section to `docs/SPEC.md` covering, normatively: the `supersedes` block; the **MUST** that it is part of the canonical definition and therefore of `machine_id`, with the reason; the one-block-per-definition rule and the two-hop chain that follows from it; and the eight-row admission table.
2. Document the seven-step migration order as a numbered normative list — gate, map, project, carry over, invariants, **react to quiescence**, return — with the refusal code each step can produce. State explicitly that a migrated instance runs its reaction phase like a freshly created one, and that `instance_migrated` therefore carries a `microsteps` array under the same absent-when-empty rule every other record uses.
3. Document all five carry-over rulings, and mark two of them as consequences an operator MUST know: **migration reschedules every deadline from the migration instant**, and a `Running` invocation slot with no counterpart refuses the whole migration. Both will otherwise be discovered in production.
4. Extend `### Record kinds` with `instance_migrated` and its exact body fields, and state the replay rule: an instance's records legitimately span two definitions, a fold tracks the current machine per instance and asserts the `from_machine_id` link, and a superseded machine is never removed from the catalogue.
5. Extend `## Appendix A — Error codes` with the fourteen codes `5301` registered. There are no new limits in this plan; say nothing in Appendix B rather than inventing a row.
6. Add the operator's runbook to `docs/EMBEDDING.md`: preview the cohort, read the grouped refusals, decide whether to widen the mapping or accept the exclusions, migrate in batches with `--limit`, and re-run after any interruption. Include the non-atomicity statement and the derived-key resumption rule.
7. In `docs/API-POLICY.md`, state what `supersedes` means for compatibility: adding it to a definition produces a **new** machine and never changes an existing one, so no published `machine_id` can ever change meaning — which is the property that makes migration safe to add at all.
8. Add to `README.md` one guarantee row — *explicit evolution: an instance changes definition because a record says so, under a mapping the new definition declares* — and two honest non-claims: migration restarts every timer, and a cohort migration is not atomic.
9. Add a **Manual acceptance** row to `docs/RELEASE.md`: preview and then migrate a live cohort against a store with instances in more than one state, and confirm the grouped refusal summary reads correctly to a person. A cohort preview is an operator-facing report and the pipeline cannot judge whether it is legible.
10. Extend `crates/fsm-cli/tests/spec_appendix.rs` to assert, in both directions, that every `req/migrate_*` and `def/supersedes_*` code in `ALL_CODES` appears in the appendix.

**Tests:**

- `cargo test -p fsm-cli --test spec_appendix` passes with all fourteen codes documented and checked in both directions.
- A documentation test asserts SPEC states the deadline-rescheduling consequence, so the surprise is pinned to prose.
- A documentation test asserts SPEC states that `supersedes` is part of `machine_id`.
- A documentation test asserts `docs/EMBEDDING.md` contains the non-atomicity statement and the `migrate-{instance_id}-{to_machine_id}` key form.
- Every new record kind in `record.rs` appears in SPEC's `### Record kinds` table, via the both-directions assertion plan 0010 added to the same test.
- The banned-vocabulary scan in `crates/fsm-cli/tests/policy.rs` passes over all new prose.
- `docs/RELEASE.md` names the cohort-migration acceptance pass.
- `cargo doc --workspace --no-deps` is warning-free under `RUSTDOCFLAGS=-D warnings`.

- **Done when:** SPEC is normative about `supersedes`, the seven-step order, all five carry-over rulings, the record kind, and the two-definition replay rule; EMBEDDING carries the runbook; API-POLICY states the compatibility consequence; README carries the guarantee and both non-claims; `cargo test -p fsm-cli --test spec_appendix --test policy` passes; and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
