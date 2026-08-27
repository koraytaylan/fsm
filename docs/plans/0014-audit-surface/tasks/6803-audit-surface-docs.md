---
id: audit-surface-docs
title: "Audit Surface Docs"
workstream: "0068"
kind: task
depends_on:
  - audit-surface-proof
gated: false
touches:
  - crates/fsm-cli/tests/spec_appendix.rs
  - docs/EMBEDDING.md
  - docs/SPEC.md
  - README.md
  - crates/fsm-cli/tests/audit_doc.rs
status: done
merged_as: ""
---
# Audit Surface Docs

The most important sentence in this documentation is the one explaining what the surface deliberately will **not** do, because an operator who does not understand why `repair` is absent will go looking for a way around it.

**Steps:**

1. Add an *Auditing a store* section to `docs/EMBEDDING.md` covering each of the five tools: what it proves, what it costs, and when to reach for it. Draw the distinction between `journal_verify` and `journal_replay` explicitly — bytes and chain versus re-executed semantics — since a reader will otherwise assume they are the same tool twice.
2. Document how to read a health, reproducing SPEC's recovery table's postures, and state that every `remedy` string the tools return is a verbatim command rather than a paraphrase.
3. State plainly that **`repair` is not exposed, and why**: it destroys data, its safety argument rests on a human reading the quarantined bytes first, and the tools therefore hand over the command instead of running it. Put the reasoning next to the statement — a bare refusal invites somebody to add the tool later without knowing what they are undoing.
4. Document degraded mode: what triggers it, which three tools remain, that `machine_create --dry-run` still works, that it is reported rather than selected, and that every other tool's refusal carries the health, blast radius, and remedy.
5. In `docs/SPEC.md §Recovery`, add a short cross-reference noting which MCP tools report each health. SPEC stays normative about the postures; this is a pointer, not a second source of truth, and it must not restate a posture in different words.
6. Add one guarantee row to `README.md`: *the audit posture is auditable — the same surface that claims a tamper-evident chain can check it*.
7. Create `crates/fsm-cli/tests/audit_doc.rs` pinning the docs to the code, in the style of the existing `executor_doc.rs`.

**Tests:**

- `crates/fsm-cli/tests/audit_doc.rs`: every audit tool name in the registry appears in the EMBEDDING auditing section.
- Every health name in the store's health enum appears in the documented table, asserted against the enum rather than a literal list.
- Every `remedy` string a tool can return appears verbatim in `docs/SPEC.md` — assert by generating each remedy and searching the document, so a paraphrase fails the build.
- A documentation test asserts EMBEDDING contains the statement that `repair` is not exposed, together with the word "quarantined", so the reasoning cannot be trimmed to a bare refusal.
- A documentation test asserts EMBEDDING names all three tools available in degraded mode and the `machine_create --dry-run` exception.
- `README.md` contains the new guarantee row.
- The banned-vocabulary scan in `crates/fsm-cli/tests/policy.rs` passes over the new prose.
- `cargo test -p fsm-cli --test spec_appendix` still passes — this task adds a cross-reference to SPEC and no new codes.

- **Done when:** EMBEDDING documents all five tools, the verify-versus-replay distinction, how to read a health, degraded mode, and the reasoned exclusion of `repair`; SPEC carries the cross-reference; README carries the guarantee row; `cargo test -p fsm-cli --test audit_doc --test policy --test spec_appendix` passes; and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.

**Landed:** An *Auditing a store* section: a table of what each of the five tools proves and what it costs, the verify-versus-replay distinction spelled out because a reader would otherwise assume they are the same tool twice, how to read a health with SPEC's postures reproduced, and degraded mode down to its authoring exception.

The sentence the task calls most important has its reasoning beside it: `repair` destroys data, its safety argument is that a person reads the quarantined bytes first, and that argument does not survive being automated — so the tools hand over the command and somebody with the authority to lose those bytes runs it. Adding a repair tool later would be undoing a decision, and the guide says so.

SPEC gains a pointer, not a second source of truth: the recovery table names which tools report it and says the postures there are normative. README gains the audit-posture row.

The prose is pinned: every audit tool is asserted present by registry name, every health by **enum variant** rather than by a list in the test, and the remedy is *generated* from a torn store and searched for in both documents — so a paraphrase in either fails the build.

**Corrections.**

- *The guarantee row count moves from 23 to 24*, as it did in 6103 and 6502. `spec_appendix` counts the README's rows, so adding one is a two-file change.
