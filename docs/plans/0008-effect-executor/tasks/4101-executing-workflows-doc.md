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
  - docs/API-POLICY.md
  - README.md
  - crates/fsm-cli/tests/executor_doc.rs
status: done
merged_as: ""
---
# Executing Workflows Doc

The normative operator-facing section: `docs/EMBEDDING.md` gains *Executing workflows*, `docs/API-POLICY.md` gains the row a fifth workspace crate owes its readers, and README gains a 4-line `fsm execute` demo plus one guarantees-area row — pinned by a mechanical test so docs cannot drift from code.

**Steps:**

1. Author `crates/fsm-cli/tests/executor_doc.rs` first, encoding exactly the mechanical inventory under **Tests**.
2. Write the *Executing workflows* section in `docs/EMBEDDING.md` in architecture §0041's order: (1) outbox contract (effects emitted, the executor runs and acks, acks never transition, the advance event is one the machine declares and the table names); (2) the full `fsm.handlers/1` format — every field including `on_ok`/`on_failed` with `event`/`payload`/`stamps`, the no-shell/no-splitting guarantee, default-deny, and the bounded-output digest and lossy-UTF-8 rules; (3) `request_id` derivation and why a restart is safe while a changed result under a recycled key is refused, not replayed; (4) the three modes (`paired` default, `embedded`, `exclusive`), what `fsm serve --read-only` does to the mutating tools named in `MUTATING_TOOLS`, and the sequence that follows — author and trigger through a writer, then let the executor run while the model watches; state plainly that embedded mode ticks only when the client sends a line; (5) honest non-claims — single-node, at-least-once at the process boundary, a handler killed mid-run is re-run by the next executor and its outside-world effect is *not* rolled back by `fsm` (model the undo as a compensating effect the failure path emits), no HA/multi-writer/distribution.
3. The section must mention every `exec/*` code the crate defines (as the doc's own list, kept in sync by the test), without adding them to `fsm_core::error::ALL_CODES`.
4. Add the `fsm-execute` row to `docs/API-POLICY.md`'s per-crate support table, stating whether an embedder may depend on it alongside the existing `fsm-core`/`fsm-store`/`fsm-cli` rows, and update the zero-dependency paragraph's crate list so it matches `zero_deps.rs`.
5. Add to `README.md`: a 4-line `fsm execute` demo mirroring the 60-second MCP demo (naming `--handlers`, the data-dir flag, and the `fsm serve --read-only` pairing) and one row in the guarantees/honest-non-claims area stating the executor is a separate single-node process with at-least-once execution.
6. Keep every example in the repo's neutral business-process vocabulary — no cloud-vendor or product names in the handler samples.

**Tests:**

- `executor_doc.rs`, mechanical — EMBEDDING.md contains the exact string `fsm.handlers/1`, the phrase `at-least-once`, and every `exec/*` code string that `fsm-execute`'s `ALL_CODES` defines (import the const and assert each appears, so adding an `exec/` code without documenting it fails).
- `executor_doc.rs`, mechanical — EMBEDDING.md names every entry of the CLI's `MUTATING_TOOLS` constant (import it and assert each appears), so the documented read-only list cannot drift from the one `dispatch` enforces.
- `executor_doc.rs`, mechanical — README: the `fsm execute` demo fenced block names `--handlers` and the data-dir flag and contains the exact `fsm serve --read-only` pairing line; the new guarantees-area row contains the phrase `single-node`.
- `executor_doc.rs`, mechanical — the mode-decision guidance is present: all three strings `paired`, `embedded`, `exclusive`, and the sentence-fragment naming `paired` the default.
- `executor_doc.rs`, mechanical — API-POLICY.md names `fsm-execute` in its per-crate table.
- Review items (manual, named): the demo commands run verbatim against a built binary; the outbox-contract paragraph reads correctly and does not contradict `docs/SPEC.md`'s effects/deadline sections.

- **Done when:** `cargo test -p fsm-cli --test executor_doc` passes every mechanical assertion (format tag, `exec/*` code coverage, `MUTATING_TOOLS` coverage, README demo flags, mode-default guidance, API-POLICY row), and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
