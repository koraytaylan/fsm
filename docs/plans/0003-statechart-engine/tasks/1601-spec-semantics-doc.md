---
id: spec-semantics-doc
title: "Spec Semantics Doc"
workstream: "0016"
kind: chore
depends_on:
  - apply-pipeline
gated: false
touches:
  - docs/SPEC.md
status: planned
merged_as: ""
---
# Spec Semantics Doc

The normative §Semantics lands in `docs/SPEC.md`: the decision procedure, pipeline ordering, history rules, creation, and atomicity — the prose from which the ordering goldens of this plan derive, and against which any future disagreement is judged.

**Steps:**

1. Replace the `## Semantics` placeholder in `docs/SPEC.md` with the complete normative section: the decision procedure as numbered pseudocode (status gate → chain candidate scan → guard evaluation → internal/external and history-target resolution → block pipeline → history capture → invariants → status), the per-block snapshot rule with the staging idiom, entry/exit scope rules, the full history rule table (declaration, pre-transition capture, outside-only targeting, unbound default, restore re-runs entry blocks, bindings retained after completion), creation semantics with the unjournaled-failure rule, and the atomicity guarantee.
2. Add the `run/*` catalogue (code, trigger, hint policy), including the `run/unhandled` versus `run/not_enabled` distinction and `run/create_failed` as the one unjournaled `run/*` code.
3. Fill the `## Machine definitions` placeholder with the `fsm.machine/1` format reference (keys, types, structural rules with their `def/*` codes, size limits) and note that machine identity covers descriptions.
4. Cross-check against the committed scenario goldens; divergences are fixed in fixtures or implementation, never by silently editing the prose.

- **Done when:** `docs/SPEC.md` contains complete §Semantics and §Machine definitions sections covering the decision procedure, history rules, creation, atomicity, `def/*` and `run/*` catalogues, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
