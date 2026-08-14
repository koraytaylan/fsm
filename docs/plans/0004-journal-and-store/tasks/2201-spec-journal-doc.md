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
status: planned
merged_as: ""
---
# Spec Journal Doc

The journal's normative contract — envelope, kinds, the journaling rule, verification, recovery postures, snapshots — must live in SPEC.md at a fidelity that lets an independent implementation interoperate byte-for-byte.

**Steps:**

1. Append a `## Journal` section to `docs/SPEC.md` covering: the envelope grammar (`{"seq","ts","kind","body","prev","hash"}`, canonical LF lines, domain `fsm:record:1`, genesis with format tag and limits) and the ten record kinds with their body fields.
2. State the journaling rule normatively — a record exists iff the outcome depended on instance state and is not retry-stable — including the `expect_seq` mismatch as the unique admitted retry-stable case (with the dedup-lookup-before-expect_seq order and the double-apply hole it closes) and creation failure as the one unjournaled `run/*` outcome.
3. Document chain verification (byte-canonical storage, seq/prev/hash checks, per-record semantic state-hash re-application), the recovery classifications with their postures (torn-tail quarantine-then-truncate only via explicit repair; interior corruption refused and never rewritten), and the snapshot format with its non-authoritative status.

- **Done when:** `docs/SPEC.md` contains the `## Journal` section covering every listed item with field-level precision, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
