---
id: live-model-acceptance-protocol
title: "Live Model Acceptance Protocol"
workstream: "0097"
kind: task
depends_on:
  - installed-executor-acceptance
gated: true
touches:
  - acceptance/manual/briefs.md
  - acceptance/manual/rubric.md
  - acceptance/manual/report-template.json
  - docs/RELEASE.md
status: planned
merged_as: ""
---
# Live Model Acceptance Protocol

**Gate:** a real supported model and interactive host are available, with an
operator able to review and retain the transcripts; authoring a rubric or
running a mock does not open this gate or complete the task.

**Steps:**

1. Preserve the existing uncoached case-review check and write two generic
   briefs for automated compensation and incompatible-handler correction;
   freeze the rubric before any scored sessions.
2. Define observable outcomes, the nine-session inventory, 40-call/15-minute
   per-session budgets, clean-session setup and the rule that coaching fails
   an attempt; retain all attempted sessions rather than selecting successes.
3. Run three fresh sessions per brief against the candidate, with only the
   published resources/docs and operator-provisioned table available to the
   model, recording model/host versions, tool calls and final external state.
4. Run the separate Desktop configuration/connect/list check already required
   by RELEASE; do not substitute it for live authoring or protocol coverage.
5. Have a human score the frozen rubric, retain redacted transcripts and
   artifact references, and rerun the complete set after relevant fixes.

**Tests:**

- Nine retained sessions each pass every rubric outcome within both budgets,
  including correct compensation, contract discovery and honest final status.
- A coached, over-budget, missing-transcript or unsafe session is classified
  failed/incomplete and cannot appear as a passed attempt in the evidence index.
- The host-specific connect/list check has its own retained result and version.

- **Done when:** a human-reviewed report and inspectable transcripts prove
  all nine uncoached sessions and the host-specific check passed on the
  candidate, with no omitted failed attempt or mock replacing a live session.
