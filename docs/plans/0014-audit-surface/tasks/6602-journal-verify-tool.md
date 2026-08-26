---
id: journal-verify-tool
title: "Journal Verify Tool"
workstream: "0066"
kind: task
depends_on:
  - explain-step-tool
gated: false
touches:
  - crates/fsm-store/src/journal_io/verify.rs
  - crates/fsm-cli/src/mcp/tools/handlers/audit.rs
  - crates/fsm-cli/src/mcp/tools/mod.rs
  - crates/fsm-cli/src/mcp/tools/schema_in.rs
  - crates/fsm-cli/src/mcp/tools/schema_out.rs
  - crates/fsm-cli/tests/audit_verify.rs
status: planned
merged_as: ""
---
# Journal Verify Tool

The README claims tamper-evident history and this is the operation that checks it, so a model that cannot run it is taking the central guarantee on faith from the thing making the claim.

**Steps:**

1. In `crates/fsm-store/src/journal_io/verify.rs`, add the **incremental seam**: an entry point taking a callback invoked every N records (N = 256) which can request cancellation. Reimplement the existing all-at-once entry point in terms of it, so `fsm journal verify` and every existing test keep their exact behaviour and only *how* the answer is produced changes.
2. Do not change a single conclusion. Same health vocabulary, same first-bad-seq, same blast radius. If you find yourself editing what verification decides, stop — that is outside this plan.
3. Add `journal_verify(from_seq?, to_seq?)` to the registry, returning `{health, verified_records, first_bad_seq?, blast_radius?, remedy?}`. The health values are exactly the seven `docs/SPEC.md §Recovery` names — `Ok`, `TornTail`, `ChainBroken`, `StateHashMismatch`, `NonCanonical`, `LockIo`, `StoreIo` — and never a new word for an existing condition.
4. Populate `remedy` from SPEC's recovery table, verbatim: the exact command a human should run, or absent when the posture is "no repair". The tool never runs it.
5. Wire plan 0012's `ProgressReporter` at the callback so a call carrying a `progressToken` reports as it goes, and the `CancelFlag` so a cancelled call returns `req/cancelled` at a record boundary. This is the first genuine consumer of both.
6. Honour `from_seq` and `to_seq` so a caller can check a window rather than a whole store, and report `verified_records` as the count actually walked rather than the journal length.
7. Keep it out of `MUTATING_TOOLS` and read it through `Store::open_read_only`, so it takes no lock and is safe beside a live writer.

**Tests:**

- `crates/fsm-cli/tests/audit_verify.rs`: a healthy store reports `Ok` with `verified_records` equal to the journal length and no `first_bad_seq`.
- A store with a byte flipped inside a record reports `NonCanonical` naming the seq, with SPEC's remedy posture.
- A truncated final line reports `TornTail` with the `fsm repair --truncate-torn-tail` remedy string.
- A rewritten `prev` reports `ChainBroken` with the blast radius SPEC prescribes and **no** remedy.
- `from_seq`/`to_seq` bound the walk, and `verified_records` reflects the window.
- A call with a `progressToken` emits progress notifications and a final report; without one it emits none.
- A cancelled call returns `req/cancelled` at a record boundary and does not walk the rest.
- The tool takes no lock: verification succeeds while a writable `Store` is open in the same test.
- **Behaviour parity:** `fsm journal verify` produces the same health and first-bad-seq as before the seam was introduced, for all four fixtures.
- The tool is absent from `MUTATING_TOOLS` and works on a read-only server.

- **Done when:** `cargo test -p fsm-cli --test audit_verify` and `cargo test -p fsm-store` pass, the incremental seam changes no conclusion, progress and cancellation are wired, remedies match SPEC verbatim, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
