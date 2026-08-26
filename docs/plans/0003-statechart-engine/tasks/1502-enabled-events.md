---
id: enabled-events
title: "Enabled Events"
workstream: "0015"
kind: task
depends_on:
  - apply-pipeline
  - shadowing-and-const-fold
gated: false
touches:
  - crates/fsm-core/src/analyze.rs
  - crates/fsm-core/tests/enabled_events.rs
status: done
merged_as: ""
---
# Enabled Events

The steering-wheel report for a live instance: for every declared event, walk the ancestor chain in conflict order with three-valued guard evaluation (context concrete, event fields unknown) and report enabled, disabled, depends-on-payload, or preempted — so a caller never has to guess what an instance can accept next.

**Steps:**

1. Author `crates/fsm-core/tests/enabled_events.rs` first, encoding exactly the inventory under **Tests**.
2. Implement `EventStatus { Enabled, Disabled, DependsOnPayload, Preempted, PreemptedMaybe }`, `EventReport` (per-candidate detail plus, for payload-dependent guards, the event field names read), and `enabled_events(m, t, st, budget)` in `crates/fsm-core/src/analyze.rs` per architecture: the per-event summary is the first non-preempted status down the chain; a definite inner winner preempts everything after it; an unknown inner candidate makes later candidates `PreemptedMaybe`.

**Tests:**

- `case_review` in `docs_review`, per-event summaries asserted exactly: `docs_ok` → `Enabled` (guardless child transition); `scored` → `Disabled` (no candidate anywhere on the chain — reported as `Disabled` with an empty candidate list, per the completeness rule below); the ancestor events `suspend`, `withdraw`, `note_added` → `Enabled` at chain level `in_review` (the per-candidate detail names the source state); `resume` → `Disabled`.
- `case_review` in `risk_review`: `scored` → `DependsOnPayload` with the reported field list exactly `["score"]` (the guard reads `evt.score`; the guardless second transition sits behind an Unknown → its candidate detail is `PreemptedMaybe`).
- Preemption orderings (hand-built machines): a guardless child transition before an ancestor's → the ancestor candidate `Preempted` and the summary `Enabled`; an Unknown child candidate before a definite ancestor one → the ancestor `PreemptedMaybe` and the summary `DependsOnPayload`; a definitely-false child before a definitely-true ancestor → summary `Enabled` at the ancestor level (false candidates never preempt).
- Conservative-Unknown rule: a candidate whose ctx-concrete guard subtree errors → that candidate reports as payload-dependent-Unknown, never a panic and never `Enabled` (consistent with the partial evaluator's documented rule).
- Report completeness: every declared event appears exactly once in the report, including events with no candidates anywhere (summary `Disabled` with an empty candidate list) — asserted by comparing the report's key set to the machine's declared events.

- **Done when:** the report table cases pass — both `case_review` states, all three preemption orderings, the conservative-Unknown rule, and report completeness — under `cargo test -p fsm-core --test enabled_events`, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
