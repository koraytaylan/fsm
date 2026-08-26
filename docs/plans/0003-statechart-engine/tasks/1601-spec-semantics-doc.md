---
id: spec-semantics-doc
title: "Spec Semantics Doc"
workstream: "0016"
kind: chore
depends_on:
  - creation-entry-chain
gated: false
touches:
  - docs/SPEC.md
status: done
merged_as: ""
---
# Spec Semantics Doc

The normative §Semantics lands in `docs/SPEC.md`: the decision procedure, pipeline ordering, history rules, creation, and atomicity — the prose from which the ordering goldens of this plan derive, and against which any future disagreement is judged.

**Steps:**

1. Replace the `## Semantics` placeholder in `docs/SPEC.md` with the complete normative section: the decision procedure as numbered pseudocode (status gate → chain candidate scan → guard evaluation → internal/external and history-target resolution → block pipeline → history capture → invariants → status), the per-block snapshot rule with the staging idiom, entry/exit scope rules, the full history rule table (declaration, pre-transition capture, outside-only targeting, unbound default, restore re-runs entry blocks, bindings retained after completion), creation semantics with the unjournaled-failure rule, and the atomicity guarantee.
2. Add the `run/*` catalogue (code, trigger, hint policy), including the `run/unhandled` versus `run/not_enabled` distinction and `run/create_failed` as the one unjournaled `run/*` code.
3. Fill the `## Machine definitions` placeholder with the `fsm.machine/1` format reference (keys, types, structural rules with their `def/*` codes, size limits) and note that machine identity covers descriptions.

**Tests:**

- This is a documentation chore — no unit tests exist for prose; acceptance is the following checklist, each item falsifiable:
- The numbered decision procedure in §Semantics matches the architecture's seven-step procedure step for step (side-by-side review; a missing or reordered step is a blocker).
- The history rule table covers all seven rules (declaration shape, capture point and pre-transition source, outside-only targeting, unbound default, restore re-runs entry blocks, retention after completion/cancel, leaf-only terminal interaction) — checked off one by one against the architecture's history table.
- Catalogue completeness by mechanical grep, recorded in the commit message: every distinct `run/*` code string under `crates/fsm-core/src/{step,analyze}.rs` appears in the catalogue and vice versa; every `def/*` code under `crates/fsm-core/src/spec.rs` appears in the §Machine definitions table and vice versa (the automated coverage test lands in plan 0006).
- Two committed scenario goldens — the deep-history scenario and the internal-transition scenario — are hand-traced against the doc's procedure, and the trace summary is recorded in the commit message (this is the "goldens derive from prose" bar being exercised in reverse).
- The `run/unhandled` vs `run/not_enabled` distinction and the unjournaled `run/create_failed` rule each appear with their rationale sentences; the size-limit table matches `limits.rs` values number for number.

- **Done when:** `docs/SPEC.md` contains complete §Semantics and §Machine definitions sections covering the decision procedure, history rules, creation, atomicity, and the `def/*` and `run/*` catalogues, with the checklist above satisfied and its grep/trace evidence in the commit message, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
