---
id: spec-journal-doc
title: "Spec Journal Doc"
workstream: "0022"
kind: chore
depends_on:
  - instance-store
gated: false
touches:
  - docs/SPEC.md
status: done
merged_as: ""
---
# Spec Journal Doc

The journal's normative contract — envelope, kinds, the journaling rule, verification, recovery postures, snapshots — must live in SPEC.md at a fidelity that lets an independent implementation interoperate byte-for-byte.

**Steps:**

1. Append a `## Journal` section to `docs/SPEC.md` covering: the envelope grammar (`{"seq","ts","kind","body","prev","hash"}`, canonical LF lines, domain `fsm:record:1`, genesis with format tag and limits) and the ten record kinds with their body fields.
2. State the journaling rule normatively — a record exists iff the outcome depended on instance state and is not retry-stable — including the `expect_seq` mismatch as the unique admitted retry-stable case (with the dedup-lookup-before-expect_seq order and the double-apply hole it closes) and creation failure as the one unjournaled `run/*` outcome.
3. Document chain verification (byte-canonical storage, seq/prev/hash checks, per-record semantic state-hash re-application), the recovery classifications with their postures (torn-tail quarantine-then-truncate only via explicit repair; interior corruption refused and never rewritten), and the snapshot format with its non-authoritative status.

**Tests:**

- No unit tests exist for prose; acceptance is this verification checklist, applied by the implementer and the reviewer against the landed code:
- Kind completeness (mechanical): every variant of `RecordKind` in `crates/fsm-core/src/record.rs` appears by its serialized snake_case name in the section, each with its body fields — checked by grepping the ten names against the doc; zero misses, zero extras.
- Envelope fidelity (mechanical): every field the doc names is observable in a real line of `crates/fsm-core/tests/fixtures/records/chain_golden.jsonl`, and the doc names no field absent from those lines.
- Classification parity (mechanical): the documented recovery classifications map one-to-one onto the `JournalHealth` variants in `journal_io.rs`, with the torn-tail entry naming the literal `fsm repair --truncate-torn-tail` command and the interior entries stating no repair exists.
- Rule completeness (review): the journaling rule appears with both named exceptions — the `expect_seq` retry-stable case including the check order and the double-apply hole it closes, and `run/create_failed` as the one unjournaled `run/*` outcome — stated normatively, not as commentary.
- Snapshot parity (mechanical): the documented snapshot body keys match the keys written by the snapshot writer in `store.rs`, including the self-hash domain `fsm:snapshot:1` and the non-authoritative status.
- Interop spot-check (review ritual, the section's bar): following only the doc, a reviewer recomputes the `hash` of the second record of `chain_golden.jsonl` and gets the committed value — the byte-for-byte interoperability claim exercised once by hand.

- **Done when:** `docs/SPEC.md` contains the `## Journal` section covering every listed item with field-level precision, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
