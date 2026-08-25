---
id: executing-workflows-doc
title: "Executing Workflows Doc"
workstream: "0041"
kind: chore
depends_on:
  - golden-two-process-session
  - serve-coordination
gated: false
touches:
  - docs/EMBEDDING.md
  - README.md
  - crates/fsm-cli/tests/executor_doc.rs
status: planned
merged_as: ""
---
# Executing Workflows Doc

The normative operator-facing section: `docs/EMBEDDING.md` gains *Executing workflows* (the outbox contract restated for operators, the `fsm.handlers/1` table format, the idempotency rules, the three run modes, and the honest single-node/at-least-once non-claims), and README gains a 4-line `fsm execute` demo plus one guarantees-area row — pinned by a mechanical test so docs cannot drift from code.

**Steps:**

1. Author `crates/fsm-cli/tests/executor_doc.rs` first, encoding exactly the mechanical inventory under **Tests**.
2. Write the *Executing workflows* section in `docs/EMBEDDING.md` in architecture §0041's order: (1) outbox contract (effects emitted, the executor runs and acks, acks never transition, the advance event is machine-declared); (2) the full `fsm.handlers/1` format — every field, the no-shell/no-splitting guarantee, default-deny, the bounded-output digest rule; (3) `request_id` derivation and why a restart is safe while a changed handler under a recycled effect is refused, not replayed; (4) the three modes (`paired` default, `embedded`, `exclusive`) and the `fsm serve --read-only` effect on the mutating tools; (5) honest non-claims — single-node, at-least-once at the process boundary, external side effects before death are *not* rolled back by `fsm` (model compensation in the machine), no HA/multi-writer/distribution.
3. The section must mention every `exec/*` code the crate defines (as the doc's own list, kept in sync by the test), without adding them to `fsm_core::error::ALL_CODES`.
4. Add to `README.md`: a 4-line `fsm execute` demo mirroring the 60-second MCP demo (naming `--data-dir`, `--handlers`, and the `fsm serve --read-only` pairing) and one row in the guarantees/honest-non-claims area stating the executor is a separate single-node process.

**Tests:**

- `executor_doc.rs`, mechanical — EMBEDDING.md contains the exact string `fsm.handlers/1`, the phrase `at-least-once`, and every `exec/*` code string that `fsm-execute`'s `ALL_CODES` defines (import the const and assert each appears, so adding an `exec/` code without documenting it fails).
- `executor_doc.rs`, mechanical — README: the `fsm execute` demo fenced block names `--data-dir`, `--handlers`, and contains the exact `fsm serve --read-only` pairing line; the new guarantees-area row contains the phrase `single-node`.
- `executor_doc.rs`, mechanical — the mode-decision guidance is present: all three strings `paired`, `embedded`, `exclusive`, and the sentence-fragment naming `paired` the default.
- Review items (manual, named): the demo commands run verbatim against a built binary; the outbox-contract paragraph reads correctly and does not contradict `docs/SPEC.md`'s effects/deadline sections.

- **Done when:** `cargo test -p fsm-cli --test executor_doc` passes every mechanical assertion (format tag, `exec/*` code coverage, README demo flags, mode-default guidance), and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
