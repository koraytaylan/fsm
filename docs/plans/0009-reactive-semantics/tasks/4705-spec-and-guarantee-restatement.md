---
id: spec-and-guarantee-restatement
title: "Spec And Guarantee Restatement"
workstream: "0047"
kind: task
depends_on:
  - macrostep-oracle-differential
  - simulate-and-enabled-events
  - reactive-inertness-proof
  - microstep-trace-and-explain
gated: false
touches:
  - docs/SPEC.md
  - docs/RELEASE.md
  - README.md
  - docs/EXAMPLES.md
  - examples/parallel_review_deadline.json
  - crates/fsm-cli/tests/spec_appendix.rs
status: planned
merged_as: ""
---
# Spec And Guarantee Restatement

SPEC is the source of truth and goldens derive from its prose, so this plan's semantics are not finished until the document says them — and the README's one-event guarantee has to be restated honestly rather than quietly left to rot.

**Steps:**

1. In `docs/SPEC.md` `## Semantics`, add a `### Macrosteps` subsection stating, normatively: the loop order (eventless selection, then the internal queue front, then quiescence); the three exceptions (invariants once at quiescence, `evt` bound only in the microstep whose trigger supplied it, one `now_ms` for the whole macrostep); atomicity across every microstep; the `MAX_MICROSTEPS` ceiling and `run/microstep_limit`; the discard rule for an unhandled internal event and the ruling that `on_unhandled` governs the trigger microstep only; and — stated as a MUST — that the internal queue is **never persisted** and is empty at every sealed state.
2. In `## Machine definitions`, document the optional `on` (absent = eventless), the `internal: true` event flag, the `raise` block key with its `with` typing rules, and `final: true` with its distinction from `terminal`. Add the new `def/*` rows to the structural-rules table and the `req/event_internal` row to the `### run/* catalogue` table with its trigger and hint policy.
3. In `### Record kinds`, document the optional `microsteps` array and state as a MUST that the key is **absent**, never empty, when a macrostep had no reaction microsteps — and that replay verifies both directions of that claim.
4. In `## Appendix B — Limits`, confirm `4201`'s `MAX_MICROSTEPS` row and add `MAX_RAISES_PER_BLOCK`, each noting it is deliberately **not** in the genesis `limits` block and why.
5. In `README.md`, replace the `one-event-one-transition` guarantee row with `one-event-one-macrostep` — "at most one transition fires for the event you sent; the machine may then react to itself to quiescence, bounded, in the same atomic record" — and add one sentence to the honest non-claims paragraph: reaction is bounded at 64 microsteps and a machine needing more is refused at run time, not truncated.
6. Add one worked example: extend `examples/parallel_review_deadline.json` (or add a sibling if that machine's identity is pinned by a golden) with a fork/join using `$done.region.*`, and document it in `docs/EXAMPLES.md` with the record it produces — one record, two microsteps. A feature nobody can copy from an example will not be used.
7. Add a **Manual acceptance** row to `docs/RELEASE.md`: drive a reactive machine — one eventless transition, one `raise`, one fork/join — through a live MCP host and confirm the cascade appears in `instance_history --trace`. The pipeline cannot check that a cascade is *legible to a person*, which is exactly what that section is for, and plan 0008 set the precedent by naming the executor there.
8. Confirm `crates/fsm-cli/tests/spec_appendix.rs` passes with every code from `4201`'s `ALL_CODES` addition present in the appendix, and extend that test to also assert every `def/*` code appears in the structural-rules table, not only the appendix.

**Tests:**

- `cargo test -p fsm-cli --test spec_appendix` passes and now covers the structural-rules table as well as Appendix A.
- `cargo test -p fsm-cli --test examples` replays the new fork/join example and asserts it settles in one record with two microsteps.
- A documentation test asserts `README.md` no longer contains the string `one-event-one-transition` and does contain `one-event-one-macrostep`, so the guarantee cannot silently revert.
- A documentation test asserts SPEC contains the words `never persisted` in the macrostep section, pinning the plan's most important structural rule to prose.
- Every SPEC code table row has a matching `ALL_CODES` entry and vice versa — assert both directions, so a documented-but-unimplemented code is caught too.
- The banned-vocabulary scan in `crates/fsm-cli/tests/policy.rs` passes over the new prose and the new example.
- `docs/RELEASE.md` names the reactive-machine manual-acceptance pass.

- **Done when:** SPEC describes macrosteps, the four new definition shapes, the optional record key and its absence rule, and every new code; README states `one-event-one-macrostep` and the bounded-reaction non-claim; a fork/join example replays in one record; `cargo test -p fsm-cli --test spec_appendix --test examples` passes; and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
