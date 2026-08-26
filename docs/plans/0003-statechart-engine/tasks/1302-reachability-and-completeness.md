---
id: reachability-and-completeness
title: "Reachability And Completeness"
workstream: "0013"
kind: task
depends_on:
  - expression-binding
  - tree-tables
gated: false
touches:
  - crates/fsm-core/src/analyze.rs
  - crates/fsm-core/tests/analyze_golden.rs
  - "crates/fsm-core/tests/fixtures/machines/analyze/**"
status: done
merged_as: ""
---
# Reachability And Completeness

The analyzer's first two claims: an exact enterable-set reachability walk (backed by the lemma that history never extends the reachable set) and the leaf-by-event completeness matrix with chain-level annotations — a plain worklist walk and a plain double loop once the tree tables exist.

**Steps:**

1. Author the reachability fixtures under `crates/fsm-core/tests/fixtures/machines/analyze/` and `crates/fsm-core/tests/analyze_golden.rs` first, encoding exactly the inventory under **Tests**.
2. Implement `Findings`, `Finding { severity, code, message, path, span, hint }`, and the enterable-set walk in `crates/fsm-core/src/analyze.rs` per architecture: seed with the creation entry chain, then repeatedly take any transition whose source is enterable and add its full possible entry set (target path, initial descents, history modeled as the owner's initial chain), until fixed point; unenterable states warn.
3. Implement the completeness matrix: rows = leaves, columns = declared events; each cell `handled@<source_state>` (the innermost chain level declaring a transition for that event) or `unhandled(<policy>)`.
4. Document the reachability lemma as a comment block with its two-sentence proof from architecture.

**Tests:**

- Reachability fixtures asserted by `analyze_golden.rs`: a machine with a state no transition targets and outside every initial chain → warning `def/unreachable_state` naming it (and only it); a machine whose only route into a compound is a history target → the compound's initial-chain states counted enterable (the lemma's modeling), no false warning; a compound entered only via a direct deep-leaf target → that leaf enterable, its *siblings* correctly flagged unenterable.
- `case_review` reachability: every state enterable, zero findings.
- The `case_review` completeness matrix asserted cell-for-cell against a golden table in the test — the four leaf rows × six event columns, including: `docs_review × docs_ok = handled@docs_review` (child level); `docs_review × note_added = handled@in_review` (ancestor level — the `@level` annotation is the point); `docs_review × scored = unhandled(reject)`; `risk_review × scored = handled@risk_review`; `intake × resume = unhandled(reject)`; `suspended × resume = handled@suspended`; the `approved` and `rejected` rows fully `unhandled(reject)`.
- Matrix on an `on_unhandled: ignore` machine (hand-built): unhandled cells render `unhandled(ignore)`.
- Review item (not a unit test): the lemma comment block is present in `analyze.rs` with the two-sentence proof, verified in code review against the architecture wording.

- **Done when:** the reachability fixtures yield exactly their expected findings and the `case_review` completeness matrix matches its golden cell-for-cell under `cargo test -p fsm-core --test analyze_golden`, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
