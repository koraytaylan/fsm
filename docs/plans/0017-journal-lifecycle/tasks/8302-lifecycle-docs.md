---
id: lifecycle-docs
title: "Lifecycle Docs"
workstream: "0083"
kind: task
depends_on:
  - audit-tools-seal
  - archive-crash-harness
  - executor-on-sealed-store
  - readers-on-sealed-store
gated: false
touches:
  - docs/EMBEDDING.md
  - docs/API-POLICY.md
  - README.md
  - crates/fsm-cli/tests/lifecycle_doc.rs
  - crates/fsm-cli/tests/spec_appendix.rs
status: done
merged_as: ""
---
# Lifecycle Docs

Sealing is the first operation in this engine that moves bytes out of the store, so the documentation has to answer the questions an operator will actually have before they run it once.

**Steps:**

1. Add a lifecycle section to `docs/EMBEDDING.md` covering, in order: when to seal, why the cut point must be a `state_checkpoint`, what `store/archive_refused` means and exactly how to clear it, what a sealed store's `verify` says and why that sentence is deliberately not the unsealed one, and the plain statement that the archive is the operator's to keep — `fsm` writes it once and never reads it again unless asked with `--with-archive`.
2. Document the carry rule and its reasoning, not just its behaviour. An operator needs to know that a dropped key cannot be distinguished later from one never seen, which is why keys belonging to live instances are carried whatever their age — and that `store/archive_refused` on this path is a **size** limit cleared by sealing at an earlier cut or letting instances settle, not a veto on having live work.
3. Document the recovery story for each new condition: `store/base_missing` and `store/base_mismatch`, with the honest statement that neither is repairable from the store directory alone and the archive is where the answer is. State it as plainly as the torn-tail remedy is stated.
4. Add to `docs/API-POLICY.md`: store `VERSION` 10, the two new hash domains, the three new format strings, the error codes, and — as its own sentence, not a clause — **a 0.3.0 store is not readable by 0.2.x, sealed or not**, because the version stamp moves on first write. There are **five** new codes rather than three: `store/sealed_replay_unavailable` came from the eighth record-scanning reader `8101` found, and `req/instance_exists` from the `create` guard `7903`'s safety argument turned out to need.
5. Document what pins an archive: a **pending effect** holds the records its execution is derived from, so a store with work mid-flight seals lower than one at rest, and `--dry-run` names the highest cut available. This is the question an operator asks when a seal moves less than they expected.
6. Add one row to `README.md`'s guarantee table for bounded retention, phrased as the guarantee it is: a sealed prefix is relocated, never rewritten, and remains checkable. Extend the honest non-claims paragraph with the two things sealing does not do — it does not delete, and it does not run on a timer.
7. Do not describe archival as compaction anywhere. Nothing is compacted; bytes are relocated unchanged, and the distinction is the reason the archive is still evidence.
8. Create `crates/fsm-cli/tests/lifecycle_doc.rs` asserting the documentation against the code rather than against a reviewer's memory, in the shape `executor_doc.rs` established.

**Tests:**

- `crates/fsm-cli/tests/lifecycle_doc.rs`: every `store/*` code this plan added appears in `EMBEDDING.md`, asserted against `fsm_core::error::ALL_CODES` so a new code cannot ship undocumented.
- The three new format strings and the two new domain constants appear in `API-POLICY.md`, asserted against the constants themselves rather than as literals in the test.
- `EMBEDDING.md` states the checkpoint-cut requirement, the carry-rule reasoning, and the no-repair position for `base_mismatch` — one assertion each, against phrases the test pins.
- `API-POLICY.md` contains the 0.2.x incompatibility sentence.
- `README.md`'s guarantee table gained exactly one row and the non-claims paragraph names both omissions.
- `EMBEDDING.md` states that a pending effect pins the cut and that `--dry-run` names the highest available one.
- The word "compaction" appears nowhere in the new prose.
- `cargo test -p fsm-cli --test spec_appendix` passes with every code, format, and domain documented.
- `cargo test -p fsm-cli --test examples` passes, and the documented command lines are the commands the binary actually accepts — assert by running each documented invocation's `--help` form.

- **Done when:** `cargo test -p fsm-cli --test lifecycle_doc --test spec_appendix --test examples` passes, every new code, format, and domain is asserted present against the constants rather than against literals, the three contested explanations are pinned by tests, the 0.2.x incompatibility is stated as its own sentence, and `cargo test`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo fmt --check` succeed.
