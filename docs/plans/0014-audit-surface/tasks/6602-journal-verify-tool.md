---
id: journal-verify-tool
title: "Journal Verify Tool"
workstream: "0066"
kind: task
depends_on:
  - explain-step-tool
gated: false
touches:
  - crates/fsm-store/src/journal_io/mod.rs
  - crates/fsm-cli/src/mcp/tools/dispatch.rs
  - crates/fsm-cli/tests/tool_schemas.rs
  - crates/fsm-cli/tests/mcp_full.rs
  - crates/fsm-cli/tests/mcp_regions_deadlines.rs
  - crates/fsm-cli/tests/naive_caller/core_tests.rs
  - crates/fsm-cli/tests/review_regressions/output_schema_and_wire_format.rs
  - crates/fsm-cli/tests/mcp_affordance_golden.rs
  - crates/fsm-cli/tests/fixtures/
  - docs/EMBEDDING.md
  - crates/fsm-store/src/journal_io/verify.rs
  - crates/fsm-cli/src/mcp/tools/handlers/audit.rs
  - crates/fsm-cli/src/mcp/tools/mod.rs
  - crates/fsm-cli/src/mcp/tools/schema_in.rs
  - crates/fsm-cli/src/mcp/tools/schema_out.rs
  - crates/fsm-cli/tests/audit_verify.rs
status: done
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

**Landed:** `verify_segments_with` is the same loop with a place to stand: every 256 records it hands the caller the running count and the last verified seq, and takes back `Continue` or `Stop`. `verify_segments` is now that function with a callback that never stops, so nothing about what verification *decides* changed — asserted directly, by running both entry points over four fixtures (clean, torn, spaced, rewritten seq) and comparing every segment's status, count and seq range.

`journal_verify(from_seq?, to_seq?)` reports the recovery table's own health name, the records it walked, the classifier's own message, and — where the table prescribes one — the exact remedy command, which it never runs. Interior damage carries SPEC's blast radius in SPEC's words and **no** remedy, because the table says there is none. It joins `PROGRESS_TOOLS`, making it the first genuine consumer of plan 0012's reporter and flag: a token gets a report per batch and a final one, no token gets silence, and a cancelled call stops at a record boundary with `req/cancelled`.

Twenty-one tools measure **32 293** bytes against the 38 000 ceiling, leaving 5 707 for the three still to come.

**Corrections.**

- *The report is a function of a **path**, not of an open `Store`.* The store you most want verified is the one that will not open — `Store::open_read_only` refuses a torn or broken journal outright — so a diagnostic that needed a healthy store to report an unhealthy one would be useless exactly when it is wanted. `tools::verify_report(data_dir, …)` is what the tool calls with `store.data_dir`, and what 6701's degraded mode will call with no store at all. Every damaged-store test in this suite goes through it.
- *`to_seq` stops the walk at a batch boundary, not at the record.* The callback fires every 256 records, so a window inside one batch saves no work; what it does bound exactly is `verified_records`, computed from the window's seqs rather than by a second count. Both are stated in the code.
- *`from_seq` bounds the count, not the start.* A chain is only checkable from its anchor, so the walk always begins at the journal's start.
- *`StateHashMismatch` covers `ReplayMismatch` and `StoreIo` covers `MissingGenesis` and `VersionMismatch`.* The health enum has ten variants and the recovery table names seven postures; the mapping is stated in one place and the classifier's own message says which of the two it was. Step 3's rule — never a new word for an existing condition — is what forced the mapping rather than three more names.
